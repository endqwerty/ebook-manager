//! How many helpers a book uses, and what happens when one ends. Statement
//! numbers refer to specs/epub-reader-mvp.md.

mod common;

use common::*;
use ebook_reader::wire::{BookOutline, Payload, Response};
use ebook_reader::{ChapterContent, ChapterInfo, Metadata, Reader, ReaderConfig, ReaderError};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// A program written a moment ago can be "text file busy" while another test
/// thread is between fork and exec. Try again until that passes.
fn reader_over(cfg: &ReaderConfig) -> Reader {
    for _ in 0..50 {
        match Reader::new(cfg.clone()) {
            Ok(reader) => return reader,
            Err(e) if format!("{e:?}").to_lowercase().contains("busy") => {
                std::thread::sleep(Duration::from_millis(20))
            }
            Err(e) => panic!("{e:?}"),
        }
    }
    panic!("the stand-in never ran");
}

fn executable(dir: &Scratch, name: &str, script: &str) -> PathBuf {
    let path = dir.file(name, script.as_bytes());
    std::process::Command::new("/bin/chmod")
        .arg("+x")
        .arg(&path)
        .status()
        .unwrap();
    path
}

/// The real bubblewrap, noting each start in `log`.
fn counted_bwrap(dir: &Scratch, log: &Path) -> ReaderConfig {
    let script = format!(
        "#!/bin/sh\necho start >> '{}'\nexec /usr/bin/bwrap \"$@\"\n",
        log.display()
    );
    ReaderConfig {
        bwrap_path: Some(executable(dir, "counted-bwrap", &script)),
        ..config()
    }
}

fn starts(log: &Path) -> usize {
    std::fs::read_to_string(log).map_or(0, |s| s.lines().count())
}

/// S5: an open book reads every chapter through one helper, even when the
/// thread that opened it has ended since.
#[test]
fn a_book_reads_through_one_helper_that_outlives_the_opening_thread() {
    let dir = Scratch::new();
    let log = dir.0.join("starts");
    let cfg = counted_bwrap(&dir, &log);
    let path = dir.file(
        "b.epub",
        &simple(&["<p>one</p>", "<p>two</p>", "<p>three</p>"], &[]),
    );
    let reader = reader_over(&cfg);
    let proven = starts(&log);
    let book = std::thread::spawn(move || reader.open(&path).unwrap())
        .join()
        .unwrap();
    assert_eq!(starts(&log), proven + 1);
    for round in 0..3 {
        for index in 0..3 {
            let content = book.read_content(index).unwrap();
            assert!(content
                .markup
                .contains(["one", "two", "three"][index as usize]));
            assert_eq!(book.read_chapter(index).unwrap().index, index);
        }
        assert_eq!(starts(&log), proven + 1, "round {round}");
    }
    // The sandbox was proven once for this configuration.
    reader_over(&cfg);
    assert_eq!(starts(&log), proven + 1);
}

fn outline_frame(dir: &Scratch) -> PathBuf {
    let outline = Response::Ok(Payload::Book(BookOutline {
        metadata: Metadata::default(),
        chapters: (0..2)
            .map(|index| ChapterInfo {
                index,
                title: None,
                linear: true,
                size: 10,
            })
            .collect(),
        toc: vec![],
    }));
    frame(dir, "outline.frame", &outline, &[])
}

fn content_frame(dir: &Scratch) -> PathBuf {
    let content = Response::Ok(Payload::Content(ChapterContent {
        index: 0,
        title: None,
        markup: "<p>x</p>".into(),
        length: 1,
        styles: vec![],
        language: None,
        right_to_left: false,
        truncated: false,
        warnings: vec![],
    }));
    frame(dir, "content.frame", &content, &[])
}

