use ebook_convert::{start, Config, Error, Metadata, Request};
use ebook_reader::{Reader, ReaderConfig};
use std::path::{Path, PathBuf};

fn config() -> Config {
    let helper = PathBuf::from(env!("CARGO_BIN_EXE_ebook-convert-helper"));
    Config {
        reader: ReaderConfig {
            helper_path: Some(helper.with_file_name("ebook-reader-helper")),
            ..Default::default()
        },
        helper,
    }
}

fn request(dir: &Path) -> Request {
    Request {
        source: dir.join("source.mobi"),
        destination: dir.join("converted.epub"),
        identity: "book-7/version-9".into(),
        metadata: Metadata {
            title: Some("A catalogue title & subtitle".into()),
            authors: Some(vec!["Catalogue Author".into()]),
            language: Some("en".into()),
        },
        cover: None,
    }
}

#[test]
fn e1_invalid_input_or_failed_sandbox_does_not_publish() {
    let dir = tempfile::tempdir().unwrap();
    let req = request(dir.path());
    std::fs::write(&req.source, b"unreadable input").unwrap();
    let config = config();
    assert!(start(req.clone(), config.clone()).wait().is_err());
    let mut missing = config;
    missing.helper = dir.path().join("absent");
    assert!(start(req.clone(), missing).wait().is_err());
    assert!(!req.destination.exists());
    assert_eq!(std::fs::read(req.source).unwrap(), b"unreadable input");
}

#[test]
fn e4_only_one_racing_job_publishes_a_checked_readable_copy() {
    let dir = tempfile::tempdir().unwrap();
    let req = request(dir.path());
    let source = include_bytes!("mobi/sample.mobi");
    std::fs::write(&req.source, source).unwrap();
    let cfg = config();
    let first = start(req.clone(), cfg.clone());
    let second = start(req.clone(), cfg.clone());
    let mut results = [first.wait(), second.wait()];
    results.sort_by_key(|r| r.is_err());
    let winner = results[0].as_ref().unwrap();
    assert!(matches!(results[1], Err(Error::Exists)));
    assert_eq!(winner.report.identity, req.identity);
    assert_eq!(
        winner.report.size,
        std::fs::metadata(&req.destination).unwrap().len()
    );
    assert_eq!(std::fs::read(&req.source).unwrap(), source);
    let book = Reader::new(cfg.reader).unwrap().open(&winner.path).unwrap();
    assert_eq!(
        book.metadata().title.as_deref(),
        Some("A catalogue title & subtitle")
    );
    assert_eq!(book.metadata().authors[0].name, "Catalogue Author");
    let titles: Vec<_> = book
        .chapters()
        .iter()
        .map(|c| c.title.as_deref().unwrap())
        .collect();
    assert_eq!(
        titles,
        [
            "Chapter One: The Lamp",
            "Chapter Two: The Storm",
            "Chapter Three: Morning"
        ]
    );
    let page = book.read_chapter(0).unwrap();
    let text: String = page
        .blocks
        .iter()
        .flat_map(|b| b.spans.iter().map(|s| s.text.as_str()))
        .collect();
    assert!(text.contains("Marta Ruiz & her cat"));
    assert!(winner.report.losses.iter().any(|loss| loss.contains("GIF")));
}
