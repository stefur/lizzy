use core::time::Duration;
use dbus::{blocking::LocalConnection, message::MatchRule, MessageType::Signal};
use once_cell::sync::Lazy;
use options::Arguments;
use std::error::Error;

mod message;
mod options;
mod output;

fn main() -> Result<(), Box<dyn Error>> {
    // Parse the options for use within the match rules
    static OPTIONS: Lazy<Arguments> = Lazy::new(|| match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    });

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
        if message::is_mediaplayer(conn, msg, &OPTIONS.mediaplayer) {
            message::handle_valid_mediaplayer_signal(conn, msg, &OPTIONS);
        } else if message::should_toggle_playback(conn, &OPTIONS) {
            message::toggle_playback_if_needed(conn, msg, &OPTIONS);
        }
        true
    })?;

    // Handles any incoming messages when a nameowner has changed.
    conn.add_match(nameowner_rule, move |_: (), _, msg| {
        // Check if we should listen to all mediaplayers
        if OPTIONS.mediaplayer.is_empty() {
            return true;
        }

        if let Ok(nameowner) = message::read_nameowner(msg) {
            if nameowner.new_name.is_empty()
                && nameowner.name.starts_with("org.mpris.MediaPlayer2.")
                && message::matches_pattern(
                    &OPTIONS.mediaplayer,
                    nameowner.name.trim_start_matches("org.mpris.MediaPlayer2."),
                )
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