/// A stand-in sandbox that numbers its starts in `count` and runs `content`
/// for a chapter request, with `$n` set to its own number.
fn numbered(dir: &Scratch, name: &str, count: &Path, content: &str) -> ReaderConfig {
    let script = format!(
        "#!/bin/sh\nn=$(($(cat '{count}' 2>/dev/null || echo 0) + 1))\necho $n > '{count}'\necho '0 {{\"Ok\":{{\"Probe\":{{\"isolated\":true}}}}}}'\nwhile read -r request; do\ncase \"$request\" in\nopen) {open};;\ncontent*) {content};;\nesac\ndone\n",
        count = count.display(),
        open = send(&outline_frame(dir)),
    );
    ReaderConfig {
        bwrap_path: Some(executable(dir, name, &script)),
        timeout: Duration::from_millis(500),
        ..config()
    }
}

fn number(count: &Path) -> u32 {
    std::fs::read_to_string(count)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}

/// S5: when a helper that had answered ends, the read is tried once in a
/// fresh helper, which the book then keeps.
#[test]
fn a_helper_that_ends_after_answering_is_replaced_once() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>", "<p>y</p>"], &[]));
    let count = dir.0.join("count");
    // The book's first helper ends on its second chapter.
    let reply = send(&content_frame(&dir));
    let cfg = numbered(
        &dir,
        "flaky",
        &count,
        &format!("[ $n = 2 ] && [ \"$request\" = 'content 1' ] && exit 1; {reply}"),
    );
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    assert_eq!(book.read_content(0).unwrap().length, 1);
    assert_eq!(book.read_content(1).unwrap().length, 1);
    assert_eq!(number(&count), first + 1);
    assert_eq!(book.read_content(0).unwrap().length, 1);
    assert_eq!(number(&count), first + 1);
}

/// S5: a read that ends every helper fails after one fresh try, and each
/// later read tries one fresh helper of its own.
#[test]
fn a_read_that_always_ends_its_helper_is_tried_once_more() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let count = dir.0.join("count");
    let cfg = numbered(&dir, "fatal", &count, "echo 'helper: stopped' >&2; exit 4");
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    match book.read_content(0) {
        Err(ReaderError::HelperFailed { detail }) => {
            assert!(detail.starts_with("exit status: 4;"), "{detail}");
            assert!(detail.contains("helper: stopped"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(number(&count), first + 1);
    assert!(book.read_content(0).is_err());
    assert_eq!(number(&count), first + 2);
}

/// F3, S5: a read that runs out of time is not tried again.
#[test]
fn a_read_past_its_deadline_is_not_tried_again() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let count = dir.0.join("count");
    let cfg = numbered(&dir, "stuck", &count, "exec /usr/bin/sleep 30");
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    let started = Instant::now();
    assert_eq!(book.read_content(0).unwrap_err(), ReaderError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(number(&count), first);
}

/// S3, S5: dropping the last copy of a book ends its helper.
#[test]
fn dropping_a_book_ends_its_helper() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let pids = dir.0.join("pids");
    let script = format!(
        "#!/bin/sh\necho $$ >> '{pids}'\necho '0 {{\"Ok\":{{\"Probe\":{{\"isolated\":true}}}}}}'\nwhile read -r request; do\ncase \"$request\" in\nopen) {open};;\nesac\ndone\n",
        pids = pids.display(),
        open = send(&outline_frame(&dir)),
    );
    let cfg = ReaderConfig {
        bwrap_path: Some(executable(&dir, "pid-bwrap", &script)),
        ..config()
    };
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let copy = book.clone();
    let pid = std::fs::read_to_string(&pids)
        .unwrap()
        .lines()
        .last()
        .unwrap()
        .to_owned();
    let alive = || Path::new(&format!("/proc/{pid}")).exists();
    drop(book);
    std::thread::sleep(Duration::from_millis(100));
    assert!(alive(), "a copy of the book still holds the helper");
    drop(copy);
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive() {
        assert!(Instant::now() < deadline, "helper {pid} outlived its book");
        std::thread::sleep(Duration::from_millis(10));
    }
}
