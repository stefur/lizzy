use core::time::Duration;
use std::error::Error;
use std::rc::Rc;

use dbus::{
    arg,
    arg::{Iter, PropMap, RefArg, TypeMismatchError, Variant},
    blocking::{BlockingSender, LocalConnection},
    message::MatchRule,
    strings::BusName,
    Error as DBusError, Message,
    MessageType::Signal,
};

use options::Arguments;
mod options;

struct NameOwnerChanged {
    name: String,
    _old_name: String,
    new_name: String,
}

struct Song {
    artist: Option<String>,
    title: Option<String>,
    playbackstatus: String,
}
enum Contents {
    PlaybackStatus(Option<String>),
    Metadata {
        artist: Option<Vec<String>>,
        title: Option<String>,
    },
}

struct Output {
    now_playing: String,
    playbackstatus: String,
}

impl Output {
    /// Create the output according to defined format
    fn new(song: Song, output_format: &str) -> Output {
        let song_artist = song.artist.as_deref().unwrap_or("");
        let song_title = song.title.as_deref().unwrap_or("");

        let now_playing = output_format
            .replace("{{artist}}", song_artist)
            .replace("{{title}}", song_title);

        Output {
            playbackstatus: song.playbackstatus,
            now_playing,
        }
    }

    /// Waybar doesn't like ampersand. So we replace them in the output string.
    fn escape_ampersand(&mut self) -> &mut Self {
        self.now_playing = self.now_playing.replace('&', "&amp;");
        self
    }

