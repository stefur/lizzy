use anyhow::{Context, Result};
use media::Media;
use once_cell::sync::Lazy;
use options::Arguments;
use zbus::export::futures_util::stream::StreamExt;
use zbus::fdo::DBusProxy;
use zbus::fdo::PropertiesChanged;
use zbus::fdo::PropertiesChangedArgs;
use zbus::names::BusName;
use zbus::names::OwnedBusName;
use zbus::zvariant::Array;
use zbus::zvariant::Dict;
use zbus::zvariant::NoneValue;
use zbus::zvariant::Value;
use zbus::Connection;
use zbus::MatchRule;
use zbus::MessageStream;
use zbus::Proxy;
mod media;
mod options;
type BoxedError = Box<dyn std::error::Error + Send + Sync>;

/// Simple glob pattern match
fn matches_glob_pattern(mediaplayer: &str, other: &str) -> bool {
    // Check if mediaplayer option contains any glob pattern characters
    match mediaplayer {
        mp if mp.starts_with('*') && mp.ends_with('*') && mp.len() > 2 => {
            let infix = &mp[1..mp.len() - 1];
            other.contains(infix)
        }
        mp if mp.ends_with('*') => {
            let prefix = &mp[..mp.len() - 1];
            other.starts_with(prefix)
        }
        mp if mp.starts_with('*') => {
            let suffix = &mp[1..];
            other.ends_with(suffix)
        }
        _ => false,
    }
}

/// Helper function to unpack the media metadata properties artist and title
async fn unpack_metadata(
    metadata: &Value<'_>,
) -> Result<(Option<String>, Option<String>), BoxedError> {
    let dict: Dict = metadata
        .downcast_ref()
        .context("No dictionary of metadata found.")?;
    let title: Option<String> = dict
        .get(&"xesam:title")
        .context("No key for xesam:title found.")?;
    let artist_array: Option<Array> = dict
        .get(&"xesam:artist")
        .context("No key for xesam:artist found.")?;

    // Get the first artist in the artist array
    let artist: Option<String> = if let Some(array) = artist_array {
        array.get(0).context("No artist found in array")?
    } else {
        None
    };

    let title = match title {
        Some(possible_bad_title) => Some(escape_special_characters(possible_bad_title.as_str())),
        None => title,
    };

    let artist = match artist {
        Some(possible_bad_artist) => Some(escape_special_characters(possible_bad_artist.as_str())),
        None => artist,
    };

    Ok((artist, title))
}

// credit for this function goes to reddit user: redartedreddit
// https://www.reddit.com/r/rust/comments/i4bg0q/how_to_escape_strings_in_json_for_example_from/
/// returns a copy of the input buffer. characters considered special in json are escaped such that they dont effect parsers
fn escape_special_characters(src: &str) -> String {
    use std::fmt::Write;
    let mut escaped = String::with_capacity(src.len());
    let mut utf16_buf = [0u16; 2];
    for c in src.chars() {
        match c {
            // pretty sure this is the only escape actually needed, but for completeness i kept the rest
            '"' => escaped += "\\\"",   // Double Quote
            '\\' => escaped += "\\",    // Backslash
            '\t' => escaped += "\\t",   // Tab
            '\x08' => escaped += "\\b", // Backspace
            '\x0c' => escaped += "\\f", // Form Feed
            '\n' => escaped += "\\n",   // Newline
            '\r' => escaped += "\\r",   // Carriage Return
            // if ascii its safe for json output
            c if c.is_ascii_graphic() => escaped.push(c),
            c => {
                let encoded = c.encode_utf16(&mut utf16_buf);
                for utf16 in encoded {
                    write!(&mut escaped, "\\u{:04X}", utf16).unwrap();
                }
            }
        }
    }
    escaped
}

/// Get the first name owner that matches the glob pattern
async fn get_first_match<'a>(
    proxy: &'a DBusProxy<'a>,
    glob_pattern: &'a str,
) -> Result<Option<BusName<'a>>, BoxedError> {
    let all_names: Vec<OwnedBusName> = proxy.list_names().await?;

    let first_matching_name = all_names.iter().find(|name| {
        if let BusName::WellKnown(bus_name) = name.inner() {
            matches_glob_pattern(
                glob_pattern,
                bus_name
                    .as_str()
                    .trim_start_matches("org.mpris.MediaPlayer2."),
            )
        } else {
            false // Skip non WellKnown variants
        }
    });

    Ok(first_matching_name.map(|name| name.inner().to_owned()))
}

/// Get either metadata or playback status from the MPRIS properties
async fn get_property(
    connection: &Connection,
    bus_name: &str,
    property: &str,
) -> Result<Value<'static>, BoxedError> {
    // Create a proxy to help us get properties
    let proxy = Proxy::new(
        connection,
        bus_name,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .await?;

    Ok(proxy.get_property(property).await?)
}

