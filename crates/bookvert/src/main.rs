//! A tool to perform batch conversion of audio.
//!
//! See [`bookvert`] documentation for more information.
//!
//! [`bookvert`]: https://crates.io/crates/bookvert

use anyhow::Result;
use clap::Parser;

const VERSION: &str = match option_env!("MEDIAVERT_VERSION") {
    Some(v) => v,
    None => env!("CARGO_PKG_VERSION"),
};

/// A tool to perform batch conversion of books.
#[derive(Parser)]
#[command(author, version, about, max_term_width = 80, version = VERSION)]
struct Opts {
    #[command(flatten)]
    inner: bookvert::cli::Bookvert,
}

fn main() -> Result<()> {
    let opts = Opts::parse();
    bookvert::cli::entry(&opts.inner)
}