    /// Print the output to Waybar
    fn send(&self) {
        println!(
            r#"{{"text": "{}", "alt": "{}", "class": "{}"}}"#,
            self.now_playing, self.playbackstatus, self.playbackstatus
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse the options for use within the match rules
    let options: options::Arguments = match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };

    let properties_options = Rc::new(options);
    let nameowner_options = Rc::clone(&properties_options);

    let conn = LocalConnection::new_session().expect("Failed to connect to the session bus.");

    let properties_rule = MatchRule::new()
        .with_path("/org/mpris/MediaPlayer2")
        .with_member("PropertiesChanged")
        .with_interface("org.freedesktop.DBus.Properties")
        .with_type(Signal);

    let nameowner_rule = MatchRule::new()
        .with_path("/org/freedesktop/DBus")
        .with_member("NameOwnerChanged")
        .with_interface("org.freedesktop.DBus")
        .with_sender("org.freedesktop.DBus")
        .with_type(Signal);

    // Handles the incoming signals from  when properties change
    conn.add_match(properties_rule, move |_: (), conn, msg| {
        // Start by checking if the signal is indeed from the mediaplayer we want
        if is_mediaplayer(conn, msg, &properties_options.mediaplayer) {
            handle_valid_mediaplayer_signal(conn, msg, &properties_options);
        } else if should_toggle_playback(conn, &properties_options) {
            toggle_playback_if_needed(conn, msg, &properties_options);
        }
        true
    })?;

    // Handles any incoming messages when a nameowner has changed.
    conn.add_match(nameowner_rule, move |_: (), _, msg| {
        // Check if we should listen to all mediaplayers
        if nameowner_options.mediaplayer.is_empty() {
            return true;
        }

        if let Ok(nameowner) = read_nameowner(msg) {
            // If the mediaplayer has been closed, clear the output
            if nameowner
                .name
                .to_lowercase()
                .contains(&nameowner_options.mediaplayer)
                && nameowner.new_name.is_empty()
            {
                println!();
            }
        }

        true
    })?;

    loop {
        conn.process(Duration::from_millis(1000))
            .expect("lizzy should be able to set up a loop to listen for messages.");
    }
}

/// This unpacks a message containing NameOwnerChanged. The field old_name is in fact never used.
fn read_nameowner(msg: &Message) -> Result<NameOwnerChanged, TypeMismatchError> {
    let mut iter: Iter = msg.iter_init();
    Ok(NameOwnerChanged {
        name: iter.read()?,
        _old_name: iter.read()?,
        new_name: iter.read()?,
    })
}

/// Parses a message and returns its contents
fn parse_message(msg: &Message) -> Option<Contents> {
    // Read the two first arguments of the received message
    if let Ok((_, map)) = msg.read2::<String, PropMap>() {
        if let Some(contents) = map.keys().next() {
            match contents.as_str() {
                "Metadata" => {
                    let metadata = &map["Metadata"].0;
                    let property_map: Option<&arg::PropMap> = arg::cast(metadata);
                    let song_title =
                        property_map.and_then(|m| arg::prop_cast::<String>(m, "xesam:title"));
                    let song_artist =
                        property_map.and_then(|m| arg::prop_cast::<Vec<String>>(m, "xesam:artist"));

                    if let (Some(title), Some(artist)) = (song_title, song_artist) {
                        return Some(Contents::Metadata {
                            artist: Some(artist.to_owned()),
                            title: Some(title.to_owned()),
                        });
                    }
                }
                "PlaybackStatus" => {
                    if let Some(playbackstatus) = map["PlaybackStatus"].0.as_str() {
                        return Some(Contents::PlaybackStatus(Some(playbackstatus.to_string())));
                    }
                }

                _ => (),
            }
        }
    }
    None
}

/// Calls a method on the interface to play or pause what is currently playing
fn toggle_playback(conn: &LocalConnection, mediaplayer: &str, cmd: &str) {
    let message = dbus::Message::new_method_call(
        mediaplayer,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
        cmd,
    );

    match message {
        Ok(message) => match conn.send_with_reply_and_block(message, Duration::from_millis(5000)) {
            Ok(_) => (),
            Err(err) => eprintln!("Failed to toggle playback. Error: {}", err),
        },
        Err(err) => eprintln!("Failed to create method call. Error: {}", err),
    }
}

fn handle_valid_mediaplayer_signal(
    conn: &LocalConnection,
    msg: &Message,
    properties_opts: &Arguments,
) {
    let contents = parse_message(msg);

    let song = match contents {
        Some(Contents::Metadata { artist, title }) => Song {
            artist: artist.unwrap_or_default().get(0).cloned(),
            title,
            playbackstatus: get_property(conn, msg.sender(), "PlaybackStatus")
                .0
                .unwrap_or_default(),
        },
        Some(Contents::PlaybackStatus(status)) => {
            let metadata = get_property(conn, msg.sender(), "Metadata");
            Song {
                artist: metadata.0,
                title: metadata.1,
                playbackstatus: status.unwrap_or_default(),
            }
        }
        None => return, // Ignore messages with no valid content
    };

    let mut output = Output::new(song, &properties_opts.format);
    output.escape_ampersand().send();
}

fn should_toggle_playback(conn: &LocalConnection, properties_opts: &Arguments) -> bool {
    properties_opts.autotoggle && query_id(conn, &properties_opts.mediaplayer).is_some()
}

fn toggle_playback_if_needed(conn: &LocalConnection, msg: &Message, properties_opts: &Arguments) {
    let other_media = parse_message(msg);
    let status = match other_media {
        Some(Contents::PlaybackStatus(status)) => status.unwrap_or_default(),
        Some(Contents::Metadata { .. }) => get_property(conn, msg.sender(), "PlaybackStatus")
            .0
            .unwrap_or_default(),
        _ => return, // Ignore messages with no valid content
    };

    let mediaplayer = match query_id(conn, &properties_opts.mediaplayer) {
        Some(mediaplayer) => mediaplayer,
        None => return, // No valid mediaplayer found
    };

    match status.as_str() {
        "Playing" => toggle_playback(conn, &mediaplayer, "Pause"),
        "Paused" | "Stopped" | "" => toggle_playback(conn, &mediaplayer, "Play"),
        _ => {
            println!("Failed to match the playbackstatus");
        }
    }
}

/// Make a method call to get a property value from the mediaplayer
fn get_property(
    conn: &LocalConnection,
    busname: Option<BusName>,
    property: &str,
) -> (Option<String>, Option<String>) {
    if let Some(mediaplayer) = busname {
        let interface = "org.mpris.MediaPlayer2.Player";
        let path = "/org/mpris/MediaPlayer2";
        let message = dbus::Message::call_with_args(
            mediaplayer,
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
            (interface, property),
        );

        let (artist, title) =
            match conn.send_with_reply_and_block(message, Duration::from_millis(5000)) {
                Ok(reply) => match property {
                    "PlaybackStatus" => {
                        let result: Variant<String> =
                            reply.read1().unwrap_or_else(|_| Variant(String::new()));
                        (Some(result.0), None)
                    }
                    "Metadata" => {
                        let metadata: Result<Variant<PropMap>, TypeMismatchError> = reply.read1();
                        match metadata {
                            Ok(metadata) => {
                                let properties: PropMap = metadata.0;
                                let title: Option<String> =
                                    arg::prop_cast(&properties, "xesam:title").cloned();
                                let artist: Option<Vec<String>> =
                                    arg::prop_cast(&properties, "xesam:artist").cloned();
                                (artist.unwrap_or_default().get(0).cloned(), title)
                            }
                            Err(err) => {
                                eprintln!("Failed to get metadata. Error: {}", err);
                                (None, None)
                            }
                        }
                    }
                    _ => (None, None),
                },
                Err(_) => (None, None),
            };

        (artist, title)
    } else {
        (None, None)
    }
}

/// Create a query with a method call to ask for the ID of the mediaplayer
fn query_id(conn: &LocalConnection, mediaplayer: &str) -> Option<String> {
    let query = dbus::Message::call_with_args(
        "org.freedesktop.DBus",
        "/",
        "org.freedesktop.DBus",
        "GetNameOwner",
        (format!("org.mpris.MediaPlayer2.{}", mediaplayer),),
    );

    // Send the query and await the reply
    let reply: Result<Message, DBusError> =
        conn.send_with_reply_and_block(query, Duration::from_millis(5000));

    match reply {
        // If we get a reply, we unpack the ID from the message and return it
        Ok(reply) => Some(reply.read1().unwrap_or_default()),

        // If the message is not from the mediaplayer we're listening to, we'll receive an error in return, which is fine, so return None
        Err(_) => None,
    }
}

/// Check if the sender of a message is the mediaplayer we're listening to
fn is_mediaplayer(conn: &LocalConnection, msg: &Message, mediaplayer: &str) -> bool {
    // If mediaplayer option is blank, we listen to all incoming signals and thus return true
    if mediaplayer.is_empty() {
        return true;
    }

    // Extract the sender of our incoming message
    let sender_id = msg.sender().map_or(String::new(), |s| s.to_string());

    // Check if the sender matches the specified mediaplayer
    match query_id(conn, mediaplayer) {
        Some(mediaplayer_id) => sender_id == mediaplayer_id,
        None => false,
    }
}
