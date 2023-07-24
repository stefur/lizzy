use core::panic;
use std::error::Error;
use std::time::Duration;

use dbus::arg::{Iter, PropMap, RefArg, TypeMismatchError, Variant};
use dbus::blocking::BlockingSender;
use dbus::message::MatchRule;
use dbus::strings::BusName;
use dbus::Error as DBusError;
use dbus::Message;
use dbus::MessageType::Signal;
use dbus::{arg, blocking::LocalConnection};

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
        let song_artist = song.artist.as_deref().unwrap_or_else(|| "").to_string();
        let song_title = song.title.as_deref().unwrap_or_else(|| "").to_string();

        let text = output_format
            .replace("{{artist}}", &song_artist)
            .replace("{{title}}", &song_title);

        Output {
            playbackstatus: song.playbackstatus,
            now_playing: text, // The complete text
        }
    }

    /// Waybar doesn't like ampersand. So we replace them in the output string.
    fn escape_ampersand(&mut self) -> &mut Self {
        self.now_playing = str::replace(&self.now_playing, "&", "&amp;");
        self
    }

    fn send(&self) {
        println!(
            r#"{{"text": "{}", "alt": "{}", "class": "{}"}}"#,
            self.now_playing, self.playbackstatus, self.playbackstatus
        );
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse the options for use within the match rule for property changes
    let properties_opts: options::Arguments = match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };
    // And a clone used for nameowner changes
    let nameowner_opts = properties_opts.clone();

    let conn =
        LocalConnection::new_session().expect("Lystra should be able to connect to session bus.");

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
        if is_mediaplayer(conn, msg, &properties_opts.mediaplayer) {
            // Unpack the song received from the signal to create an output
            let contents = parse_message(msg);

            let song: Song = match contents {
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
                None => {
                    // Just return if we fail to find a match.
                    // For now this assumes messages we're not interested in.
                    return true;
                }
            };

            // Create an output from the song
            let mut output = Output::new(song, &properties_opts.format);

            // Customize the output and send it
            output.escape_ampersand().send();
        } else {
            // First we check that our mediaplayer is even running and that the autotoggle flag is used
            if let (Some(mediaplayer), true) = (
                query_id(conn, &properties_opts.mediaplayer),
                properties_opts.autotoggle,
            ) {
                // If the other mediaplayer wasn't closed after sending its signal we parse the message
                let other_media = parse_message(msg);
                let status: String = match other_media {
                    Some(Contents::PlaybackStatus(status)) => status.unwrap_or_default(),
                    // If we receive a message containing only metadata, we manually check the playback status
                    // since a message with metadata also means me is playing
                    Some(Contents::Metadata { .. }) => {
                        get_property(conn, msg.sender(), "PlaybackStatus")
                            .0
                            .unwrap_or_default()
                    }
                    // Ignore anything else
                    _ => return true,
                };

                match status.as_str() {
                    "Playing" => {
                        toggle_playback(conn, &mediaplayer, "Pause");
                    }
                    "Paused" | "Stopped" | "" => {
                        toggle_playback(conn, &mediaplayer, "Play");
                    }
                    _ => {
                        println!("Failed to match the playbackstatus");
                        return true;
                    }
                }
            }
        }
        true
    })?;

    // Handles any incoming messages when a nameowner has changed.
    conn.add_match(nameowner_rule, move |_: (), _, msg| {
        // Not a very pretty solution, but if we listen to signals coming from all mediaplayers,
        // we never clear the output to avoid clearing the output in error due to mediaplayers closing
        if nameowner_opts.mediaplayer.is_empty() {
            return true;
        }

        if let Ok(nameowner) = read_nameowner(msg) {
            // If mediaplayer has been closed, clear the output by sending an empty string
            if nameowner
                .name
                .to_lowercase()
                .contains(nameowner_opts.mediaplayer.as_str())
                && nameowner.new_name.is_empty()
            {
                println!("");
            }
        }
        true
    })?;

    loop {
        conn.process(Duration::from_millis(1000))
            .expect("Lystra should be able to set up a loop to listen for messages.");
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
    let read_msg: Result<(String, PropMap), TypeMismatchError> = msg.read2();

    match read_msg {
        Ok(read_msg) => {
            // Get the HashMap from the second argument, which contains the relevant info
            let map = read_msg.1;

            // The string that tells us what kind of contents is in the message
            if let Some(contents) = map.keys().next() {
                match contents.as_str() {
                    "Metadata" => {
                        let metadata: &dyn RefArg = &map["Metadata"].0;
                        let property_map: Option<&arg::PropMap> = arg::cast(metadata);

                        match property_map {
                            Some(property_map) => {
                                let song_title: Option<&String> =
                                    arg::prop_cast(property_map, "xesam:title");

                                let song_artist: Option<&Vec<String>> =
                                    arg::prop_cast(property_map, "xesam:artist");

                                let result = Contents::Metadata {
                                    artist: song_artist.cloned(),
                                    title: song_title.cloned(),
                                };
                                Some(result)
                            }
                            None => None,
                        }
                    }
                    "PlaybackStatus" => map["PlaybackStatus"].0.as_str().map(|playbackstatus| {
                        Contents::PlaybackStatus(Some(playbackstatus.to_string()))
                    }),
                    &_ => None,
                }
            } else {
                None
            }
        }
        Err(err) => {
            eprintln!("Hit an error: {}", err);
            panic!("Aborting.")
        }
    }
}

