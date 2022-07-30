use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::time::Duration;

use dbus::arg::{Iter, PropMap, RefArg, TypeMismatchError, Variant};
use dbus::blocking::BlockingSender;
use dbus::message::MatchRule;
use dbus::Error as DBusError;
use dbus::Message;
use dbus::MessageType::Signal;
use dbus::{arg, blocking::LocalConnection};

use clap::{arg, Parser};

mod options;

struct NameOwnerChanged {
    name: String,
    _old_name: String,
    new_name: String,
}

struct Song {
    artist: String,
    title: String,
    playbackstatus: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    clap::Command::new("Lystra")
        .about("A simple and small app to let Waybar display what is playing on Spotify.")
        .arg(arg!(--length <NUMBER> "Set max length of output. Default: <40>").required(false))
        .arg(
            arg!(--signal <NUMBER> "Set signal number used to update Waybar. Default: <8>")
                .required(false),
        )
        .arg(
            arg!(--playing <STRING> "Set max length of output. Default: <Playing:>")
                .required(false),
        )
        .arg(arg!(--paused <STRING> "Set max length of output. Default: <Paused:>").required(false))
        .get_matches();

    let conn = LocalConnection::new_session().expect("D-Bus connection failed!");

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

    conn.add_match(properties_rule, |_: (), conn, msg| {
        handle_properties(&conn, &msg).expect("Failed to handle properties.");
        true
    })
    .expect("Adding properties_rule failed!");

    conn.add_match(nameowner_rule, |_: (), _, msg| {
        handle_nameowner(&msg).expect("Failed to handle nameowner.");
        true
    })
    .expect("Adding nameowner_rule failed!");

    loop {
        conn.process(Duration::from_millis(1000))
            .expect("Failed to set up loop to handle messages.");
    }
}

