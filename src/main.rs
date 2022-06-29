use std::error::Error;
use std::time::Duration;

use dbus::blocking::LocalConnection;
use dbus::message::MatchRule;
use dbus::Message;
use dbus::MessageType::Signal;

fn main() -> Result<(), Box<dyn Error>> {
    let conn = LocalConnection::new_session().expect("D-Bus connection failed!");

    let rule = MatchRule::new()
        .with_path("/org/mpris/MediaPlayer2")
        .with_member("PropertiesChanged")
        .with_interface("org.freedesktop.DBus.Properties")
        .with_type(Signal);

    conn.add_match(rule, |_: (), conn, msg| {
        handle_message(&conn, &msg);
        true
    })
    .expect("add_match failed!");

    loop {
        conn.process(Duration::from_millis(1000)).unwrap();
    }
}

fn handle_message(conn: &LocalConnection, msg: &Message) {
    if get_spotify_id(&conn).is_err() {
        return;
    }

    let spotify_id = get_spotify_id(&conn).unwrap().to_string();

    let sender = msg.sender().unwrap().to_string();

    if spotify_id == sender {
        println!("Got message: {:?}", msg);
        println!("From here we should update the string output");
    }
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