/// Calls a method on the interface to play or pause what is currently playing
fn toggle_playback(conn: &LocalConnection, mediaplayer: &String, cmd: &str) {
    if let Ok(message) = dbus::Message::new_method_call(
        mediaplayer,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
        cmd,
    ) {
        match conn.send_with_reply_and_block(message, Duration::from_millis(5000)) {
            Ok(_) => (),
            Err(err) => eprintln!("Failed to toggle playback. Error: {}", err),
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
        let message = dbus::Message::call_with_args(
            mediaplayer,
            "/org/mpris/MediaPlayer2",
            "org.freedesktop.DBus.Properties",
            "Get",
            ("org.mpris.MediaPlayer2.Player", property),
        );

        let reply: Result<Message, DBusError> =
            conn.send_with_reply_and_block(message, Duration::from_millis(5000));

        match (reply, property) {
            (Ok(reply), "PlaybackStatus") => {
                // Unpack the playback status, or return an empty string which is rare, but happens
                let result: Variant<String> = reply.read1().unwrap_or(Variant(String::new()));

                (Some(result.0), None)
            }
            (Ok(reply), "Metadata") => {
                let metadata: Result<Variant<PropMap>, TypeMismatchError> = reply.read1();

                match metadata {
                    Ok(metadata) => {
                        let properties: PropMap = metadata.0;
                        let title: Option<String> =
                            arg::prop_cast(&properties, "xesam:title").cloned();
                        let artist: Option<Vec<String>> =
                            arg::prop_cast(&properties, "xesam:artist").cloned();
                        let artist = artist.unwrap_or_default().get(0).cloned();
                        let result: (Option<String>, Option<String>) = (artist, title);

                        result
                    }

                    Err(err) => {
                        // We print an error message if there is an issue with parsing the reply.
                        // Returning none, no need to panic. This should rarely, if ever, happen.
                        // Error might still be useful though.
                        eprintln!("Failed to get metadata. Error: {}", err);
                        (None, None)
                    }
                }
            }
            (Ok(_reply), &_) => (None, None),
            // Return none in case we receive an error.
            // This is usually cause the mediaplayer closed after the signal was sent.
            (Err(_err), _) => (None, None),
        }
    } else {
        (None, None)
    }
}

/// Create a query with a method call to ask for the ID of the mediaplayer
fn query_id(conn: &LocalConnection, mediaplayer: &String) -> Option<String> {
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
        Ok(reply) => {
            let mediaplayer_id: String = reply.read1().unwrap_or_default();

            Some(mediaplayer_id)
        }

        // If the message is not from the mediaplayer we're listening to we'll receive an error in return, which is fine, so return false
        Err(_reply) => None,
    }
}

/// Check if the sender of a message is the mediaplayer we're listening to
fn is_mediaplayer(conn: &LocalConnection, msg: &Message, mediaplayer: &String) -> bool {
    // If mediaplayer option is blank, we listen to all incoming signals and thus return true
    if mediaplayer.is_empty() {
        return true;
    }

    // Extract the sender of our incoming message
    let sender_id = msg
        .sender()
        .unwrap_or_else(|| String::new().into())
        .to_string();

    // Send the query and await the reply
    if let Some(mediaplayer_id) = query_id(conn, mediaplayer) {
        sender_id == mediaplayer_id
    } else {
        false
    }
}