/// Parses arguments and unpacks metadata and playbackstatus as well as completes missing data
async fn parse_msg_args(
    connection: &Connection,
    args: PropertiesChangedArgs<'_>,
    mediaplayer_bus: &str,
) -> Result<Media, BoxedError> {
    // While we can receive metadata or playbackstatus, we never get them both.
    // This is why we for each instance get the missing information to make sure
    // we produce correct output.

    // Handle metadata

    let mut metadata = (None, None);
    let mut playbackstatus = None;

    // Check if metadata is present in the changed properties
    if let Some(metadata_value) = args.changed_properties().get("Metadata") {
        // Then unpack it
        metadata = unpack_metadata(metadata_value).await?;
    } else if let Ok(metadata_value) = get_property(connection, mediaplayer_bus, "Metadata").await {
        // Otherwise we try to fetch it ourselvesand then unpack it
        // This can fail which is fine
        metadata = unpack_metadata(&metadata_value).await?;
    }

    // Then the same procedure for playbackstatus
    if let Some(playbackstatus_value) = args.changed_properties().get("PlaybackStatus") {
        playbackstatus = Some(playbackstatus_value.downcast_ref::<String>()?);
    } else if let Ok(playbackstatus_value) =
        get_property(connection, mediaplayer_bus, "PlaybackStatus").await
    // This can also fail, which is fine
    {
        playbackstatus = Some(playbackstatus_value.downcast::<String>()?);
    }

    Ok(Media::new(metadata.0, metadata.1, playbackstatus))
}

/// Calls a method on the interface to play or pause what is currently playing
async fn toggle_playback(
    connection: &Connection,
    bus_name: &str,
    cmd: &str,
) -> Result<(), BoxedError> {
    // Create a proxy to help us get properties
    let proxy = Proxy::new(
        connection,
        bus_name,
        "/org/mpris/MediaPlayer2",
        "org.mpris.MediaPlayer2.Player",
    )
    .await?;
    Ok(proxy.call_noreply(cmd, &()).await?)
}

