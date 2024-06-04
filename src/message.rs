use crate::options::Arguments;
use crate::output::BarOutput;
use dbus::{
    arg,
    arg::{Iter, PropMap, RefArg, TypeMismatchError, Variant},
    blocking::{BlockingSender, LocalConnection},
    strings::BusName,
    Error as DBusError, Message,
};
use std::fmt;
use std::time::Duration;

pub struct Media {
    pub artist: String,
    pub title: String,
    pub playbackstatus: String,
}

pub struct NameOwnerChanged {
    pub name: String,
    pub _old_name: String,
    pub new_name: String,
}

pub enum Contents {
    PlaybackStatus(String),
    Metadata { artist: Vec<String>, title: String },
}

#[derive(Debug)]
pub enum MessageError {
    Parsing,
    GetProperty,
    MessageCreation,
    MethodCall,
}

impl std::error::Error for MessageError {}

impl fmt::Display for MessageError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MessageError::Parsing => write!(f, "Failed to parse message."),
            MessageError::GetProperty => {
                write!(f, "Failed to get property.")
            }
            MessageError::MessageCreation => {
                write!(f, "Failed to create a message with arguments.")
            }
            MessageError::MethodCall => {
                write!(f, "Failed to make a method call.")
            }
        }
    }
}

/// This unpacks a message containing NameOwnerChanged. The field old_name is in fact never used.
pub fn read_nameowner(msg: &Message) -> Result<NameOwnerChanged, TypeMismatchError> {
    let mut iter: Iter = msg.iter_init();
    Ok(NameOwnerChanged {
        name: iter.read()?,
        _old_name: iter.read()?,
        new_name: iter.read()?,
    })
}

/// Parses a message and returns its contents
fn parse_message(msg: &Message) -> Result<Contents, MessageError> {
    // Read the two first arguments of the received message
    if let Ok((_, map)) = msg.read2::<String, PropMap>() {
        if let Some(contents) = map.keys().next() {
            match contents.as_str() {
                "Metadata" => {
                    let metadata = &map["Metadata"].0;
                    let property_map: Option<&arg::PropMap> = arg::cast(metadata);
                    let media_title =
                        property_map.and_then(|m| arg::prop_cast::<String>(m, "xesam:title"));
                    let media_artist =
                        property_map.and_then(|m| arg::prop_cast::<Vec<String>>(m, "xesam:artist"));

                    if let (Some(title), Some(artist)) = (media_title, media_artist) {
                        return Ok(Contents::Metadata {
                            artist: artist.to_owned(),
                            title: title.to_owned(),
                        });
                    }
                }
                "PlaybackStatus" => {
                    if let Some(playbackstatus) = map["PlaybackStatus"].0.as_str() {
                        return Ok(Contents::PlaybackStatus(playbackstatus.to_string()));
                    }
                }

                _ => (),
            }
        }
    }
    Err(MessageError::Parsing)
}

/// Make a method call to get a property value from the mediaplayer
pub fn get_property(
    conn: &LocalConnection,
    busname: Option<BusName>,
    property: &str,
) -> Result<Contents, MessageError> {
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

        if let Ok(reply) = conn.send_with_reply_and_block(message, Duration::from_millis(5000)) {
            match property {
                "PlaybackStatus" => {
                    let result: Variant<String> =
                        reply.read1().unwrap_or_else(|_| Variant(String::new()));
                    return Ok(Contents::PlaybackStatus(result.0));
                }
                "Metadata" => {
                    let metadata: Result<Variant<PropMap>, TypeMismatchError> = reply.read1();
                    if let Ok(metadata) = metadata {
                        let properties: PropMap = metadata.0;
                        let title: Option<String> =
                            arg::prop_cast(&properties, "xesam:title").cloned();
                        let artist: Option<Vec<String>> =
                            arg::prop_cast(&properties, "xesam:artist").cloned();
                        return Ok(Contents::Metadata {
                            artist: artist.unwrap_or_default(),
                            title: title.unwrap_or_default(),
                        });
                    }
                }
                &_ => {}
            }
        } else {
            return Err(MessageError::MethodCall);
        }
    } else {
        return Err(MessageError::MessageCreation);
    }

    Err(MessageError::GetProperty)
}

