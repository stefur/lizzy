use media::Media;
use once_cell::sync::Lazy;
use options::Arguments;
use std::error::Error;
use zbus::export::futures_util::stream::StreamExt;
use zbus::fdo::DBusProxy;
use zbus::fdo::PropertiesChanged;
use zbus::fdo::PropertiesChangedArgs;
use zbus::names::BusName;
use zbus::names::OwnedBusName;
use zbus::zvariant::Array;
use zbus::zvariant::Dict;
use zbus::zvariant::Value;
use zbus::Connection;

use zbus::MatchRule;
use zbus::MessageStream;
use zbus::Proxy;
mod media;
mod options;

/// Simple glob pattern match
fn matches_pattern(mediaplayer: &str, other: &str) -> bool {
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
) -> Result<(Option<String>, Option<String>), Box<dyn Error + Send + Sync>> {
    let dict: Dict = metadata.downcast_ref()?;
    let title: Option<String> = dict.get(&"xesam:title")?;
    let artist_array: Option<Array> = dict.get(&"xesam:artist")?;
    // Get the first artist in the artist array
    let artist: Option<String> = match artist_array {
        Some(array) => array.get(0)?,
        None => None,
    };

    Ok((artist, title))
}

async fn unpack_playbackstatus(
    changed: &PropertiesChangedArgs<'_>,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    if let Some(playbackstatus) = changed.changed_properties().get("PlaybackStatus") {
        let playbackstatus: String = playbackstatus.downcast_ref()?;
        Ok(Some(playbackstatus))
    } else {
        Ok(None)
    }
}

/// Get the first name owner that matches the glob pattern
async fn get_first_match(
    proxy: &DBusProxy<'_>,
    glob_pattern: &str,
) -> Result<Option<String>, Box<dyn Error + Send + Sync>> {
    let all_names: Vec<OwnedBusName> = proxy.list_names().await?;

    let first_matching_name = all_names.iter().find(|name| {
        if let BusName::WellKnown(bus_name) = name.inner() {
            matches_pattern(
                glob_pattern,
                bus_name
                    .as_str()
                    .trim_start_matches("org.mpris.MediaPlayer2."),
            )
        } else {
            false // Skip non WellKnown variants
        }
    });

    Ok(first_matching_name.map(|name| name.inner().to_string()))
}

/// Get either metadata or playback status from the MPRIS properties
async fn get_property(
    connection: &Connection,
    bus_name: &str,
    property: &str,
) -> Result<Value<'static>, Box<dyn Error + Send + Sync>> {
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

