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
    /// Create the output according to defined order of artist and title.
    fn new(song: Song, order: &str, separator: &str) -> Output {
        let order = order.split(','); // Separate the keywords for "artist" and "title"
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
        Output {
            playbackstatus: song.playbackstatus,
            now_playing: format!(
                "{}{}{}",
                order_set[0].expect("Make sure you entered the output order correctly. Should be 'artist,title' or 'title,artist'."),
                separator,
                order_set[1].expect("Make sure you entered the output order correctly. Should be 'artist,title' or 'title,artist'.")
            ), // The complete text
        }
    }

    /// Shorten the output according to the determined max length
    fn shorten(&mut self, length: usize) -> &mut Self {
        if self.now_playing.chars().count() > length {
            let upto = self
                .now_playing
                .char_indices()
                .map(|(i, _)| i)
                .nth(length)
                .unwrap_or(self.now_playing.chars().count());
            self.now_playing.truncate(upto);

            self.now_playing = self.now_playing.trim_end().to_owned();

            self.now_playing = format!("{}{}", self.now_playing, "…");

            if self.now_playing.contains('(') && !self.now_playing.contains(')') {
                self.now_playing = format!("{}{}", self.now_playing, ")")
            }
        }
        self
    }

    /// Apply playback status arguments.
    fn set_status(&mut self, playing: &str, paused: &str) -> &mut Self {
        if self.playbackstatus == "Playing" {
            self.playbackstatus = playing.to_owned();
        } else if self.playbackstatus == "Paused" {
            self.playbackstatus = paused.to_owned();
        }

        self
    }

    /// Waybar doesn't like ampersand. So we replace them in the output string.
    fn escape_ampersand(&mut self) -> &mut Self {
        self.now_playing = str::replace(&self.now_playing, "&", "&amp;");
        self
    }

    /// Apply color to artist/title as well as playback status.
    fn colorize(&mut self, textcolor: &str, playbackcolor: &str) -> &mut Self {
        if !textcolor.is_empty() {
            self.now_playing = format!(
                "<span foreground='{}'>{}</span>",
                textcolor, self.now_playing
            );
        }

        if !playbackcolor.is_empty() {
            self.playbackstatus = format!(
                "<span foreground='{}'>{}</span>",
                playbackcolor, self.playbackstatus
            );
        }

        self
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // Parse the options for use within the match rule for property changes
    let properties_opts: options::Arguments = match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };
    // And a clone used for nameowner changes
    let nameowner_opts = properties_opts.clone();

    let conn =
        LocalConnection::new_session().expect("Lystra should be able to connect to session bus.");

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

        if is_mediaplayer(conn, msg, &properties_opts.mediaplayer) {
            // Unpack the song received from the signal to create an output
            if let Ok(Some(song)) = unpack_song(conn, msg) {
                let mut output =
                    Output::new(song, &properties_opts.order, &properties_opts.separator);

                // Customize the output
                output
                    .shorten(properties_opts.length)
                    .escape_ampersand()
                    .set_status(&properties_opts.playing, &properties_opts.paused)
                    .colorize(&properties_opts.textcolor, &properties_opts.playbackcolor);

                // Write out the output to file and update Waybar
                write_output(format!("{}{}", output.playbackstatus, output.now_playing))
                    .expect("Lystra failed to write output to file.");
                send_update_signal(properties_opts.signal)
                    .expect("Failed to send update signal to Waybar.");
            }
        }
        true
    })?;

    // Handles any incoming messages when a nameowner has changed.
    conn.add_match(nameowner_rule, move |_: (), _, msg| {
        // Not a very pretty solution, but if we listen to signals coming from all mediaplayers,
        // we never clear the output to avoid clearing the output in error due to mediaplayers closing
        if nameowner_opts.mediaplayer.is_empty() {
            return true;
        }

        let nameowner: NameOwnerChanged = read_nameowner(msg).expect(
            "Read the nameowner from incoming message needs to be done to determine the change.",
        );

        // If mediaplayer has been closed, clear the output by writing an empty string (for now)
        if nameowner
            .name
            .to_lowercase()
            .contains(nameowner_opts.mediaplayer.as_str())
            && nameowner.new_name.is_empty()
        {
            write_output("".to_string()).expect("Need to clear output by writing to file.");
            send_update_signal(nameowner_opts.signal)
                .expect("Clearing the output should also trigger an update to Waybar.");
        }

        true
    })?;

    loop {
        conn.process(Duration::from_millis(1000))
            .expect("Lystra should be able to set up a loop to listen for messages.");
    }
}

/// This unpacks a message containing NameOwnerChanged. The field old_name is in fact never used.
fn read_nameowner(msg: &Message) -> Result<NameOwnerChanged, TypeMismatchError> {
    let mut iter: Iter = msg.iter_init();
    Ok(NameOwnerChanged {
        name: iter.read()?,
        _old_name: iter.read()?,
        new_name: iter.read()?,
    })
}

