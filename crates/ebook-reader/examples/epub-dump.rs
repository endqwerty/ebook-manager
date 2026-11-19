//! Print a book the way a reading interface would walk it. A manual and
//! packaging check: it finds the helper by the documented installed layout.
//!
//! Usage: epub-dump <book.epub> [chapter-number]

use ebook_reader::{BlockKind, Reader, ReaderConfig};
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(path) = args.get(1) else {
        eprintln!("usage: epub-dump <book.epub> [chapter-number]");
        return ExitCode::from(2);
    };
    match run(Path::new(path), args.get(2).and_then(|n| n.parse::<u32>().ok())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

fn run(path: &Path, only: Option<u32>) -> Result<(), ebook_reader::ReaderError> {
    let reader = Reader::new(ReaderConfig::default())?;
    let book = reader.open(path)?;
    let meta = book.metadata();
    println!("title: {}", meta.title.as_deref().unwrap_or("(none)"));
    for a in &meta.authors {
        println!("author: {}", a.name);
    }
    for i in &meta.identifiers {
        println!("identifier: {}:{}", i.scheme.as_deref().unwrap_or("?"), i.value);
    }
    println!("chapters: {}", book.chapters().len());
    for entry in book.toc() {
        let to = entry.target.as_ref().map(|t| format!(" -> {}", t.chapter + 1)).unwrap_or_default();
        println!("toc: {}{}{}", "  ".repeat(entry.depth as usize), entry.title, to);
    }
    for info in book.chapters() {
        if only.is_some_and(|n| n != info.index + 1) {
            continue;
        }
        let chapter = book.read_chapter(info.index)?;
        println!("\n=== {} · {} ===", info.index + 1, chapter.title.as_deref().unwrap_or("untitled"));
        for block in &chapter.blocks {
            match block.kind {
                BlockKind::Heading { level } => println!("{} {}", "#".repeat(level as usize), block.text()),
                BlockKind::ListItem { ordinal: Some(n), .. } => println!("{n}. {}", block.text()),
                BlockKind::ListItem { .. } => println!("- {}", block.text()),
                BlockKind::Quote => println!("> {}", block.text()),
                BlockKind::Separator => println!("* * *"),
                BlockKind::Image => println!("[picture: {}]", block.text()),
                _ => println!("{}", block.text()),
            }
        }
    }
    Ok(())
}
