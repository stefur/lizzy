use core::time::Duration;
use dbus::{blocking::LocalConnection, message::MatchRule, MessageType::Signal};
use std::error::Error;
use std::rc::Rc;

mod message;
mod options;
mod output;

fn main() -> Result<(), Box<dyn Error>> {
    // Parse the options for use within the match rules
    let options: options::Arguments = match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };

    let properties_options = Rc::new(options);
    let nameowner_options = Rc::clone(&properties_options);

    let conn = LocalConnection::new_session().expect("Failed to connect to the session bus.");

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
        if message::is_mediaplayer(conn, msg, &properties_options.mediaplayer) {
            message::handle_valid_mediaplayer_signal(conn, msg, &properties_options);
        } else if message::should_toggle_playback(conn, &properties_options) {
            message::toggle_playback_if_needed(conn, msg, &properties_options);
        }
        true
    })?;

    // Handles any incoming messages when a nameowner has changed.
    conn.add_match(nameowner_rule, move |_: (), _, msg| {
        // Check if we should listen to all mediaplayers
        if nameowner_options.mediaplayer.is_empty() {
            return true;
        }

        if let Ok(nameowner) = message::read_nameowner(msg) {
            // If the mediaplayer has been closed, clear the output
            if nameowner
                .name
                .to_lowercase()
                .contains(&nameowner_options.mediaplayer)
                && nameowner.new_name.is_empty()
            {
                println!();
            }
        }

        true
    })?;

    loop {
        conn.process(Duration::from_millis(1000))
            .expect("lizzy should be able to set up a loop to listen for messages.");
    }
}
