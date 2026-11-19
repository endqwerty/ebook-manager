//! Refusals, limits and the sandbox boundary. Statement numbers refer to
//! specs/epub-reader-mvp.md.

mod common;

use common::*;
use ebook_reader::{Reader, ReaderConfig, ReaderError, Unsupported};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

fn open_err(bytes: &[u8]) -> ReaderError {
    let dir = Scratch::new();
    reader().open(&dir.file("b.epub", bytes)).unwrap_err()
}

/// S1: no helper, no bubblewrap or a sandbox that does not start: no reader.
#[test]
fn reader_fails_closed_without_a_working_sandbox() {
    let missing = ReaderConfig { helper_path: Some("/nonexistent/ebook-reader-helper".into()), ..ReaderConfig::default() };
    assert!(matches!(Reader::new(missing), Err(ReaderError::HelperNotFound { .. })));

    let no_bwrap = ReaderConfig { bwrap_path: Some("/nonexistent/bwrap".into()), ..config() };
    assert!(matches!(Reader::new(no_bwrap), Err(ReaderError::SandboxUnavailable { .. })));

    // A "bwrap" that sandboxes nothing: it exits without running the helper.
    for stand_in in ["/usr/bin/false", "/usr/bin/true"] {
        let cfg = ReaderConfig { bwrap_path: Some(stand_in.into()), ..config() };
        assert!(matches!(Reader::new(cfg), Err(ReaderError::SandboxUnavailable { .. })), "{stand_in}");
    }

    // A "bwrap" that runs the helper with no sandbox at all: the helper's own
    // check refuses, so the reader is still not created.
    let dir = Scratch::new();
    let fake = dir.file("fake-bwrap", b"#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift; shift\nexec \"$HELPER\" \"$@\"\n");
    Command::new("/bin/chmod").arg("+x").arg(&fake).status().unwrap();
    // The fake needs the helper path; env is cleared by the host, so bake it in.
    let script = format!("#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift; shift\nexec '{HELPER}' \"$@\"\n");
    std::fs::write(&fake, script).unwrap();
    let cfg = ReaderConfig { bwrap_path: Some(fake), ..config() };
    match Reader::new(cfg) {
        Err(ReaderError::SandboxUnavailable { reason }) => assert!(reason.contains("visible"), "{reason}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

/// S2: run directly, outside any sandbox, the helper will not touch a book.
#[test]
fn helper_refuses_to_parse_outside_a_sandbox() {
    for op in ["probe", "metadata", "open"] {
        let out = Command::new(HELPER).arg(op).output().unwrap();
        let reply = String::from_utf8_lossy(&out.stdout);
        assert!(reply.contains("NotIsolated"), "{op}: {reply}");
    }
}

/// S3: inside the sandbox there is no network, no home, no writable disk, and
/// no host file other than the one book. Checked from inside with a shell.
#[test]
fn sandbox_has_no_network_and_sees_only_the_book() {
    let dir = Scratch::new();
    let book = dir.file("b.epub", &epub3());
    let secret = dir.file("secret.txt", b"private");
    // Same arguments the host uses, with a shell in place of the helper.
    let mut cmd = Command::new("/usr/bin/bwrap");
    cmd.args(["--unshare-all", "--die-with-parent", "--new-session", "--clearenv", "--cap-drop", "ALL"]);
    for d in ["/usr/lib", "/usr/lib64", "/lib", "/lib64"] {
        cmd.args(["--ro-bind-try", d, d]);
    }
    cmd.args(["--ro-bind", "/bin/sh", "/helper"]).arg("--ro-bind").arg(&book).arg("/book.epub");
    cmd.args(["--remount-ro", "/", "--chdir", "/", "--", "/helper", "-c"]);
    cmd.arg(format!(
        "for p in /home /etc /proc /tmp /dev /run /usr/bin '{}' '{}'; do [ -e \"$p\" ] && echo \"SEES $p\"; done; \
         [ -r /book.epub ] && echo BOOK-OK; ( : > /book.epub ) 2>/dev/null && echo WROTE-BOOK; \
         ( : > /x ) 2>/dev/null && echo WROTE-ROOT; echo DONE",
        secret.display(),
        dir.0.display()
    ));
    let out = cmd.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(text.trim(), "BOOK-OK\nDONE", "stderr: {}", String::from_utf8_lossy(&out.stderr));
    // The helper's own probe covers the network: it ran when `reader()` succeeded.
    reader();
}

/// F1: files that are not EPUBs say so.
#[test]
fn non_epub_files_are_refused() {
    assert_eq!(open_err(b"just some text, not a book"), ReaderError::Unsupported(Unsupported::NotEpub));
    assert_eq!(open_err(b""), ReaderError::Unsupported(Unsupported::NotEpub));
    assert_eq!(open_err(&zip(&[("hello.txt", b"a zip, but no book")])), ReaderError::Unsupported(Unsupported::NotEpub));
    let dir = Scratch::new();
    assert_eq!(reader().inspect(&dir.file("x", b"PK\x03\x04 broken")).unwrap_err(), ReaderError::Unsupported(Unsupported::NotEpub));
}

/// F2: a missing or unreadable source is reported before any sandbox starts.
#[test]
fn unreadable_sources_are_reported() {
    let dir = Scratch::new();
    let r = reader();
    assert!(matches!(r.open(&dir.0.join("absent.epub")), Err(ReaderError::SourceUnreadable { .. })));
    assert!(matches!(r.open(&dir.0), Err(ReaderError::SourceUnreadable { .. })));
    assert!(matches!(r.inspect(&dir.0.join("absent.epub")), Err(ReaderError::SourceUnreadable { .. })));
}

/// F3: a protected book is named as such; its metadata is still importable.
#[test]
fn protected_books_are_refused_with_a_clear_reason() {
    let drm = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/>
  <enc:CipherData><enc:CipherReference URI="OEBPS/c0.xhtml"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let bytes = simple(&["<p>locked</p>"], &[("META-INF/encryption.xml", drm)]);
    let dir = Scratch::new();
    let path = dir.file("drm.epub", &bytes);
    let err = reader().open(&path).unwrap_err();
    assert_eq!(err, ReaderError::Unsupported(Unsupported::Protected));
    assert!(err.to_string().contains("DRM-free"));
    let m = reader().inspect(&path).unwrap();
    assert!(m.protected);
    assert_eq!(m.title.as_deref(), Some("Simple"));

    // Obfuscated fonts alone do not lock the text.
    let fonts = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
  <enc:CipherData><enc:CipherReference URI="OEBPS/font.otf"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let bytes = simple(&["<p>open</p>"], &[("META-INF/encryption.xml", fonts)]);
    let book = reader().open(&dir.file("fonts.epub", &bytes)).unwrap();
    assert!(!book.metadata().protected);
    assert_eq!(book.read_chapter(0).unwrap().plain_text(), "open\n");
}

/// F4: damaged books fail with a reason instead of opening half-way.
#[test]
fn damaged_books_are_reported() {
    let no_opf = zip(&[("mimetype", b"application/epub+zip"), ("META-INF/container.xml", CONTAINER.as_bytes())]);
    assert!(matches!(open_err(&no_opf), ReaderError::InvalidBook { .. }));

    let no_spine = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", b"<package version='3.0'><metadata/><manifest/><spine/></package>"),
    ]);
    assert!(matches!(open_err(&no_spine), ReaderError::InvalidBook { .. }));

    let garbage_opf = zip(&[("META-INF/container.xml", CONTAINER.as_bytes()), ("OEBPS/content.opf", b"\x00\x01\x02 not xml")]);
    assert!(matches!(open_err(&garbage_opf), ReaderError::InvalidBook { .. }));

    // Cut the file short: the ZIP directory at the end is gone.
    let whole = epub3();
    assert_eq!(open_err(&whole[..whole.len() / 2]), ReaderError::Unsupported(Unsupported::NotEpub));

    // A chapter the package lists but the archive lacks: the book opens, that
    // chapter fails, the others read.
    let opf = br#"<package version="3.0"><metadata/><manifest>
        <item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
        <item id="b" href="gone.xhtml" media-type="application/xhtml+xml"/></manifest>
        <spine><itemref idref="a"/><itemref idref="b"/></spine></package>"#;
    let bytes = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf),
        ("OEBPS/a.xhtml", b"<html><body><p>Here.</p></body></html>"),
    ]);
    let dir = Scratch::new();
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    assert_eq!(book.read_chapter(0).unwrap().plain_text(), "Here.\n");
    assert!(matches!(book.read_chapter(1), Err(ReaderError::InvalidBook { .. })));
    assert_eq!(book.read_chapter(2).unwrap_err(), ReaderError::ChapterOutOfRange { index: 2, count: 2 });
}

