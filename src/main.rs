use std::error::Error;
use std::time::Duration;

use dbus::blocking::Connection;
use dbus::message::MatchRule;
use dbus::Message;
use dbus::MessageType::Signal;

fn main() -> Result<(), Box<dyn Error>> {
    let conn = Connection::new_session().expect("D-Bus connection failed!");

    let rule = MatchRule::new()
        .with_path("/org/mpris/MediaPlayer2")
        .with_member("PropertiesChanged")
        .with_interface("org.freedesktop.DBus.Properties")
        .with_type(Signal);

    conn.add_match(rule, |_: (), _, msg| {
        handle_message(&msg);
        true
    })
    .expect("add_match failed!");

    loop {
        conn.process(Duration::from_millis(1000)).unwrap();
    }
}

fn handle_message(msg: &Message) {
    println!("Got message: {:?}", msg);
}