/// Start a message stream to listen for property changes
async fn property_changes_stream(
    connection: Connection,
    options: &Arguments,
) -> Result<(), BoxedError> {
    // Define a rule to catch properties changed
    let rule: MatchRule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path("/org/mpris/MediaPlayer2")?
        .build();

    // A proxy to get name owners
    let dbus_proxy = DBusProxy::new(&connection).await?;

    // The mediaplayer bus name, constructed by using the mediaplayer defined by the user, but will be
    let mut mediaplayer_busname: String = if options.glob {
        BusName::null_value().to_owned()
    } else {
        BusName::try_from(format!("org.mpris.MediaPlayer2.{}", options.mediaplayer))
            .context("Invalid busname for mediaplayer.")?
            .to_string()
    };

    let mut property_stream = MessageStream::for_match_rule(
        rule,
        &connection,
        // No big queue needed here
        Some(10),
    )
    .await?;

    // Start catching messages on the stream
    while let Some(Ok(msg)) = property_stream.next().await {
        // If globbing mediaplayers we try to get the first match, but if there is none we skip
        if options.glob {
            match get_first_match(&dbus_proxy, &options.mediaplayer).await {
                Ok(Some(matching_busname)) => {
                    // We update the mediaplayer with the match
                    mediaplayer_busname = matching_busname.to_string();
                }
                _ => {
                    // Skip if no match
                    continue;
                }
            }
        }

        // Start unpacking the properties from the message
        let properties =
            PropertiesChanged::from_message(msg).expect("Failed to unpack changed properties");
        let changed_args = properties.args();

        let changed = changed_args.expect("Failed to get changed properties arguments");

        // TODO Mediaplayers sometimes send more than one message, which is annoying but does not
        // affect the output. Maybe some kind guard would be useful at some point.

        // Get the sender busname of the message so that we can check the unique ID
        let sender = properties
            .message()
            .header()
            .sender()
            .expect("A message should always have a sender")
            .to_owned();

        let sender_busname = BusName::from(sender).to_string();

        // Check if we should listen to all mediaplayers. If so we modify the mediaplayer_bus to whatever is incoming
        // and proceed to unpacking the contents
        if options.mediaplayer.is_empty() {
            sender_busname.clone_into(&mut mediaplayer_busname);
        } else {
            // Getting the name owner errors if our mediaplayer is not open...
            if let Ok(mediaplayer_id) = dbus_proxy
                .get_name_owner(BusName::try_from(mediaplayer_busname.to_owned())?)
                .await
            {
                // If the sender is not a mediaplayer we're after, skip it
                if sender_busname != mediaplayer_id.as_str() {
                    // But first check if we should toggle the playback status
                    if options.autotoggle {
                        // If we should toggle the playback, we get the playbackstatus reported from the other mediaplayer
                        let media = parse_msg_args(&connection, changed, &sender_busname).await?;

                        if let Some(playbackstatus) = media.playbackstatus {
                            // And we send the reverse method call to our mediaplayer
                            match playbackstatus.as_str() {
                                "Playing" => {
                                    toggle_playback(&connection, &mediaplayer_busname, "Pause")
                                        .await?
                                }
                                _ => {
                                    toggle_playback(&connection, &mediaplayer_busname, "Play")
                                        .await?
                                }
                            }
                        }
                    }
                    // Since this is not a mediaplayer we care about, just go next and don't unpack any contents
                    continue;
                }
            } else {
                // ...so in the case that we fail getting the ID of our mediaplayer we skip
                continue;
            }
        }

        // Now parse the arguments and finally send the media output to Waybar
        let media = parse_msg_args(&connection, changed, &mediaplayer_busname).await?;
        media.send(&options.format)
    }
    Ok(())
}
/// Start a message stream receiving info about change of name owners, e.g. mediaplayers closing
async fn name_owner_changed_stream(
    connection: Connection,
    options: &Arguments,
) -> Result<(), BoxedError> {
    let dbus_proxy = DBusProxy::new(&connection).await?;

    // Define a rule to catch properties changed
    let mut name_owner_changed_stream = dbus_proxy.receive_name_owner_changed().await?;

    while let Some(ownership_change) = name_owner_changed_stream.next().await {
        // Unpack the changes in name owner
        let change = ownership_change
            .args()
            .expect("Unpacking the name owner change failed.");

        // Only care about the human readable names that contains MPRIS players
        if let BusName::WellKnown(bus_name) = change.name() {
            if bus_name.contains("org.mpris.MediaPlayer2.") {
                let name = bus_name.trim_start_matches("org.mpris.MediaPlayer2.");

                // Check if the mediaplayer matches, either via glob or direct match
                let matched_player = if options.glob {
                    matches_glob_pattern(&options.mediaplayer, name)
                } else {
                    name == options.mediaplayer
                };

                // A typical message when a mediaplayer closes contains info about the old owner
                // but there is no no new owner, and it should match a player we're interested in.

                // TODO This means that we never clear output if here is no mediaplayer specified,
                // but maybe we should clear it either way?
                if change.old_owner().is_some() && change.new_owner().is_none() && matched_player {
                    // Print empty line and abort the property task if the mediaplayer closes
                    println!();
                }

                // Firefox sometimes appear as a new name owner, with content playing (usually a stream) but does not
                // send any message about it. Therefore we check all non matching players playback status as they appear
                // and toggle playback accordingly.
                if change.old_owner().is_none()
                    && change.new_owner().is_some()
                    && !matched_player
                    && options.autotoggle
                {
                    // Figure out the correct busname to call
                    let mediaplayer_busname = {
                        if options.glob {
                            if let Ok(matched) =
                                get_first_match(&dbus_proxy, &options.mediaplayer).await
                            {
                                matched
                            } else {
                                // This can fail, in that case we skip
                                continue;
                            }
                        } else {
                            Some(BusName::try_from(format!(
                                "org.mpris.MediaPlayer2.{}",
                                options.mediaplayer.as_str()
                            ))?)
                        }
                    };

                    // Then send a command to pause our mediaplayer. Any other status we just ignore.
                    if let Some(mediaplayer_busname) = mediaplayer_busname {
                        let playbackstatus: String =
                            get_property(&connection, bus_name.as_str(), "PlaybackStatus")
                                .await?
                                .downcast_ref()?;
                        if playbackstatus.as_str() == "Playing" {
                            toggle_playback(&connection, &mediaplayer_busname, "Pause").await?
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), BoxedError> {
    // Parse the options supplied by the user
    static OPTIONS: Lazy<Arguments> = Lazy::new(|| match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    });

    // Connect to the session bus
    let connection = Connection::session().await?;

    // Set up streams to handle properties as well as opening/closing mediaplayers
    let property_changes_stream =
        tokio::spawn(property_changes_stream(connection.clone(), &OPTIONS));

    // Only set up a name owner changed stream if user has specified a mediaplayer
    let name_owner_changed_stream = if !OPTIONS.mediaplayer.is_empty() {
        Some(tokio::spawn(name_owner_changed_stream(
            connection.clone(),
            &OPTIONS,
        )))
    } else {
        None
    };

    // Await the tasks
    match name_owner_changed_stream {
        Some(stream) => {
            let (property_changes_result, name_owner_result) =
                tokio::try_join!(property_changes_stream, stream)?;
            property_changes_result?;
            name_owner_result?;
        }
        None => {
            property_changes_stream.await??;
        }
    }

    Ok(())
}
