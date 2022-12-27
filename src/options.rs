use clap::Parser;

#[derive(Parser)]
#[clap()]
pub struct Args {
    /// Max length of the output before truncating
    #[clap(long, value_parser, default_value_t = 45)]
    pub length: usize,
    /// Signal number used to update Waybar
    #[clap(long, value_parser, default_value_t = 8)]
    pub signal: u8,
    /// The indicator used when a song is playing
    #[clap(long, value_parser, default_value = "Playing: ")]
    pub playing: String,
    /// The indicator used when a song is paused
    #[clap(long, value_parser, default_value = "Paused: ")]
    pub paused: String,
    /// A separator between song artist and title
    #[clap(long, value_parser, default_value = " - ")]
    pub separator: String,
    /// The order of artist and title value, comma-separated
    #[clap(long, value_parser, default_value = "artist,title")]
    pub order: String,
    /// A specific text color for artist and title
    #[clap(long, value_parser)]
    pub textcolor: Option<String>,
    /// A specific color for the playback status
    #[clap(long, value_parser)]
    pub playbackcolor: Option<String>,
}
