use clap::Parser;

#[derive(Parser)]
#[clap()]
pub struct Args {
    /// Max length of the output before truncating
    #[clap(short, long, value_parser, default_value_t = 45)]
    pub length: usize,
}
