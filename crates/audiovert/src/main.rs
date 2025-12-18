//! A tool to perform batch conversion of audio.
//!
//! See [`audiovert`] documentation for more information.
//!
//! [`audiovert`]: https://crates.io/crates/audiovert

use anyhow::Result;
use clap::Parser;

const VERSION: &str = match option_env!("MEDIAVERT_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// A tool to perform batch conversion of audio.
#[derive(Parser)]
#[command(author, version, about, max_term_width = 80, version = VERSION)]
pub struct Opts {
    #[command(flatten)]
    inner: audiovert::cli::Audiovert,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    audiovert::cli::entry(&opts.inner)
}
