const HELP: &str = r#"lizzy
=====

USAGE:
  lizzy --[OPTIONS] [INPUT]
FLAGS:
  -h, --help            Prints help information
OPTIONS:
  --format STRING       The format of output using handlebar tags       <Default: "{{artist}} - {{title}}">
  --mediaplayer STRING  Mediaplayer interface to pick up signals from   <Default: None>
  --autotoggle          Include this flag for automatic play/pause      <Default: False>
"#;

pub struct Arguments {
    pub format: String,
    pub mediaplayer: String,
    pub autotoggle: bool,
    pub glob: bool,
}

/// Get the user arguments
pub fn parse_args() -> Result<Arguments, pico_args::Error> {
    let mut pargs = pico_args::Arguments::from_env();

    // Help has a higher priority and should be handled separately.
    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    // Extract mediaplayer first to use it for glob determination
    let mediaplayer: String = pargs
        .opt_value_from_str("--mediaplayer")?
        .unwrap_or_else(String::new);

    // Check for glob
    let glob = mediaplayer.contains('*');

    let args = Arguments {
        format: pargs
            .opt_value_from_str("--format")?
            .unwrap_or(String::from("{{artist}} - {{title}}")),
        mediaplayer,
        autotoggle: pargs.contains("--autotoggle"),
        glob,
    };

    // It's up to the caller what to do with the remaining arguments.
    let remaining = pargs.finish();
    if !remaining.is_empty() {
        eprintln!("Warning: unused arguments left: {:?}.", remaining);
    }

    Ok(args)
}
