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

#[derive(Clone)]
pub struct Arguments {
    pub format: String,
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
        format: pargs
            .opt_value_from_str("--format")?
            .unwrap_or(String::from("{{artist}} - {{title}}")),
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