/// F5: references that try to leave the container or reach the host are inert.
#[test]
fn hostile_references_reach_nothing() {
    // The package points its chapter at a host file by climbing out, and a
    // chapter declares an external entity. Neither yields host content.
    let opf = br#"<package version="3.0"><metadata/><manifest>
        <item id="a" href="../../../../etc/hostname" media-type="application/xhtml+xml"/>
        <item id="b" href="/etc/hostname" media-type="application/xhtml+xml"/>
        <item id="c" href="file:///etc/hostname" media-type="application/xhtml+xml"/>
        <item id="d" href="d.xhtml" media-type="application/xhtml+xml"/></manifest>
        <spine><itemref idref="a"/><itemref idref="b"/><itemref idref="c"/><itemref idref="d"/></spine></package>"#;
    let xxe = b"<?xml version=\"1.0\"?><!DOCTYPE html [<!ENTITY leak SYSTEM \"file:///book.epub\"><!ENTITY big \"aaaaaaaaaa\">]>\n<html><body><p>Before &leak; after &big;.</p></body></html>";
    let bytes = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf),
        ("OEBPS/d.xhtml", xxe),
    ]);
    let dir = Scratch::new();
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    // "a" and "c" cannot be named inside the container and are dropped; "b"
    // resolves to the container path etc/hostname, which does not exist.
    assert_eq!(book.chapters().len(), 2);
    assert!(matches!(book.read_chapter(0), Err(ReaderError::InvalidBook { .. })));
    assert_eq!(book.read_chapter(1).unwrap().plain_text(), "Before &leak; after &big;.\n");
}

