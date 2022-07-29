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

mod args;

struct NameOwnerChanged {
    name: String,
    _old_name: String,
    new_name: String,
}

struct Song {
    artist: Option<String>,
    title: Option<String>,
    playbackstatus: Option<String>,
}

fn main() -> Result<(), Box<dyn Error>> {
    clap::Command::new("Lystra")
        .about("A simple and small app to let Waybar display what is playing on Spotify.")
        .arg(arg!(-l --length <NUMBER> "Set max length of output. Default: <40>").required(false))
        .arg(
            arg!(-s --signal <NUMBER> "Set signal number used to update Waybar. Default: <8>")
                .required(false),
        )
        .arg(
            arg!(-p --playing <STRING> "Set max length of output. Default: <Playing:>")
                .required(false),
        )
        .arg(
            arg!(-n --notplaying <STRING> "Set max length of output. Default: <Paused:>")
                .required(false),
        )
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

fn handle_nameowner(msg: &Message) -> Result<(), Box<dyn Error>> {
    let nameowner: NameOwnerChanged =
        read_nameowner(msg).expect("Could not read the nameowner from incoming message.");

    if nameowner.name == "org.mpris.MediaPlayer2.spotify" && nameowner.new_name == "" {
        write_to_file("".to_string()).expect("Failed to send a blank string to Waybar.");
        send_update_signal().expect("Failed to send update signal to Waybar.");
    }
    Ok(())
}

fn read_nameowner(msg: &Message) -> Result<NameOwnerChanged, TypeMismatchError> {
    let mut iter: Iter = msg.iter_init();
    Ok(NameOwnerChanged {
        name: iter.read()?,
        _old_name: iter.read()?,
        new_name: iter.read()?,
    })
}

fn truncate_output(text: String) -> String {
    let mut text: String = text;
    let max_length: usize = args::Args::parse().length;

    if text.len() > max_length {
        let upto = text
            .char_indices()
            .map(|(i, _)| i)
            .nth(max_length)
            .unwrap_or(text.len());
        text.truncate(upto);

        let mut result: String = format!("{}{}", text, "…");

        if result.contains("(") && !result.contains(")") {
            result = format!("{}{}", result, ")")
        }

        return result;
    } else {
        return text.to_string();
    }
}

fn escape_ampersand(text: String) -> String {
    // Waybar doesn't like ampersand. So we replace them in the output string.
    let result = str::replace(&text, "&", "&amp;");
    result
}

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
        let now_playing = unpack_signal(&msg);

        match now_playing {
            // Assuming we got something useful back we proceed to prepare the output
            Ok(now_playing) => {
                if let Some(song) = now_playing {
                    // This will use the fields in the Song. If it it's None it will retrieve the missing information
                    // This is done to make sure that we have accurate information to pass on to output
                    let artist = song.artist.unwrap_or_else(|| get_metadata(conn).unwrap().0);
                    let title = song.title.unwrap_or_else(|| get_metadata(conn).unwrap().1);
                    let mut playbackstatus = song
                        .playbackstatus
                        .unwrap_or_else(|| get_playbackstatus(conn).unwrap());

                    // Swap out the default status message
                    if playbackstatus == "Playing" {
                        playbackstatus = args::Args::parse().playing;
                    } else if playbackstatus == "Paused" {
                        playbackstatus = args::Args::parse().notplaying;
                    }

                    // The default, for now.
                    let separator = String::from(" - ");

                    // TODO
                    // This is not pretty.
                    let mut artist_song = artist + &separator + &title;
                    artist_song = truncate_output(artist_song);
                    artist_song = escape_ampersand(artist_song);

                    write_to_file(format!("{} {}", playbackstatus, artist_song))
                        .expect("Failed to write to file.");
                    send_update_signal().expect("Failed to send update signal to Waybar.");
                }
            }
            // Bail on error.
            Err(_) => (),
        }
    }
    Ok(())
}

fn write_to_file(text: String) -> Result<(), Box<dyn Error>> {
    let text: &[u8] = text.as_bytes();
    let mut file: File = File::create("/tmp/lystra_output.txt")?;
    file.write_all(text)?;
    Ok(())
}

fn unpack_signal(msg: &Message) -> Result<Option<Song>, Box<dyn Error>> {
    // Read the two first arguments of the received message
    let read_msg: Result<(String, PropMap), TypeMismatchError> = msg.read2();

    // Get the HashMap from the second argument, which contains the relevant info
    let map = &read_msg.unwrap().1;

    // Unwrap the string that tells us what kind of contents is in the message
    let contents = map.keys().nth(0).unwrap().as_str();

    // Match the contents to perform the correct unpacking
    match contents {
        // Unpack the metadata to get artist and title of the song
        "Metadata" => {
            let metadata: &dyn RefArg = &map["Metadata"].0;
            let map: &arg::PropMap = arg::cast(metadata).unwrap();
            let song_title: Option<&String> = arg::prop_cast(&map, "xesam:title");
            let song_artist: Option<&Vec<String>> = arg::prop_cast(&map, "xesam:artist");

            Ok(Some(Song {
                artist: Some(song_artist.unwrap()[0].to_string()),
                title: Some(song_title.unwrap().to_string()),
                playbackstatus: None,
            }))
        }
        // If we receive an update on PlaybackStatus we receive no infromation about artist or title
        "PlaybackStatus" => Ok(Some(Song {
            artist: None,
            title: None,
            playbackstatus: Some(map["PlaybackStatus"].0.as_str().unwrap().to_string()),
        })),
        _ => Ok(None),
    }
}

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

    let properties = metadata.unwrap().0;

    let title: Option<&String> = arg::prop_cast(&properties, "xesam:title");
    let artist: Option<&Vec<String>> = arg::prop_cast(&properties, "xesam:artist");

    let result: (String, String) = (artist.unwrap()[0].to_string(), title.unwrap().to_string());

    Ok(result)
}

fn send_update_signal() -> Result<(), Box<dyn Error>> {
    let signal = format!("-RTMIN+{}", args::Args::parse().signal);

    Command::new("pkill")
        .arg(signal)
        .arg("waybar")
        .output()
        .expect("Failed to send update signal to Waybar.");
    Ok(())
}

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
