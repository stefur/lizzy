use std::error::Error;
use std::process::Command;
use std::time::Duration;

use dbus::arg::{TypeMismatchError, RefArg};
use dbus::blocking::stdintf::org_freedesktop_dbus::Properties;
use dbus::message::MatchRule;
use dbus::Message;
use dbus::MessageType::Signal;
use dbus::{arg, blocking::LocalConnection};

struct NameOwnerChanged {
    name: String,
    old_name: String,
    new_name: String,
}

struct Song {
    artist: String,
    title: String,
    playbackstatus: String,
}

fn main() -> Result<(), Box<dyn Error>> {
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
        handle_properties(&conn, &msg);
        true
    })
    .expect("Adding properties_rule failed!");

    conn.add_match(nameowner_rule, |_: (), _, msg| {
        handle_nameowner(&msg);
        true
    })
    .expect("Adding properties_rule failed!");

    loop {
        conn.process(Duration::from_millis(1000)).unwrap();
    }
}

fn handle_nameowner(msg: &Message) {
    let nameowner = read_nameowner(msg).unwrap();

    if nameowner.name == "org.mpris.MediaPlayer2.spotify" && nameowner.new_name == "" {
        execute_update("".to_string()).expect("Execution of IPC command failed.");
    }
}

fn read_nameowner(msg: &Message) -> Result<NameOwnerChanged, TypeMismatchError> {
    let mut iter = msg.iter_init();
    Ok(NameOwnerChanged {
        name: iter.read()?,
        old_name: iter.read()?,
        new_name: iter.read()?,
    })
}

fn handle_properties(conn: &LocalConnection, msg: &Message) {
    if get_spotify_id(&conn).is_err() {
        return;
    }

    let spotify_id = get_spotify_id(&conn).unwrap().to_string();

    let sender = msg.sender().unwrap().to_string();

    if spotify_id == sender {
        let now_playing: Song = unpack_message(&conn, &msg).expect("Failed to unpack the message.");
        execute_update(now_playing.playbackstatus + ": " + &now_playing.artist + " - " + &now_playing.title).expect("Execution of IPC command failed.");
    }
}

fn unpack_message(conn: &LocalConnection, msg: &Message) -> Result<Song, Box<dyn std::error::Error>> {
    let read_msg: Result<(String, dbus::arg::PropMap), TypeMismatchError> = msg.read2();

    let map = &read_msg.unwrap().1;

    let contents = map.keys().nth(0).unwrap().as_str();

    match contents {
        "Metadata" => {
            let metadata: &dyn RefArg = &map["Metadata"].0;
            let map: &arg::PropMap = arg::cast(metadata).unwrap();
            let song_title: Option<&String> = arg::prop_cast(&map, "xesam:title");
            let song_artist: Option<&Vec<String>> = arg::prop_cast(&map, "xesam:artist");
            let song_playbackstatus: String = get_playbackstatus(&conn).unwrap().to_string();

            Ok(Song {
                artist: song_artist.unwrap()[0].to_string(),
                title: song_title.unwrap().to_string(),
                playbackstatus: song_playbackstatus,
            })
        }
        "PlaybackStatus" => {
            let song_playbackstatus: String = map["PlaybackStatus"].0.as_str().unwrap().to_string();
            let song_metadata: (String, String) = get_metadata(&conn).unwrap();
            let song_artist: String = song_metadata.0;
            let song_title: String = song_metadata.1;

            Ok(Song {
                artist: song_artist,
                title: song_title,
                playbackstatus: song_playbackstatus,
            })
        }
        _ => {
            panic!("Unable to unpack: {:?}", contents);
        }
    }

}

fn get_metadata(conn: &LocalConnection) -> Result<(String, String), Box<dyn std::error::Error>> {
    let proxy = conn.with_proxy(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        Duration::from_millis(5000),
    );

    let metadata: arg::PropMap = proxy.get("org.mpris.MediaPlayer2.Player", "Metadata")?;
    let title: Option<&String> = arg::prop_cast(&metadata, "xesam:title");
    let artist: Option<&Vec<String>> = arg::prop_cast(&metadata, "xesam:artist");

    let result = (artist.unwrap()[0].to_string(), title.unwrap().to_string());

    Ok(result)
}

fn get_playbackstatus(
    conn: &LocalConnection,
) -> Result<String, Box<dyn std::error::Error>> {
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

fn execute_update(text: String) -> Result<(), Box<dyn std::error::Error>> {
    let _cmd = Command::new("polybar-msg")
        .arg("action")
        .arg("spotify.send")
        .arg(text)
        .output()
        .expect("Failed to execute update");
    Ok(())
}

fn get_spotify_id(conn: &LocalConnection) -> Result<String, Box<dyn std::error::Error>> {
    let proxy = conn.with_proxy("org.freedesktop.DBus", "/", Duration::from_millis(5000));
    let (spotify_id,): (String,) = proxy.method_call(
        "org.freedesktop.DBus",
        "GetNameOwner",
        ("org.mpris.MediaPlayer2.spotify",),
    )?;
    Ok(spotify_id)
}
