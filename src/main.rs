use std::fs::File;
use std::io::Write;
use std::process::Command;
use std::time::Duration;

use anyhow::{Context, Result};

use dbus::arg::{Iter, PropMap, RefArg, Variant};
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

struct Output {
    now_playing: String,
    playbackstatus: String,
}

impl Output {
    /// Construct the output according to defined order of artist and title.
    fn construct(song: Song) -> Output {
        let order = options::Args::parse().order; // Get the order of output
        let separator = options::Args::parse().separator; // Get the separator

        let order = order.split(","); // Separate the keywords for "artist" and "title"
        let mut order_set: [Option<&str>; 2] = [None, None]; // Set up an array to store the desired order

        for (i, s) in order.enumerate() {
            match s {
                "artist" => {
                    order_set[i] = Some(song.artist.as_str());
                }
                "title" => {
                    order_set[i] = Some(song.title.as_str());
                }
                _ => (),
            }
        }
        return Output {
            playbackstatus: song.playbackstatus,
            now_playing: format!(
                "{}{}{}",
                order_set[0].unwrap_or("Make sure you entered the output order correctly."),
                separator,
                order_set[1].unwrap_or("Make sure you entered the output order correctly.")
            ), // The complete text
        };
    }

    /// Shorten the output according to the determined max length
    fn shorten(&mut self) -> &mut Self {
        let max_length: usize = options::Args::parse().length;

        if self.now_playing.chars().count() > max_length {
            let upto = self
                .now_playing
                .char_indices()
                .map(|(i, _)| i)
                .nth(max_length)
                .unwrap_or(self.now_playing.chars().count());
            self.now_playing.truncate(upto);

            self.now_playing = self.now_playing.trim_end().to_string();

            self.now_playing = format!("{}{}", self.now_playing, "…");

            if self.now_playing.contains("(") && !self.now_playing.contains(")") {
                self.now_playing = format!("{}{}", self.now_playing, ")")
            }
        }
        return self;
    }

    /// Waybar doesn't like ampersand. So we replace them in the output string.
    fn escape_ampersand(&mut self) -> &mut Self {
        self.now_playing = str::replace(&self.now_playing, "&", "&amp;");
        return self;
    }
}

fn main() -> Result<()> {
    clap::Command::new("Lystra")
        .about("A simple and small app to let Waybar display what is playing on Spotify.")
        .arg(arg!(--length <NUMBER> "Set max length of output. Default: <40>").required(false))
        .arg(
            arg!(--signal <NUMBER> "Set signal number used to update Waybar. Default: <8>")
                .required(false),
        )
        .arg(
            arg!(--playing <STRING> "Set value for playbackstatus = Playing. Default: <Playing:>")
                .required(false),
        )
        .arg(
            arg!(--paused <STRING> "Set value for playbackstatus = Paused. Default: <Paused:>")
                .required(false),
        )
        .arg(
            arg!(--separator <STRING> "Set value for the separator. Default: <->")
                .required(false),
        )
        .arg(
            arg!(--order <STRING> "Set order of artist and song, comma-separated. Default: <artist,song>")
                .required(false),
        )
        .get_matches();

    let conn = LocalConnection::new_session().context("D-Bus connection failed!")?;

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
    })?;

    conn.add_match(nameowner_rule, |_: (), _, msg| {
        handle_nameowner(&msg).expect("Failed to handle nameowner.");
        true
    })?;

    loop {
        conn.process(Duration::from_millis(1000))
            .expect("Failed to set up loop to handle messages.");
    }
}

/// Handles any incoming messages when a nameowner has changed.
fn handle_nameowner(msg: &Message) -> Result<()> {
    let nameowner: NameOwnerChanged =
        read_nameowner(msg).expect("Could not read the nameowner from incoming message.");

    if nameowner.name == "org.mpris.MediaPlayer2.spotify" && nameowner.new_name == "" {
        write_to_file("".to_string())?;
        send_update_signal()?;
    }
    Ok(())
}

