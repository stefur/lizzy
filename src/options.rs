const HELP: &str = "\
Lystra
USAGE:
  lystra --[OPTIONS] [INPUT]
FLAGS:
  -h, --help            Prints help information
OPTIONS:
  --length NUMBER       Max length of the output before truncating      <Default: 45>
  --signal NUMBER       Signal number used to update Waybar             <Default: 8>
  --playing STRING      Indicator used when a song is playing           <Default: Playing: >
  --paused STRING       Indicator used when a song is paused            <Default: Paused: >
  --separator STRING    A separator between song artist and title       <Default: - >
  --order STRING        The order of artist and title, comma-separated  <Default: artist,title>
  --textcolor STRING    Text color for artist and title                 <Default: None>
  --textcolor STRING    Text color for playback status                  <Default: None>
  --mediaplayer STRING  Mediaplayer interface to pick up signals from   <Default: None>
  --autotoggle          Include this flag for automatic play/pause      <Default: False>
";

#[derive(Clone)]
pub struct Arguments {
    pub length: usize,
    pub signal: u8,
    pub playing: String,
    pub paused: String,
    pub separator: String,
    pub order: String,
    pub textcolor: String,
    pub playbackcolor: String,
    pub mediaplayer: String,
    pub autotoggle: bool,
}

pub fn parse_args() -> Result<Arguments, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let args = Arguments {
        length: pargs.opt_value_from_str("--length")?.unwrap_or(45),
        signal: pargs.opt_value_from_str("--signal")?.unwrap_or(8),
        playing: pargs
            .opt_value_from_str("--playing")?
            .unwrap_or(String::from("Playing: ")),
        paused: pargs
            .opt_value_from_str("--paused")?
            .unwrap_or(String::from("Paused: ")),
        separator: pargs
            .opt_value_from_str("--separator")?
            .unwrap_or(String::from(" - ")),
        order: pargs
            .opt_value_from_str("--order")?
            .unwrap_or(String::from("artist,title")),
        textcolor: pargs
            .opt_value_from_str("--textcolor")?
            .unwrap_or(String::new()),
        playbackcolor: pargs
            .opt_value_from_str("--playbackcolor")?
            .unwrap_or(String::new()),
        mediaplayer: pargs
            .opt_value_from_str("--mediaplayer")?
            .unwrap_or(String::new()),
        autotoggle: pargs.contains("--autotoggle"),
    };

    // It's up to the caller what to do with the remaining arguments.
    let remaining = pargs.finish();
    if !remaining.is_empty() {
        eprintln!("Warning: unused arguments left: {:?}.", remaining);
    }

    Ok(args)
}
