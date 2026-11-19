//! Writes a generated catalogue of made-up books into an app data folder, so
//! an app can be started against a large library. The folder is not a
//! complete portable copy: see `fixture::generate`.
use ebook_library::*;
use std::path::PathBuf;

fn main() -> Result<()> {
    let mut args = std::env::args_os().skip(1);
    let (Some(folder), Some(count)) = (args.next(), args.next()) else {
        eprintln!("usage: fixture <app data folder> <number of books>");
        std::process::exit(2);
    };
    let count: u32 = count.to_string_lossy().parse()?;
    fixture::generate(&PathBuf::from(folder), count)
}
