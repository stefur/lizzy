use std::error::Error;
use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::time::Duration;

use dbus::arg::{Iter, PropMap, RefArg, TypeMismatchError};
use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
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
    artist: String,
    title: String,
    playbackstatus: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    clap::Command::new("Lystra")
        .about("A simple and small app to let Waybar display what is playing on Spotify.")
        .arg(arg!(-l --length <NUMBER> "Set max length of output. Default: 40").required(false))
        .arg(
            arg!(-s --signal <NUMBER> "Set signal number used to update Waybar. Default: 8")
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
        send_signal().expect("Failed to send update signal to Waybar.");
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
    let result = str::replace(&text, "&", "&amp;");
    result
}

fn handle_properties(conn: &LocalConnection, msg: &Message) -> Result<(), Box<dyn Error>> {
    let id = get_spotify_id(&conn);
    let sender_id = msg.sender().unwrap().to_string();

    if let Ok(spotify_id) = id {
        if spotify_id != sender_id {
            return Ok(());
        }

        let now_playing: Option<Song> =
            unpack_message(&conn, &msg).expect("Failed to unpack the message.");
        if let Some(mut song) = now_playing {
            match song.playbackstatus.as_str() {
                "Playing" => song.playbackstatus = "".to_string(),
                "Paused" => song.playbackstatus = "Paused: ".to_string(),
                &_ => (),
            }

            let mut artist_song = song.artist + " - " + &song.title;
            artist_song = truncate_output(artist_song);
            artist_song = escape_ampersand(artist_song);

            write_to_file(format!("{}{}", song.playbackstatus, artist_song))
                .expect("Failed to write to file.");
            send_signal().expect("Failed to send update signal to Waybar.");
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

fn unpack_message(conn: &LocalConnection, msg: &Message) -> Result<Option<Song>, Box<dyn Error>> {
    let read_msg: Result<(String, PropMap), TypeMismatchError> = msg.read2();

    let map = &read_msg.unwrap().1;

    let contents = map.keys().nth(0).unwrap().as_str();

    match contents {
        "Metadata" => {
            let metadata: &dyn RefArg = &map["Metadata"].0;
            let map: &arg::PropMap = arg::cast(metadata).unwrap();
            let song_title: Option<&String> = arg::prop_cast(&map, "xesam:title");
            let song_artist: Option<&Vec<String>> = arg::prop_cast(&map, "xesam:artist");
            let song_playbackstatus: String = get_playbackstatus(&conn).unwrap().to_string();

            Ok(Some(Song {
                artist: song_artist.unwrap()[0].to_string(),
                title: song_title.unwrap().to_string(),
                playbackstatus: song_playbackstatus,
            }))
        }
        "PlaybackStatus" => {
            let song_playbackstatus: String = map["PlaybackStatus"].0.as_str().unwrap().to_string();
            let song_metadata: (String, String) = get_metadata(&conn).unwrap();
            let song_artist: String = song_metadata.0;
            let song_title: String = song_metadata.1;

            Ok(Some(Song {
                artist: song_artist,
                title: song_title,
                playbackstatus: song_playbackstatus,
            }))
        }
        _ => Ok(None),
    }
}

fn get_metadata(conn: &LocalConnection) -> Result<(String, String), Box<dyn Error>> {
    let proxy = conn.with_proxy(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        Duration::from_millis(5000),
    );

    let metadata: PropMap = proxy.get("org.mpris.MediaPlayer2.Player", "Metadata")?;
    let title: Option<&String> = arg::prop_cast(&metadata, "xesam:title");
    let artist: Option<&Vec<String>> = arg::prop_cast(&metadata, "xesam:artist");

    let result = (artist.unwrap()[0].to_string(), title.unwrap().to_string());

    Ok(result)
}

fn get_playbackstatus(conn: &LocalConnection) -> Result<String, Box<dyn Error>> {
    let proxy = conn.with_proxy(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        Duration::from_millis(5000),
    );

    let playbackstatus: Box<dyn arg::RefArg> =
        proxy.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus")?;

    let result = playbackstatus.as_str().unwrap().to_string();

    Ok(result)
}

fn send_signal() -> Result<(), Box<dyn Error>> {
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