/// This unpacks a message containing NameOwnerChanged. The field old_name is in fact never used.
fn read_nameowner(msg: &Message) -> Result<NameOwnerChanged> {
    let mut iter: Iter = msg.iter_init();
    Ok(NameOwnerChanged {
        name: iter.read()?,
        _old_name: iter.read()?,
        new_name: iter.read()?,
    })
}

/// Function to handle the incoming signals from Spotify when properties change
fn handle_properties(conn: &LocalConnection, msg: &Message) -> Result<()> {
    // First we try to get the ID of Spotify, as well as the ID of the signal sender
    let spotify_id = get_spotify_id(&conn);
    let sender_id = msg.sender().unwrap().to_string();

    // Check if it was indeed Spotify that sent the signal, otherwise we just return an Ok and do nothing else.
    if let Ok(spotify_id) = spotify_id {
        if spotify_id != Some(sender_id) {
            return Ok(());
        }

        // Unpack the message received from the signal
        let song = unpack_signal(&conn, &msg)?;

        if let Some(mut song) = song {
            // Swap out the default status message
            if song.playbackstatus == "Playing" {
                song.playbackstatus = options::Args::parse().playing;
            } else if song.playbackstatus == "Paused" {
                song.playbackstatus = options::Args::parse().paused;
            }

            let mut output = Output::construct(song);

            output.shorten().escape_ampersand();

            write_to_file(format!("{}{}", output.playbackstatus, output.now_playing))
                .expect("Failed to write to file.");
            send_update_signal().expect("Failed to send update signal to Waybar.");
        }
    }

    Ok(())
}

/// Writes out the finished output to a file that is then parsed by Waybar
fn write_to_file(text: String) -> Result<()> {
    let text: &[u8] = text.as_bytes();
    let mut file: File = File::create("/tmp/lystra_output.txt")?;
    file.write_all(text)?;
    Ok(())
}

/// Unpacks an incoming message when receiving a signal of PropertiesChanged
fn unpack_signal(conn: &LocalConnection, msg: &Message) -> Result<Option<Song>> {
    // Read the two first arguments of the received message
    let read_msg: (String, PropMap) = msg.read2()?;

    // Get the HashMap from the second argument, which contains the relevant info
    let map = read_msg.1;

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
fn get_playbackstatus(conn: &LocalConnection) -> Result<String> {
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

    let playbackstatus: Variant<String> = reply.read1()?;

    let result = playbackstatus.0;

    Ok(result)
}

/// Gets the currently playing artist and title from Spotify in a tuple: (artist, title)
fn get_metadata(conn: &LocalConnection) -> Result<(String, String)> {
    let message = dbus::Message::call_with_args(
        "org.mpris.MediaPlayer2.spotify",
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.mpris.MediaPlayer2.Player", "Metadata"),
    );

    let reply = conn.send_with_reply_and_block(message, Duration::from_millis(5000))?;

    let metadata: Variant<PropMap> = reply.read1()?;

    let properties: PropMap = metadata.0;

    let title: &String =
        arg::prop_cast(&properties, "xesam:title").expect("Failed to get the song title.");
    let artist: &Vec<String> =
        arg::prop_cast(&properties, "xesam:artist").expect("Failed to get the song artist.");

    let result: (String, String) = (artist[0].to_owned(), title.to_owned());

    Ok(result)
}

/// Sends a signal to Waybar so that the output is updated
fn send_update_signal() -> Result<()> {
    let signal = format!("-RTMIN+{}", options::Args::parse().signal);

    Command::new("pkill")
        .arg(signal)
        .arg("waybar")
        .output()
        .context("Failed to send a signal command to Waybar")?;
    Ok(())
}

/// Gets the Spotify ID over DBus
fn get_spotify_id(conn: &LocalConnection) -> Result<Option<String>> {
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
        // If we get a reply, we unpack the ID from the message and return it
        Ok(reply) => {
            let spotify_id: String = reply.read1()?;
            Ok(Some(spotify_id))
        }
        // If Spotify is not running we'll receive an error in return, which is fine, so return None instead
        Err(_reply) => Ok(None),
    }
}
