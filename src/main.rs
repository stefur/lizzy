use std::error::Error;
use std::process::Command;
use std::time::Duration;

use dbus::arg::TypeMismatchError;
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

    if nameowner.name == "org.mpris.MediaPlayer2.spotify" {
        if nameowner.old_name == "" {
            println!("Spotify opened. Probably not need to do anything here.")
        }

        if nameowner.new_name == "" {
            println!("Spotify closed. Now we should clear the output.")
        }
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
        get_playbackstatus(conn).expect("Failed to poll the playback status.");
        unpack_message(msg);
        execute_update("This is a test!".to_string()).expect("Execution of IPC command failed.");
        //println!("Got message: {:?}", msg);
        println!("From here we should update the string output");
    }
}

fn unpack_message(msg: &Message) {
    let iter = msg.iter_init();
    for i in iter {
        println!("{:?}", i);
    }
}

fn get_metadata(conn: &LocalConnection) -> Result<(), Box<dyn std::error::Error>> {
    let proxy = conn.with_proxy(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        Duration::from_millis(5000),
    );

    let metadata: arg::PropMap = proxy.get("org.mpris.MediaPlayer2.Player", "Metadata")?;
    let title: Option<&String> = arg::prop_cast(&metadata, "xesam:title");
    let artist: Option<&Vec<String>> = arg::prop_cast(&metadata, "xesam:artist");

    println!("The artist is: {:?}", artist);
    println!("The title is: {:?}", title);

    Ok(())
}

fn get_playbackstatus(
    conn: &LocalConnection,
) -> Result<Box<dyn arg::RefArg>, Box<dyn std::error::Error>> {
    let proxy = conn.with_proxy(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        Duration::from_millis(5000),
    );

    let playbackstatus: Box<dyn arg::RefArg> =
        proxy.get("org.mpris.MediaPlayer2.Player", "PlaybackStatus")?;

    Ok(playbackstatus)
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