/// Create a query with a method call to ask for the ID of the mediaplayer
pub fn query_id(conn: &LocalConnection, mediaplayer: &str) -> Result<String, MessageError> {
    // Create a query to get all names available
    let query = Message::call_with_args(
        "org.freedesktop.DBus",
        "/",
        "org.freedesktop.DBus",
        "ListNames",
        (),
    );

    // Send the query and await the reply
    let reply: Result<Message, DBusError> =
        conn.send_with_reply_and_block(query, Duration::from_millis(5000));

    // Go over the names in the message
    if let Ok(message) = reply {
        // Unpack the vector of names
        let names = message.read1::<Vec<String>>().unwrap_or_default();

        // Get the name owners on org.mpris.MediaPlayer2, e.g. MPRIS players and keep only the identifier
        let mpris_names: Vec<String> = names
            .into_iter()
            .filter(|name| name.starts_with("org.mpris.MediaPlayer2."))
            .map(|name| {
                name.trim_start_matches("org.mpris.MediaPlayer2.")
                    .to_string()
            })
            .collect();

        // Now iterate over the names that is an MPRIS player
        for name in mpris_names {
            if matches_pattern(mediaplayer, &name) {
                // If a match is found, get the owner ID of the player
                let query = Message::call_with_args(
                    "org.freedesktop.DBus",
                    "/",
                    "org.freedesktop.DBus",
                    "GetNameOwner",
                    (format!("org.mpris.MediaPlayer2.{}", &name),),
                );

                // Send the query and get the response
                let reply = conn.send_with_reply_and_block(query, Duration::from_millis(2000));

                // And get the ID
                if let Ok(message) = reply {
                    let id = message.read1::<String>().unwrap_or_default();
                    return Ok(id);
                }
            }
        }
    }
    // Should probably handle this some other way, but we basically ignore errors
    Err(MessageError::MethodCall)
}

/// Simple glob pattern check for when mediaplayer names vary, like Firefox does for each instance
pub fn matches_pattern(mediaplayer: &str, sender: &str) -> bool {
    // Check if mediaplayer option contains any glob pattern characters
    if mediaplayer.contains('*') {
        match mediaplayer {
            mp if mp.starts_with('*') && mp.ends_with('*') && mp.len() > 2 => {
                let infix = &mp[1..mp.len() - 1];
                sender.contains(infix)
            }
            mp if mp.ends_with('*') => {
                let prefix = &mp[..mp.len() - 1];
                sender.starts_with(prefix)
            }
            mp if mp.starts_with('*') => {
                let suffix = &mp[1..];
                sender.ends_with(suffix)
            }
            _ => false, // This case should not occur really, but got to handle it
        }
    } else {
        // If no glob wildcard, directly compare with sender instead
        mediaplayer == sender
    }
}

/// Check if the sender of a message is the mediaplayer we're listening to
pub fn is_mediaplayer(conn: &LocalConnection, msg: &Message, mediaplayer: &str) -> bool {
    // If mediaplayer option is blank, we listen to all incoming signals and thus return true
    if mediaplayer.is_empty() {
        return true;
    }

    // Extract the sender of our incoming message
    let sender_id = msg.sender().map_or(String::new(), |s| s.to_string());

    // Check if the sender matches the specified mediaplayer
    if let Ok(mediaplayer_id) = query_id(conn, mediaplayer) {
        sender_id == mediaplayer_id
    } else {
        false
    }
}

/// Calls a method on the interface to play or pause what is currently playing
pub fn toggle_playback(conn: &LocalConnection, mediaplayer: &str, cmd: &str) {
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

/// Unpack the message and return media metadata contents to use as output in Waybar
pub fn handle_valid_mediaplayer_signal(
    conn: &LocalConnection,
    msg: &Message,
    properties_opts: &Arguments,
) {
    let contents = parse_message(msg);

    let media = match contents {
        Ok(Contents::Metadata { artist, title }) => {
            let playbackstatus = get_property(conn, msg.sender(), "PlaybackStatus");
            if let Ok(Contents::PlaybackStatus(playbackstatus)) = playbackstatus {
                Media {
                    artist: artist.first().cloned().unwrap_or_default(),
                    title,
                    playbackstatus,
                }
            } else {
                return;
            }
        }
        Ok(Contents::PlaybackStatus(playbackstatus)) => {
            let metadata = get_property(conn, msg.sender(), "Metadata");
            if let Ok(Contents::Metadata { artist, title }) = metadata {
                Media {
                    artist: artist.first().cloned().unwrap_or_default(),
                    title,
                    playbackstatus,
                }
            } else {
                return;
            }
        }
        Err(_) => return, // Ignore messages with no valid content
    };

    let mut output = BarOutput::new(media, &properties_opts.format);
    output.escape_ampersand().send();
}

/// Check if we should toggle the playback
pub fn should_toggle_playback(conn: &LocalConnection, properties_opts: &Arguments) -> bool {
    properties_opts.autotoggle && query_id(conn, &properties_opts.mediaplayer).is_ok()
}

/// Toggle playback of the mediaplayer when another player sends a play/pause message
pub fn toggle_playback_if_needed(
    conn: &LocalConnection,
    msg: &Message,
    properties_opts: &Arguments,
) {
    let other_media = parse_message(msg);
    let status = match other_media {
        Ok(Contents::PlaybackStatus(status)) => status,
        Ok(Contents::Metadata { .. }) => {
            let playbackstatus = get_property(conn, msg.sender(), "PlaybackStatus");
            if let Ok(Contents::PlaybackStatus(status)) = playbackstatus {
                status
            } else {
                return;
            }
        }
        _ => return, // Ignore messages with no valid content
    };

    let mediaplayer = match query_id(conn, &properties_opts.mediaplayer) {
        Ok(mediaplayer) => mediaplayer,
        Err(_) => return, // No valid mediaplayer found
    };

    match status.as_str() {
        "Playing" => toggle_playback(conn, &mediaplayer, "Pause"),
        _ => toggle_playback(conn, &mediaplayer, "Play"),
    }
}