/// Handles any incoming messages when a nameowner has changed.
fn handle_nameowner(msg: &Message) -> Result<(), Box<dyn Error>> {
    let nameowner: NameOwnerChanged =
        read_nameowner(msg).expect("Could not read the nameowner from incoming message.");

    if nameowner.name == "org.mpris.MediaPlayer2.spotify" && nameowner.new_name == "" {
        write_to_file("".to_string()).expect("Failed to send a blank string to Waybar.");
        send_update_signal().expect("Failed to send update signal to Waybar.");
    }
    Ok(())
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

/// Truncate the text accordingly before output
fn truncate_output(text: String) -> String {
    let mut text: String = text;
    let max_length: usize = options::Args::parse().length;

    if text.chars().count() > max_length {
        let upto = text
            .char_indices()
            .map(|(i, _)| i)
            .nth(max_length)
            .unwrap_or(text.chars().count());
        text.truncate(upto);

        text = text.trim_end().to_string();

        text = format!("{}{}", text, "…");

        if text.contains("(") && !text.contains(")") {
            text = format!("{}{}", text, ")")
        }

        return text;
    } else {
        return text.to_string();
    }
}

/// Waybar doesn't like ampersand. So we replace them in the output string.
fn escape_ampersand(text: String) -> String {
    let result = str::replace(&text, "&", "&amp;");
    result
}

/// Function to handle the incoming signals from Spotify when properties change
fn handle_properties(conn: &LocalConnection, msg: &Message) -> Result<(), Box<dyn Error>> {
    // First we try to get the ID of Spotify, as well as the ID of the signal sender
    let id = get_spotify_id(&conn);
    let sender_id = msg.sender().unwrap().to_string();

    // Check if it was indeed Spotify that sent the signal, otherwise we just return an Ok and do nothing else.
    if let Ok(spotify_id) = id {
        if spotify_id != Some(sender_id) {
            return Ok(());
        }

        // Unpack the message received from the signal
        let now_playing = unpack_signal(&conn, &msg);

        match now_playing {
            // Assuming we got something useful back we proceed to prepare the output
            Ok(now_playing) => {
                if let Some(mut song) = now_playing {
                    // Swap out the default status message
                    if song.playbackstatus == "Playing" {
                        song.playbackstatus = options::Args::parse().playing;
                    } else if song.playbackstatus == "Paused" {
                        song.playbackstatus = options::Args::parse().paused;
                    }

                    // The default, for now.
                    let separator = String::from(" - ");

                    // TODO
                    // This is not pretty.
                    let mut text = song.artist + &separator + &song.title;
                    text = truncate_output(text);
                    text = escape_ampersand(text);
                    let output = format!("{} {}", song.playbackstatus, text);

                    write_to_file(output).expect("Failed to write to file.");
                    send_update_signal().expect("Failed to send update signal to Waybar.");
                }
            }
            // Bail on error.
            Err(_) => (),
        }
    }
    Ok(())
}

/// Writes out the finished output to a file that is then parsed by Waybar
fn write_to_file(text: String) -> Result<(), Box<dyn Error>> {
    let text: &[u8] = text.as_bytes();
    let mut file: File = File::create("/tmp/lystra_output.txt")?;
    file.write_all(text)?;
    Ok(())
}

/// Unpacks an incoming message when receiving a signal of PropertiesChanged
fn unpack_signal(conn: &LocalConnection, msg: &Message) -> Result<Option<Song>, Box<dyn Error>> {
    // Read the two first arguments of the received message
    let read_msg: Result<(String, PropMap), TypeMismatchError> = msg.read2();

    // Get the HashMap from the second argument, which contains the relevant info
    let map = &read_msg.unwrap().1;

    // Unwrap the string that tells us what kind of contents is in the message
    let contents = map.keys().nth(0).unwrap().as_str();

    // Match the contents to perform the correct unpacking
    match contents {
        // Unpack the metadata to get artist and title of the song.
        // Since  the metadata never contains any information about playbackstatus, we explicitly ask for it
        "Metadata" => {
            let metadata: &dyn RefArg = &map["Metadata"].0;
            let map: &arg::PropMap = arg::cast(metadata).unwrap();
            let song_title: Option<&String> = arg::prop_cast(&map, "xesam:title");
            let song_artist: Option<&Vec<String>> = arg::prop_cast(&map, "xesam:artist");

            Ok(Some(Song {
                artist: song_artist.unwrap()[0].to_string(),
                title: song_title.unwrap().to_string(),
                playbackstatus: get_playbackstatus(&conn).unwrap(),
            }))
        }
        // If we receive an update on PlaybackStatus we receive no infromation about artist or title
        // As above, no metadata is provided with the playbackstatus, so we have to get it ourselves
        "PlaybackStatus" => {
            let artist_title = get_metadata(&conn).unwrap();
            Ok(Some(Song {
                artist: artist_title.0,
                title: artist_title.1,
                playbackstatus: map["PlaybackStatus"].0.as_str().unwrap().to_string(),
            }))
        }
        _ => Ok(None),
    }
}

/// Gets the playbackstatus from Spotify
fn get_playbackstatus(conn: &LocalConnection) -> Result<String, Box<dyn Error>> {
    let message = dbus::Message::call_with_args(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.mpris.MediaPlayer2.Player", "PlaybackStatus"),
    );

    let reply = conn
        .send_with_reply_and_block(message, Duration::from_millis(5000))
        .unwrap();

    let playbackstatus: Result<Variant<String>, TypeMismatchError> = reply.read1();

    let result = playbackstatus.unwrap().0;

    Ok(result)
}

/// Gets the currently playing artist and title from Spotify in a tuple: (artist, title)
fn get_metadata(conn: &LocalConnection) -> Result<(String, String), Box<dyn Error>> {
    let message = dbus::Message::call_with_args(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.mpris.MediaPlayer2.Player", "Metadata"),
    );

    let reply = conn
        .send_with_reply_and_block(message, Duration::from_millis(5000))
        .unwrap();

    let metadata: Result<Variant<PropMap>, TypeMismatchError> = reply.read1();

    let properties: PropMap = metadata.unwrap().0;

    let title: Option<&String> = arg::prop_cast(&properties, "xesam:title");
    let artist: Option<&Vec<String>> = arg::prop_cast(&properties, "xesam:artist");

    let result: (String, String) = (artist.unwrap()[0].to_string(), title.unwrap().to_string());

    Ok(result)
}

/// Sends a signal to Waybar so that the output is updated
fn send_update_signal() -> Result<(), Box<dyn Error>> {
    let signal = format!("-RTMIN+{}", options::Args::parse().signal);

    Command::new("pkill")
        .arg(signal)
        .arg("waybar")
        .output()
        .expect("Failed to send update signal to Waybar.");
    Ok(())
}

/// Gets the Spotify ID over DBus
fn get_spotify_id(
    conn: &LocalConnection,
) -> Result<Option<String>, (DBusError, TypeMismatchError)> {
    // Create a message with a method call to ask for the ID of Spotify
    let message = dbus::Message::call_with_args(
        "org.freedesktop.DBus",
        "/",
        "org.freedesktop.DBus",
        "GetNameOwner",
        ("org.mpris.MediaPlayer2.spotify",),
    );

    // Send the message and await the reply
    let reply: Result<Message, DBusError> =
        conn.send_with_reply_and_block(message, Duration::from_millis(5000));

    match reply {
        // If we get a reply, we unpack ID from the message and return it
        Ok(reply) => {
            let read_msg: Result<String, TypeMismatchError> = reply.read1();
            let spotify_id = read_msg.unwrap();
            Ok(Some(spotify_id))
        }
        // If Spotify is not running we'll receive an error in return, which is fine, so return None instead
        Err(_) => Ok(None),
    }
}
