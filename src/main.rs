use std::error::Error;
use std::time::Duration;

use dbus::arg::TypeMismatchError;
use dbus::blocking::LocalConnection;
use dbus::message::MatchRule;
use dbus::Message;
use dbus::MessageType::Signal;

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

struct NameOwnerChanged {
    name: String,
    old_name: String,
    new_name: String,
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