/// Calls a method on the interface to play or pause what is currently playing
async fn toggle_playback(
    connection: &Connection,
    bus_name: &str,
    cmd: &str,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
    glob: bool,
    options: &Arguments,
    mut mediaplayer_bus: String,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    // Define a rule to catch properties changed
    let rule: MatchRule = MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .interface("org.freedesktop.DBus.Properties")?
        .member("PropertiesChanged")?
        .path("/org/mpris/MediaPlayer2")?
        .build();

    // A proxy to get name owners
    let dbus_proxy = DBusProxy::new(&connection).await?;

    let mut property_stream = MessageStream::for_match_rule(
        rule,
        &connection,
        // No big queue needed here
        Some(1),
    )
    .await?;

    // Start catching messages on the stream
    while let Some(Ok(msg)) = property_stream.next().await {
        // If globbing mediaplayers we try to get the first match, but if there is none we skip
        if glob {
            match get_first_match(&dbus_proxy, &options.mediaplayer).await {
                Ok(Some(matching_player)) => {
                    // We update the mediaplayer with the match
                    mediaplayer_bus = matching_player;
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

        // Get the header of the message so that we can check ID of the sender
        let header = properties.message().header();
        let sender = header.sender().unwrap().as_str();

        // Check if we should listen to all mediaplayers
        if !options.mediaplayer.is_empty() {
            // Create a BusName to get the ID from
            let bus_name = BusName::try_from(mediaplayer_bus.to_owned())?;
            let mediaplayer_id = dbus_proxy.get_name_owner(bus_name).await?;

            // If the sender is not a mediaplayer we're after, skip it
            if sender != mediaplayer_id.as_str() {
                // But first check if we should toggle the playback status
                if options.autotoggle {
                    // If we should toggle the playback, we get the playbackstatus reported from the other mediaplayer
                    let playbackstatus: Option<String> = unpack_playbackstatus(&changed).await?;

                    // And we send the reverse method call to our mediaplayer
                    match playbackstatus.unwrap_or_default().as_str() {
                        "Playing" => {
                            toggle_playback(&connection, &mediaplayer_bus, "Pause").await?
                        }
                        "Paused" => toggle_playback(&connection, &mediaplayer_bus, "Play").await?,
                        _ => (),
                    }
                }
                // Since this is not a mediaplayer we care about, just go next
                continue;
            }
        } else {
            // If there is no mediaplayer defined, we just use the busname from whatever sender comes through
            mediaplayer_bus = sender.to_owned();
        }

        // Handle metadata
        if let Some(metadata) = changed.changed_properties().get("Metadata") {
            // Unpack it into a tuple of (artist, title)
            let metadata = unpack_metadata(metadata).await?;

            let playbackstatus: String =
                get_property(&connection, &mediaplayer_bus, "PlaybackStatus")
                    .await?
                    .downcast_ref()?;

            // While we have received the metadata, get the playbackstatus as well to make sure
            // the output is correct
            let media = Media::new(
                metadata.0.unwrap_or_default(),
                metadata.1.unwrap_or_default(),
                playbackstatus,
            );

            // Send the output to Waybar for display
            media.send(&options.format);
        }

        // Handle playbackstatus
        if let Some(playbackstatus) = changed.changed_properties().get("PlaybackStatus") {
            let playbackstatus: String = playbackstatus.downcast_ref()?;

            // Getting the playbackstatus sometimes still leaves the rest of the metadata
            // outdated depending on the mediaplayer behavior. So to make sure we're printing the
            // correct output we grab the rest of the metadata as well.
            let metadata: Value = get_property(&connection, &mediaplayer_bus, "Metadata").await?;
            let metadata = unpack_metadata(&metadata).await?;

            let media = Media::new(
                metadata.0.unwrap_or_default(),
                metadata.1.unwrap_or_default(),
                playbackstatus,
            );

            media.send(&options.format);
        }
    }

    Ok(())
}
/// Start a message stream receiving info about change of name owners, e.g. mediaplayers closing
async fn name_owner_changed_stream(
    connection: Connection,
    mediaplayer: &String,
    glob: bool,
) -> Result<(), Box<dyn Error + Send + Sync>> {
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
                let matched_player = if glob {
                    matches_pattern(&mediaplayer, name)
                } else {
                    name == mediaplayer
                };

                // A typical message when a mediaplayer closes contains info about the old owner
                // but there is no no new owner, and it should match a player we're interested in.

                // TODO This means that we never clear output if here is no mediaplayer specified,
                // but maybe we should clear it either way?
                if change.old_owner().is_some() && change.new_owner().is_none() && matched_player {
                    // Print empty line and abort the property task if the mediaplayer closes
                    println!();
                }
            }
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    // Parse the options supplied by the user
    static OPTIONS: Lazy<Arguments> = Lazy::new(|| match options::parse_args() {
        Ok(value) => value,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    });

    // Check if theres a glob pattern to match
    let glob: bool = OPTIONS.mediaplayer.contains('*');

    // Connect to the session bus
    let connection = Connection::session().await?;

    // The mediaplayer bus name, constructed by using the mediaplayer defined by the user
    let mediaplayer_bus = format!("org.mpris.MediaPlayer2.{}", OPTIONS.mediaplayer);

    // Set up two streams to handle properties as well as opening/closing mediaplayers
    let property_changes_stream = tokio::spawn(property_changes_stream(
        connection.clone(),
        glob,
        &OPTIONS,
        mediaplayer_bus,
    ));

    let name_owner_changed_stream = tokio::spawn(name_owner_changed_stream(
        connection.clone(),
        &OPTIONS.mediaplayer,
        glob,
    ));

    let _ = name_owner_changed_stream.await;
    let _ = property_changes_stream.await;

    Ok(())
}