/// Unpacks an incoming message when receiving a signal of PropertiesChanged from mediaplayer
fn unpack_song(conn: &LocalConnection, msg: &Message) -> Result<Option<Song>, TypeMismatchError> {
    // Read the two first arguments of the received message
    let read_msg: (String, PropMap) = msg.read2()?;

    let sender_id = msg
        .sender()
        .expect("A sender should have a valid id.")
        .to_string();

    // Get the HashMap from the second argument, which contains the relevant info
    let map = read_msg.1;

    // Unwrap the string that tells us what kind of contents is in the message
    let contents = map
        .keys()
        .next()
        .expect("Second key in contents should contain metadata or playbackstatus.")
        .as_str();

    // Match the contents to perform the correct unpacking
    match contents {
        // Unpack the metadata to get artist and title of the song.
        // Since  the metadata never contains any information about playbackstatus, we explicitly ask for it
        "Metadata" => {
            let metadata: &dyn RefArg = &map["Metadata"].0;
            let map: &arg::PropMap =
                arg::cast(metadata).expect("RefArg with metadata should be cast to PropMap");
            let song_title: Option<&String> = arg::prop_cast(map, "xesam:title");
            let song_artist: Option<&Vec<String>> = arg::prop_cast(map, "xesam:artist");

            Ok(Some(Song {
                artist: song_artist.expect("Correct metadata should contain an artist.")[0]
                    .to_owned(),
                title: song_title
                    .expect("Correct metadata should contain a song title.")
                    .to_owned(),
                playbackstatus: get_playbackstatus(conn, sender_id)
                    .expect("Correct metadata should contain playbackstatus."),
            }))
        }
        // If we receive an update on PlaybackStatus we receive no infromation about artist or title
        // As above, no metadata is provided with the playbackstatus, so we have to get it ourselves
        "PlaybackStatus" => {
            let artist_title = get_metadata(conn, sender_id)
                .expect("A currently playing song should have metadata.");
            Ok(Some(Song {
                artist: artist_title.0,
                title: artist_title.1,
                playbackstatus: map["PlaybackStatus"]
                    .0
                    .as_str()
                    .expect("Correct metadata should contain playbackstatus.")
                    .to_owned(),
            }))
        }
        _ => Ok(None),
    }
}

/// Gets the playbackstatus from the mediaplayer
fn get_playbackstatus(conn: &LocalConnection, mediaplayer: String) -> Result<String, DBusError> {
    let message = dbus::Message::call_with_args(
        mediaplayer,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.mpris.MediaPlayer2.Player", "PlaybackStatus"),
    );

    let reply = conn.send_with_reply_and_block(message, Duration::from_millis(5000))?;

    let playbackstatus: Variant<String> = reply.read1()?;

    let result = playbackstatus.0;

    Ok(result)
}

/// Gets the currently playing artist and title from the mediaplayer in a tuple: (artist, title)
fn get_metadata(
    conn: &LocalConnection,
    mediaplayer: String,
) -> Result<(String, String), DBusError> {
    let message = dbus::Message::call_with_args(
        mediaplayer,
        "/org/mpris/MediaPlayer2",
        "org.freedesktop.DBus.Properties",
        "Get",
        ("org.mpris.MediaPlayer2.Player", "Metadata"),
    );

    let reply = conn.send_with_reply_and_block(message, Duration::from_millis(5000))?;

    let metadata: Variant<PropMap> = reply.read1()?;

    let properties: PropMap = metadata.0;

    let title: &String = arg::prop_cast(&properties, "xesam:title")
        .expect("The song title should be present and extracted from the message.");
    let artist: &Vec<String> = arg::prop_cast(&properties, "xesam:artist")
        .expect("The song artist should be present and extracted from the message.");

    let result: (String, String) = (artist[0].to_owned(), title.to_owned());

    Ok(result)
}

/// Check if the sender of a message is the mediaplayer we're listening to
fn is_mediaplayer(conn: &LocalConnection, msg: &Message, mediaplayer: &String) -> bool {
    // If mediaplayer option is blank, we listen to all incoming signals and thus return true
    if mediaplayer.is_empty() {
        return true;
    }

    // Extract the sender of our incoming message
    let sender_id = msg
        .sender()
        .expect("A sender should have a valid id.")
        .to_string();

    // Create a query with a method call to ask for the ID of the mediaplayer
    let query = dbus::Message::call_with_args(
        "org.freedesktop.DBus",
        "/",
        "org.freedesktop.DBus",
        "GetNameOwner",
        (format!("org.mpris.MediaPlayer2.{}", mediaplayer),),
    );

    // Send the query and await the reply
    let reply: Result<Message, DBusError> =
        conn.send_with_reply_and_block(query, Duration::from_millis(5000));

    match reply {
        // If we get a reply, we unpack the ID from the message and return it
        Ok(reply) => {
            let mediaplayer_id: String = reply
                .read1()
                .expect("Mediaplayer ID should be a string in the first argument of the message");
            mediaplayer_id == sender_id
        }
        // If the message is not from the mediaplayer we're listening to we'll receive an error in return, which is fine, so return false
        Err(_reply) => false,
    }
}

/// Sends a signal to Waybar so that the output is updated
fn send_update_signal(signal: u8) -> Result<(), Box<dyn Error>> {
    let signal = format!("-RTMIN+{}", signal);

    Command::new("pkill")
        .arg(signal)
        .arg("waybar")
        .output()
        .expect("Failed to execute the command to update Waybar.");
    Ok(())
}

/// Writes out the finished output to a file that is then parsed by Waybar
fn write_output(text: String) -> Result<(), Box<dyn Error>> {
    let text: &[u8] = text.as_bytes();
    let mut file: File = File::create("/tmp/lystra-output")?;
    file.write_all(text)?;
    Ok(())
}