/// F6: archive limits. Entry count, unpacked size of a document, reply size.
#[test]
fn oversized_books_hit_limits_not_memory() {
    let dir = Scratch::new();

    // 64 MiB of spaces deflates to a few kilobytes: a classic decompression bomb.
    let bomb = vec![b' '; 64 * 1024 * 1024];
    let bytes = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", br#"<package version="3.0"><metadata/><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>"#),
        ("OEBPS/a.xhtml", &bomb),
    ]);
    assert!(bytes.len() < 200_000);
    let book = reader().open(&dir.file("bomb.epub", &bytes)).unwrap();
    assert!(matches!(book.read_chapter(0), Err(ReaderError::LimitExceeded { .. })));

    let bomb_opf = zip(&[("META-INF/container.xml", CONTAINER.as_bytes()), ("OEBPS/content.opf", &bomb)]);
    assert!(matches!(reader().inspect(&dir.file("opf.epub", &bomb_opf)), Err(ReaderError::LimitExceeded { .. })));

    let mut cfg = config();
    cfg.limits.max_archive_entries = 5;
    let small = Reader::new(cfg).unwrap();
    assert!(matches!(small.open(&dir.file("many.epub", &epub3())), Err(ReaderError::LimitExceeded { .. })));

    let mut cfg = config();
    cfg.limits.max_file_bytes = 100;
    let tiny = Reader::new(cfg).unwrap();
    assert!(matches!(tiny.open(&dir.file("big.epub", &epub3())), Err(ReaderError::LimitExceeded { .. })));

    let mut cfg = config();
    cfg.limits.max_output_bytes = 200;
    let quiet = Reader::new(cfg).unwrap();
    assert!(matches!(quiet.open(&dir.file("out.epub", &epub3())), Err(ReaderError::LimitExceeded { .. })));

    let mut cfg = config();
    cfg.limits.max_spine_items = 2;
    let short = Reader::new(cfg).unwrap();
    assert!(matches!(short.open(&dir.file("spine.epub", &epub3())), Err(ReaderError::LimitExceeded { .. })));
}

