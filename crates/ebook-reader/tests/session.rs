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
        "#!/bin/sh\necho start >> '{}'\nexec {SANDBOX} \"$@\"\n",
        log.display()
    );
    ReaderConfig {
        sandbox_path: Some(executable(dir, "counted-bwrap", &script)),
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
        sandbox_path: Some(executable(dir, name, &script)),
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

/// S5, S6: a reply the host refuses ends the helper that sent it, and the
/// next read starts another.
#[test]
fn a_refused_reply_ends_its_helper() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let count = dir.0.join("count");
    // The markup's text is one unit long, not the two the reply states.
    let wrong = Response::Ok(Payload::Content(ChapterContent {
        index: 0,
        title: None,
        markup: "<p>x</p>".into(),
        length: 2,
        styles: vec![],
        language: None,
        right_to_left: false,
        truncated: false,
        warnings: vec![],
    }));
    let wrong = frame(&dir, "wrong.frame", &wrong, &[]);
    let cfg = numbered(&dir, "liar", &count, &send(&wrong));
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    assert!(matches!(
        book.read_content(0),
        Err(ReaderError::HelperFailed { .. })
    ));
    assert_eq!(number(&count), first);
    assert!(book.read_content(0).is_err());
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
    let cfg = numbered(&dir, "stuck", &count, &format!("exec {SLEEP} 30"));
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
        sandbox_path: Some(executable(&dir, "pid-bwrap", &script)),
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
    let alive = || alive(&pid);
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

/// F6: a helper that writes replies nobody asked for is not read ahead of the
/// requests, so the host holds at most one reply's worth of them.
#[test]
fn replies_nobody_asked_for_are_not_read_ahead() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let written = dir.0.join("written");
    let ready = Response::Ok(Payload::Probe(ebook_reader::wire::ProbeReport {
        isolated: true,
    }));
    let flood = frame(&dir, "flood.frame", &ready, &[7; 64 * 1024]);
    let script = format!(
        "#!/bin/sh\necho '0 {{\"Ok\":{{\"Probe\":{{\"isolated\":true}}}}}}'\nwhile read -r request; do\ncase \"$request\" in\nopen) {open}; while cat '{flood}'; do echo x >> '{written}'; done;;\nesac\ndone\n",
        open = send(&outline_frame(&dir)),
        flood = flood.display(),
        written = written.display(),
    );
    let cfg = ReaderConfig {
        sandbox_path: Some(executable(&dir, "flood-bwrap", &script)),
        ..config()
    };
    let book = reader_over(&cfg).open(&book_path).unwrap();
    std::thread::sleep(Duration::from_secs(1));
    let frames = std::fs::read_to_string(&written).map_or(0, |s| s.lines().count());
    // The pipe and one read buffer hold a few; reading ahead would take hundreds.
    assert!(frames <= 16, "{frames} unasked replies were read");
    drop(book);
}

/// S5: a read that finds its book's helper busy with a slow read uses a
/// helper of its own instead of waiting for it.
#[test]
fn a_slow_read_holds_up_no_other() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>", "<p>y</p>"], &[]));
    let count = dir.0.join("count");
    let reply = send(&content_frame(&dir));
    let mut cfg = numbered(
        &dir,
        "slow",
        &count,
        &format!("[ \"$request\" = 'content 0' ] && sleep 3; {reply}"),
    );
    cfg.timeout = Duration::from_secs(10);
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    let slow = {
        let book = book.clone();
        std::thread::spawn(move || book.read_content(0).map(|c| c.length))
    };
    std::thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    assert_eq!(book.read_content(1).unwrap().length, 1);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(number(&count), first + 1);
    assert_eq!(slow.join().unwrap().unwrap(), 1);
    // The book keeps its own helper for the reads after.
    assert_eq!(book.read_content(1).unwrap().length, 1);
    assert_eq!(number(&count), first + 1);
}

/// F6, S5: a reply over the size limit is refused and not asked for again.
#[test]
fn an_oversized_reply_is_not_asked_for_again() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let count = dir.0.join("count");
    let ready = Response::Ok(Payload::Probe(ebook_reader::wire::ProbeReport {
        isolated: true,
    }));
    let big = frame(&dir, "big.frame", &ready, &[1; 8192]);
    let mut cfg = numbered(&dir, "big", &count, &send(&big));
    cfg.limits.max_output_bytes = 4096;
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    assert!(matches!(
        book.read_content(0),
        Err(ReaderError::LimitExceeded { .. })
    ));
    assert_eq!(number(&count), first);
}

