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
    #[clap(long, value_parser, default_value = "Playing:")]
    pub playing: String,
    /// The indicator used when a song is paused
    #[clap(long, value_parser, default_value = "Paused:")]
    pub paused: String,
}