/// F7: a helper that runs out of memory or time is stopped and reported, and
/// the next call works.
#[test]
fn memory_and_time_limits_stop_the_helper() {
    let dir = Scratch::new();
    let path = dir.file("b.epub", &epub3());

    // 24 MiB of address space is not enough to unpack a 16 MiB chapter limit's
    // worth of text; the allocation fails inside the helper.
    let big = format!("<p>{}</p>", "word ".repeat(2 * 1024 * 1024));
    let heavy = dir.file("heavy.epub", &simple(&[&big], &[]));
    let mut cfg = config();
    cfg.limits.memory_bytes = 24 * 1024 * 1024;
    let starved = Reader::new(cfg).unwrap();
    let err = starved.open(&heavy).and_then(|b| b.read_chapter(0)).unwrap_err();
    assert!(matches!(err, ReaderError::HelperFailed { .. }), "{err:?}");
    assert!(starved.inspect(&path).is_ok());

    let cfg = ReaderConfig { timeout: Duration::from_millis(40), ..config() };
    // The probe itself must fit in the allowance on this machine; if it does
    // not, creation fails closed, which is also the behaviour under test.
    if let Ok(hurried) = Reader::new(cfg) {
        let slow = format!("<p>{}</p>", "<b>x</b> ".repeat(1024 * 1024));
        let slow = dir.file("slow.epub", &simple(&[&slow], &[]));
        let result = hurried.open(&slow).and_then(|b| b.read_chapter(0));
        assert_eq!(result.unwrap_err(), ReaderError::Timeout);
    }
    assert!(reader().open(&path).is_ok());
}

/// F8: if the file changes under an open book, reading stops.
#[test]
fn a_changed_file_is_noticed() {
    let dir = Scratch::new();
    let path = dir.file("b.epub", &epub3());
    let book = reader().open(&path).unwrap();
    assert!(book.read_chapter(1).is_ok());
    std::fs::write(&path, epub2()).unwrap();
    assert_eq!(book.read_chapter(1).unwrap_err(), ReaderError::SourceChanged);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(book.read_chapter(1).unwrap_err(), ReaderError::SourceChanged);
}

/// S4: installed the way a package lays it out, with an empty environment and
/// a different working directory, the reader finds its helper and reads.
#[test]
fn installed_layout_finds_the_helper_and_reads() {
    let example = PathBuf::from(HELPER).parent().unwrap().join("examples/epub-dump");
    assert!(example.is_file(), "build the example first: {}", example.display());
    let dir = Scratch::new();
    let book = dir.file("b.epub", &epub3());
    for libexec in [false, true] {
        let root = dir.0.join(if libexec { "prefix-libexec" } else { "prefix-flat" });
        let bin = root.join("bin");
        let helper_dir = if libexec { root.join("libexec/ebook-manager") } else { bin.clone() };
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&helper_dir).unwrap();
        std::fs::copy(&example, bin.join("epub-dump")).unwrap();
        std::fs::copy(HELPER, helper_dir.join("ebook-reader-helper")).unwrap();
        let out = Command::new(bin.join("epub-dump")).arg(&book).arg("2").env_clear().current_dir("/").output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        assert!(text.contains("title: The Lighthouse at Low Tide"), "{text}");
        assert!(text.contains("# Chapter One\nThe keeper\u{2019}s boat"), "{text}");
    }
    // With no helper installed the same program fails closed with a sentence.
    let bare = dir.0.join("bare/bin");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::copy(&example, bare.join("epub-dump")).unwrap();
    let out = Command::new(bare.join("epub-dump")).arg(&book).env_clear().output().unwrap();
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("reader component is missing"));
}