/// S5: an error only a helper that is not working as written would send ends
/// that helper, and a book's own error keeps it.
#[test]
fn a_helper_that_answers_out_of_turn_is_ended() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>", "<p>y</p>"], &[]));
    let count = dir.0.join("count");
    let usage = dir.file(
        "usage.frame",
        b"0 {\"Err\":{\"kind\":\"Usage\",\"detail\":\"x\"}}\n",
    );
    let invalid = dir.file(
        "invalid.frame",
        b"0 {\"Err\":{\"kind\":\"Invalid\",\"detail\":\"missing file in book: c1.xhtml\"}}\n",
    );
    let cfg = numbered(
        &dir,
        "odd",
        &count,
        &format!(
            "if [ \"$request\" = 'content 1' ]; then {}; else {}; fi",
            send(&invalid),
            send(&usage)
        ),
    );
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    assert!(matches!(
        book.read_content(1),
        Err(ReaderError::InvalidBook { .. })
    ));
    assert_eq!(number(&count), first);
    assert!(matches!(
        book.read_content(0),
        Err(ReaderError::HelperFailed { .. })
    ));
    assert!(book.read_content(1).is_err());
    assert_eq!(number(&count), first + 1);
}

/// F3, S5: a helper that closes its replies and hangs runs out the call's
/// time, and is not tried again once the time is gone.
#[test]
fn a_helper_that_goes_quiet_is_not_tried_again() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let count = dir.0.join("count");
    let cfg = numbered(
        &dir,
        "quiet",
        &count,
        &format!("exec 1>&-; exec {SLEEP} 30"),
    );
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let first = number(&count);
    let started = Instant::now();
    assert_eq!(book.read_content(0).unwrap_err(), ReaderError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5));
    assert_eq!(number(&count), first);
}

/// S5: a helper that stops reading requests is ended when its input is full,
/// instead of holding the call past its deadline.
#[test]
fn a_helper_that_stops_reading_requests_is_ended() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let reply = std::fs::read(content_frame(&dir)).unwrap();
    let flood = dir.file("replies", &reply.repeat(256));
    let script = format!(
        "#!/bin/sh\necho '0 {{\"Ok\":{{\"Probe\":{{\"isolated\":true}}}}}}'\nread -r request\n{open}\nwhile cat '{flood}'; do :; done\n",
        open = send(&outline_frame(&dir)),
        flood = flood.display(),
    );
    let cfg = ReaderConfig {
        sandbox_path: Some(executable(&dir, "deaf-bwrap", &script)),
        timeout: Duration::from_secs(2),
        ..config()
    };
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let ended = (0..20_000).find_map(|_| {
        let started = Instant::now();
        book.read_content(0)
            .err()
            .map(|error| (error, started.elapsed()))
    });
    match ended {
        Some((ReaderError::HelperFailed { detail }, elapsed)) => {
            assert!(detail.contains("stopped reading"), "{detail}");
            assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
        }
        other => panic!("{other:?}"),
    }
}

/// S5: however many calls find a book's helper busy, at most four run in
/// helpers of their own at once; the rest wait within their deadline.
#[test]
fn a_book_runs_at_most_four_spare_helpers() {
    let dir = Scratch::new();
    let book_path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let count = dir.0.join("count");
    let asked = dir.0.join("asked");
    let mut cfg = numbered(
        &dir,
        "stuck-all",
        &count,
        &format!("echo x >> '{}'; exec {SLEEP} 30", asked.display()),
    );
    cfg.timeout = Duration::from_secs(2);
    let book = reader_over(&cfg).open(&book_path).unwrap();
    let started = Instant::now();
    let calls: Vec<_> = (0..10)
        .map(|_| {
            let book = book.clone();
            std::thread::spawn(move || book.read_content(0))
        })
        .collect();
    // Every helper there will be until the first deadlines has been asked by
    // now: the book's own and the four spares. Nothing ends before then.
    std::thread::sleep(Duration::from_millis(1500));
    let early = std::fs::read_to_string(&asked).map_or(0, |s| s.lines().count());
    for call in calls {
        assert_eq!(call.join().unwrap().unwrap_err(), ReaderError::Timeout);
    }
    assert!(
        started.elapsed() < Duration::from_secs(6),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(early, 5, "the book's helper and four spares");
}

/// F6: a reply allowance of nothing refuses at once instead of waiting.
#[test]
fn a_reply_allowance_of_nothing_refuses_at_once() {
    let mut cfg = config();
    cfg.limits.max_output_bytes = 0;
    let started = Instant::now();
    assert!(matches!(
        Reader::new(cfg),
        Err(ReaderError::SandboxUnavailable { .. })
    ));
    assert!(started.elapsed() < Duration::from_secs(5));
}
