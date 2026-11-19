//! Refusals, limits and the sandbox boundary. Statement numbers refer to
//! specs/epub-reader-mvp.md.

mod common;

use common::*;
use ebook_reader::{Reader, ReaderConfig, ReaderError, Unsupported};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

/// A program written a moment ago can be "text file busy" while another test
/// thread is between fork and exec with the write handle inherited. Run again
/// until that passes; it is a property of the test process, not the reader.
fn settle(mut run: impl FnMut() -> String) -> String {
    let mut last = String::new();
    for _ in 0..50 {
        last = run();
        if !last.to_lowercase().contains("busy") {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    last
}

fn open_err(bytes: &[u8]) -> ReaderError {
    let dir = Scratch::new();
    reader().open(&dir.file("b.epub", bytes)).unwrap_err()
}

/// S1: no helper, no bubblewrap or a sandbox that does not start: no reader.
#[test]
fn reader_fails_closed_without_a_working_sandbox() {
    let missing = ReaderConfig {
        helper_path: Some("/nonexistent/ebook-reader-helper".into()),
        ..ReaderConfig::default()
    };
    assert!(matches!(
        Reader::new(missing),
        Err(ReaderError::HelperNotFound { .. })
    ));

    let no_bwrap = ReaderConfig {
        bwrap_path: Some("/nonexistent/bwrap".into()),
        ..config()
    };
    assert!(matches!(
        Reader::new(no_bwrap),
        Err(ReaderError::SandboxUnavailable { .. })
    ));

    // A "bwrap" that sandboxes nothing: it exits without running the helper.
    for stand_in in ["/usr/bin/false", "/usr/bin/true"] {
        let cfg = ReaderConfig {
            bwrap_path: Some(stand_in.into()),
            ..config()
        };
        assert!(
            matches!(
                Reader::new(cfg),
                Err(ReaderError::SandboxUnavailable { .. })
            ),
            "{stand_in}"
        );
    }

    // A "bwrap" that runs the helper with no sandbox at all: the helper's own
    // check refuses, so the reader is still not created.
    let dir = Scratch::new();
    let script = format!("#!/bin/sh\nwhile [ \"$1\" != \"--\" ]; do shift; done\nshift; shift\nexec '{HELPER}' \"$@\"\n");
    let fake = dir.file("fake-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        bwrap_path: Some(fake),
        ..config()
    };
    let refusal = settle(|| match Reader::new(cfg.clone()) {
        Err(ReaderError::SandboxUnavailable { reason }) => reason,
        other => panic!("expected a refusal, got {other:?}"),
    });
    assert!(refusal.contains("visible"), "{refusal}");
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
    cmd.args([
        "--unshare-all",
        "--unshare-user",
        "--disable-userns",
        "--die-with-parent",
        "--new-session",
        "--clearenv",
        "--cap-drop",
        "ALL",
    ]);
    for d in ["/usr/lib", "/usr/lib64", "/lib", "/lib64"] {
        cmd.args(["--ro-bind-try", d, d]);
    }
    // Tools for the checks below; the real sandbox has neither.
    cmd.args(["--ro-bind-try", "/usr/bin/unshare", "/unshare"]);
    cmd.args(["--ro-bind", "/usr/bin/true", "/true"]);
    cmd.args(["--ro-bind", "/bin/sh", "/helper"])
        .arg("--ro-bind")
        .arg(&book)
        .arg("/book.epub");
    cmd.args(["--remount-ro", "/", "--chdir", "/", "--", "/helper", "-c"]);
    cmd.arg(format!(
        "for p in /home /etc /proc /tmp /dev /run /usr/bin '{}' '{}'; do [ -e \"$p\" ] && echo \"SEES $p\"; done; \
         [ -r /book.epub ] && echo BOOK-OK; ( : > /book.epub ) 2>/dev/null && echo WROTE-BOOK; \
         ( : > /x ) 2>/dev/null && echo WROTE-ROOT; \
         [ -e /unshare ] && /unshare -U /true 2>/dev/null && echo NESTED-NAMESPACE; echo DONE",
        secret.display(),
        dir.0.display()
    ));
    let out = cmd.output().unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        text.trim(),
        "BOOK-OK\nDONE",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // The helper's own probe covers the network: it ran when `reader()` succeeded.
    reader();
}

/// F1: files that are not EPUBs say so.
#[test]
fn non_epub_files_are_refused() {
    assert_eq!(
        open_err(b"just some text, not a book"),
        ReaderError::Unsupported(Unsupported::NotEpub)
    );
    assert_eq!(
        open_err(b""),
        ReaderError::Unsupported(Unsupported::NotEpub)
    );
    assert_eq!(
        open_err(&zip(&[("hello.txt", b"a zip, but no book")])),
        ReaderError::Unsupported(Unsupported::NotEpub)
    );
    let dir = Scratch::new();
    assert_eq!(
        reader()
            .inspect(&dir.file("x", b"PK\x03\x04 broken"))
            .unwrap_err(),
        ReaderError::Unsupported(Unsupported::NotEpub)
    );
}

/// F2: a missing or unreadable source is reported before any sandbox starts.
#[test]
fn unreadable_sources_are_reported() {
    let dir = Scratch::new();
    let r = reader();
    assert!(matches!(
        r.open(&dir.0.join("absent.epub")),
        Err(ReaderError::SourceUnreadable { .. })
    ));
    assert!(matches!(
        r.open(&dir.0),
        Err(ReaderError::SourceUnreadable { .. })
    ));
    assert!(matches!(
        r.inspect(&dir.0.join("absent.epub")),
        Err(ReaderError::SourceUnreadable { .. })
    ));
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
    let no_opf = zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
    ]);
    assert!(matches!(open_err(&no_opf), ReaderError::InvalidBook { .. }));

    let no_spine = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        (
            "OEBPS/content.opf",
            b"<package version='3.0'><metadata/><manifest/><spine/></package>",
        ),
    ]);
    assert!(matches!(
        open_err(&no_spine),
        ReaderError::InvalidBook { .. }
    ));

    let garbage_opf = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", b"\x00\x01\x02 not xml"),
    ]);
    assert!(matches!(
        open_err(&garbage_opf),
        ReaderError::InvalidBook { .. }
    ));

    // Cut the file short: the ZIP directory at the end is gone.
    let whole = epub3();
    assert_eq!(
        open_err(&whole[..whole.len() / 2]),
        ReaderError::Unsupported(Unsupported::NotEpub)
    );

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
    assert!(matches!(
        book.read_chapter(1),
        Err(ReaderError::InvalidBook { .. })
    ));
    assert_eq!(
        book.read_chapter(2).unwrap_err(),
        ReaderError::ChapterOutOfRange { index: 2, count: 2 }
    );
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
    assert!(matches!(
        book.read_chapter(0),
        Err(ReaderError::InvalidBook { .. })
    ));
    assert_eq!(
        book.read_chapter(1).unwrap().plain_text(),
        "Before &leak; after &big;.\n"
    );
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
    assert!(matches!(
        book.read_chapter(0),
        Err(ReaderError::LimitExceeded { .. })
    ));

    let bomb_opf = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", &bomb),
    ]);
    assert!(matches!(
        reader().inspect(&dir.file("opf.epub", &bomb_opf)),
        Err(ReaderError::LimitExceeded { .. })
    ));

    let mut cfg = config();
    cfg.limits.max_archive_entries = 5;
    let small = Reader::new(cfg).unwrap();
    assert!(matches!(
        small.open(&dir.file("many.epub", &epub3())),
        Err(ReaderError::LimitExceeded { .. })
    ));

    let mut cfg = config();
    cfg.limits.max_file_bytes = 100;
    let tiny = Reader::new(cfg).unwrap();
    assert!(matches!(
        tiny.open(&dir.file("big.epub", &epub3())),
        Err(ReaderError::LimitExceeded { .. })
    ));

    let mut cfg = config();
    cfg.limits.max_output_bytes = 200;
    let quiet = Reader::new(cfg).unwrap();
    assert!(matches!(
        quiet.open(&dir.file("out.epub", &epub3())),
        Err(ReaderError::LimitExceeded { .. })
    ));

    let mut cfg = config();
    cfg.limits.max_toc_entries = 0;
    let none = Reader::new(cfg).unwrap();
    assert!(matches!(
        none.open(&dir.file("toc0.epub", &epub3())),
        Err(ReaderError::LimitExceeded { .. })
    ));

    // Contents longer than the limit are an error, not a shorter list.
    let mut cfg = config();
    cfg.limits.max_toc_entries = 2;
    let brief = Reader::new(cfg).unwrap();
    assert!(matches!(
        brief.open(&dir.file("toc.epub", &epub3())),
        Err(ReaderError::LimitExceeded { .. })
    ));

    let mut cfg = config();
    cfg.limits.max_spine_items = 2;
    let short = Reader::new(cfg).unwrap();
    assert!(matches!(
        short.open(&dir.file("spine.epub", &epub3())),
        Err(ReaderError::LimitExceeded { .. })
    ));
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
    cfg.limits.max_compression_ratio = u32::MAX;
    let starved = Reader::new(cfg).unwrap();
    let err = starved
        .open(&heavy)
        .and_then(|b| b.read_chapter(0))
        .unwrap_err();
    assert!(matches!(err, ReaderError::HelperFailed { .. }), "{err:?}");
    assert!(starved.inspect(&path).is_ok());

    // A stand-in sandbox that answers the probe and then never replies.
    let script = "#!/bin/sh\ncase \"$*\" in\n*' -- /helper probe '*) echo '{\"Ok\":{\"Probe\":{\"isolated\":true}}}';;\n*) exec /usr/bin/sleep 30;;\nesac\n";
    let stuck = dir.file("stuck-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&stuck)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        bwrap_path: Some(stuck),
        timeout: Duration::from_millis(300),
        ..config()
    };
    let started = std::time::Instant::now();
    let mut outcome = None;
    settle(|| match Reader::new(cfg.clone()) {
        Ok(hurried) => {
            outcome = Some(hurried.open(&path).map(|_| ()));
            String::new()
        }
        Err(e) => format!("{e:?}"),
    });
    assert_eq!(outcome, Some(Err(ReaderError::Timeout)));
    assert!(started.elapsed() < Duration::from_secs(10));
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
    assert_eq!(
        book.read_chapter(1).unwrap_err(),
        ReaderError::SourceChanged
    );
    std::fs::remove_file(&path).unwrap();
    assert_eq!(
        book.read_chapter(1).unwrap_err(),
        ReaderError::SourceChanged
    );
}

/// S4: installed the way a package lays it out, with an empty environment and
/// a different working directory, the reader finds its helper and reads.
#[test]
fn installed_layout_finds_the_helper_and_reads() {
    let example = PathBuf::from(HELPER)
        .parent()
        .unwrap()
        .join("examples/epub-dump");
    assert!(
        example.is_file(),
        "build the example first: {}",
        example.display()
    );
    let dir = Scratch::new();
    let book = dir.file("b.epub", &epub3());
    for libexec in [false, true] {
        let root = dir.0.join(if libexec {
            "prefix-libexec"
        } else {
            "prefix-flat"
        });
        let bin = root.join("bin");
        let helper_dir = if libexec {
            root.join("libexec/ebook-manager")
        } else {
            bin.clone()
        };
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&helper_dir).unwrap();
        std::fs::copy(&example, bin.join("epub-dump")).unwrap();
        std::fs::copy(HELPER, helper_dir.join("ebook-reader-helper")).unwrap();
        let (ok, text) = {
            let mut ok = false;
            let text = settle(|| {
                match Command::new(bin.join("epub-dump"))
                    .arg(&book)
                    .arg("2")
                    .env_clear()
                    .current_dir("/")
                    .output()
                {
                    Ok(out) => {
                        ok = out.status.success();
                        format!(
                            "{}{}",
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        )
                    }
                    Err(e) => e.to_string(),
                }
            });
            (ok, text)
        };
        assert!(ok, "{text}");
        assert!(text.contains("title: The Lighthouse at Low Tide"), "{text}");
        assert!(
            text.contains("# Chapter One\nThe keeper\u{2019}s boat"),
            "{text}"
        );
    }
    // With no helper installed the same program fails closed with a sentence.
    let bare = dir.0.join("bare/bin");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::copy(&example, bare.join("epub-dump")).unwrap();
    let message = settle(|| {
        match Command::new(bare.join("epub-dump"))
            .arg(&book)
            .env_clear()
            .output()
        {
            Ok(out) => String::from_utf8_lossy(&out.stderr).into_owned(),
            Err(e) => e.to_string(),
        }
    });
    assert!(message.contains("reader component is missing"), "{message}");
}

/// F3: a ZIP-encrypted entry anywhere in the archive is noticed when the book
/// is opened, not when the reader reaches that chapter.
#[test]
fn zip_encrypted_entries_are_noticed_at_open() {
    // Mark one chapter's entry as encrypted the way the ZIP format does: bit 0
    // of the general-purpose flags in its local and central headers.
    let mut bytes = simple(&["<p>one</p>", "<p>two</p>"], &[]);
    let name = b"OEBPS/c1.xhtml";
    let mut marked = 0;
    for at in 0..bytes.len() - name.len() {
        if &bytes[at..at + name.len()] != name {
            continue;
        }
        if at >= 30 && &bytes[at - 30..at - 26] == b"PK\x03\x04" {
            bytes[at - 30 + 6] |= 1;
            marked += 1;
        }
        if at >= 46 && &bytes[at - 46..at - 42] == b"PK\x01\x02" {
            bytes[at - 46 + 8] |= 1;
            marked += 1;
        }
    }
    assert_eq!(marked, 2);
    let dir = Scratch::new();
    let path = dir.file("locked.epub", &bytes);
    assert_eq!(
        reader().open(&path).unwrap_err(),
        ReaderError::Unsupported(Unsupported::Protected)
    );
    let m = reader().inspect(&path).unwrap();
    assert!(m.protected);
    assert_eq!(m.title.as_deref(), Some("Simple"));
}

/// F7: extreme settings and indices give answers, not panics.
#[test]
fn extreme_settings_do_not_panic() {
    let dir = Scratch::new();
    let path = dir.file("b.epub", &epub3());
    let mut cfg = config();
    cfg.timeout = Duration::MAX;
    cfg.limits.max_output_bytes = u64::MAX;
    cfg.limits.max_chapter_bytes = u64::MAX;
    cfg.limits.max_package_bytes = u64::MAX;
    cfg.limits.max_file_bytes = u64::MAX;
    let book = Reader::new(cfg).unwrap().open(&path).unwrap();
    assert!(book.read_chapter(1).is_ok());
    let err = book.read_chapter(u32::MAX).unwrap_err();
    assert_eq!(
        err,
        ReaderError::ChapterOutOfRange {
            index: u32::MAX,
            count: 5
        }
    );
    assert!(err.to_string().contains("4294967296"));
}

/// S3: files the application has open are not handed to the sandbox, even
/// when they are not marked close-on-exec.
#[test]
fn open_descriptors_do_not_reach_the_sandbox() {
    use std::os::fd::AsRawFd;
    let dir = Scratch::new();
    let secret = std::fs::File::open(dir.file("secret.txt", b"private")).unwrap();
    // A descriptor number unlikely to be in use, inheritable on purpose.
    const LEAKY: i32 = 57;
    // SAFETY: dup2 onto an unused number; the copy is closed below.
    assert_eq!(unsafe { libc::dup2(secret.as_raw_fd(), LEAKY) }, LEAKY);

    // A stand-in for bwrap that reports whether it was handed descriptor 57.
    let script = format!(
        "#!/bin/sh\nif [ -e /proc/$$/fd/{LEAKY} ]; then echo 'bwrap: LEAKED' >&2; else echo 'bwrap: CLEAN' >&2; fi\nexit 1\n"
    );
    let fake = dir.file("fd-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        bwrap_path: Some(fake),
        ..config()
    };
    let report = settle(|| match Reader::new(cfg.clone()) {
        Err(ReaderError::SandboxUnavailable { reason }) => reason,
        other => panic!("expected the stand-in to fail, got {other:?}"),
    });
    // SAFETY: closes the copy made above.
    unsafe { libc::close(LEAKY) };
    assert_eq!(report, "bwrap: CLEAN");
}

/// S6: a helper reply that breaks the crate's promises is refused or
/// corrected before the caller sees it.
#[test]
fn inconsistent_replies_are_not_passed_on() {
    let dir = Scratch::new();
    let book = dir.file("b.epub", &epub3());
    // A stand-in for the whole sandbox that answers with fixed replies.
    let fake = |name: &str, open_reply: &str| {
        let script = format!(
            "#!/bin/sh\ncase \"$*\" in\n*' -- /helper probe '*) echo '{{\"Ok\":{{\"Probe\":{{\"isolated\":true}}}}}}';;\n*) echo '{open_reply}';;\nesac\n"
        );
        let path = dir.file(name, script.as_bytes());
        Command::new("/bin/chmod")
            .arg("+x")
            .arg(&path)
            .status()
            .unwrap();
        ReaderConfig {
            bwrap_path: Some(path),
            ..config()
        }
    };
    let meta = r#"{"title":null,"authors":[],"identifiers":[],"languages":[],"epub_version":null,"protected":false,"series":[],"published":null,"right_to_left":false}"#;

    let empty = fake(
        "empty",
        &format!(r#"{{"Ok":{{"Book":{{"metadata":{meta},"chapters":[],"toc":[]}}}}}}"#),
    );
    let result = settle(|| {
        format!(
            "{:?}",
            Reader::new(empty.clone())
                .and_then(|r| r.open(&book))
                .map(|_| ())
        )
    });
    assert!(result.contains("HelperFailed"), "{result}");

    let odd = fake(
        "odd",
        &format!(
            r#"{{"Ok":{{"Book":{{"metadata":{meta},"chapters":[{{"index":9,"title":"a\u001bb","linear":true}}],"toc":[{{"title":"x","depth":4294967295,"target":{{"chapter":7,"anchor":null}}}}]}}}}}}"#
        ),
    );
    let mut opened = None;
    settle(
        || match Reader::new(odd.clone()).and_then(|r| r.open(&book)) {
            Ok(b) => {
                opened = Some(b);
                String::new()
            }
            Err(e) => format!("{e:?}"),
        },
    );
    let opened = opened.expect("a corrected outline");
    assert_eq!(opened.chapters()[0].index, 0);
    assert_eq!(opened.chapters()[0].title.as_deref(), Some("ab"));
    assert!(opened.toc()[0].depth <= 33);
    assert_eq!(opened.toc()[0].target, None);
}

/// F6: a megabyte or more from almost nothing is refused by its ratio, and a
/// run cannot unpack more than its total allowance.
#[test]
fn compression_ratio_and_total_unpacked_size_are_bounded() {
    let dir = Scratch::new();
    // 6 MiB of one repeated short line: under the 16 MiB chapter limit,
    // far over a 200:1 ratio.
    let body = format!("<p>{}</p>", "a ".repeat(3 * 1024 * 1024));
    let squeezed = dir.file("ratio.epub", &simple(&[&body], &[]));
    let book = reader().open(&squeezed).unwrap();
    match book.read_chapter(0) {
        Err(ReaderError::LimitExceeded { what }) => assert!(what.contains("ratio"), "{what}"),
        other => panic!("expected a ratio refusal, got {other:?}"),
    }

    // A megabyte of repeated lines, such as a log or a table, is ordinary.
    let log = "<p>2026-01-01 status: nominal</p>\n".repeat(30_000);
    let plain = dir.file("log.epub", &simple(&[&log], &[]));
    let chapter = reader().open(&plain).unwrap().read_chapter(0).unwrap();
    assert_eq!(chapter.blocks.len(), 30_000);

    let mut cfg = config();
    cfg.limits.max_unpacked_bytes = 1500;
    let lean = Reader::new(cfg).unwrap();
    match lean.open(&dir.file("total.epub", &epub3())) {
        Err(ReaderError::LimitExceeded { what }) => assert!(what.contains("total"), "{what}"),
        other => panic!("expected a total-size refusal, got {other:?}"),
    }
}

/// F7: an application that already runs under a lower memory ceiling than the
/// reader's own still gets a working reader, held to that lower ceiling.
#[test]
fn a_lower_inherited_memory_ceiling_is_kept() {
    let dir = Scratch::new();
    let book = dir.file("b.epub", &epub3());
    let script = "#!/bin/sh\nulimit -v 400000\nexec /usr/bin/bwrap \"$@\"\n";
    let wrapped = dir.file("capped-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&wrapped)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        bwrap_path: Some(wrapped),
        ..config()
    };
    let mut title = None;
    let trouble = settle(
        || match Reader::new(cfg.clone()).and_then(|r| r.open(&book)) {
            Ok(b) => {
                title = b.metadata().title.clone();
                String::new()
            }
            Err(e) => format!("{e:?}"),
        },
    );
    assert_eq!(
        title.as_deref(),
        Some("The Lighthouse at Low Tide"),
        "{trouble}"
    );
}

/// F7: the helper under test was built to stop on arithmetic overflow, in
/// whichever profile and workspace built it.
#[test]
fn helper_is_built_with_overflow_checks() {
    let out = Command::new(HELPER).arg("overflow-probe").output().unwrap();
    assert!(
        !out.status.success(),
        "the helper wrapped on overflow: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(out.stdout.is_empty());
}

/// F6: a ZIP64 archive with the longest possible comment is counted by its
/// real number of entries, not by the 65,535 placeholder in its end record.
#[test]
fn zip64_archives_are_counted_by_their_real_entries() {
    let mut bytes = simple(&["<p>one</p>"], &[]);
    let end = bytes.len() - 22;
    assert_eq!(&bytes[end..end + 4], b"PK\x05\x06");
    let total = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]) as u64;
    let dir_size = u32::from_le_bytes(bytes[end + 12..end + 16].try_into().unwrap()) as u64;
    let dir_at = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as u64;
    bytes.truncate(end);

    let record_at = bytes.len() as u64;
    bytes.extend_from_slice(b"PK\x06\x06");
    bytes.extend_from_slice(&44u64.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&45u16.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&total.to_le_bytes());
    bytes.extend_from_slice(&total.to_le_bytes());
    bytes.extend_from_slice(&dir_size.to_le_bytes());
    bytes.extend_from_slice(&dir_at.to_le_bytes());
    bytes.extend_from_slice(b"PK\x06\x07");
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&record_at.to_le_bytes());
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(b"PK\x05\x06");
    bytes.extend_from_slice(&[0, 0, 0, 0, 0xFF, 0xFF, 0xFF, 0xFF]);
    bytes.extend_from_slice(&[0xFF; 8]);
    bytes.extend_from_slice(&0xFFFFu16.to_le_bytes());
    bytes.extend_from_slice(&vec![b'c'; 0xFFFF]);

    let dir = Scratch::new();
    let book = reader().open(&dir.file("zip64.epub", &bytes)).unwrap();
    assert_eq!(book.read_chapter(0).unwrap().plain_text(), "one\n");
}

/// F7: a lower soft memory limit inherited from the application is kept too,
/// not raised to the helper's own.
#[test]
fn a_lower_inherited_soft_memory_limit_is_kept() {
    let dir = Scratch::new();
    // About 12 MiB of text needs several times that to unpack, decode and
    // split into blocks: more than the 64 MiB soft limit below, far less than
    // the helper's own 768 MiB.
    let body = "<p>word word word word</p>\n".repeat(450_000);
    let heavy = dir.file("heavy.epub", &simple(&[&body], &[]));
    let script = "#!/bin/sh\nulimit -Sv 65536\nexec /usr/bin/bwrap \"$@\"\n";
    let wrapped = dir.file("soft-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&wrapped)
        .status()
        .unwrap();
    let mut cfg = ReaderConfig {
        bwrap_path: Some(wrapped),
        ..config()
    };
    cfg.limits.max_compression_ratio = u32::MAX;
    cfg.limits.max_blocks_per_chapter = 1_000_000;
    let mut outcome = None;
    settle(|| match Reader::new(cfg.clone()) {
        Ok(r) => {
            outcome = Some(
                r.open(&heavy)
                    .and_then(|b| b.read_chapter(0))
                    .map(|c| c.blocks.len()),
            );
            String::new()
        }
        Err(e) => format!("{e:?}"),
    });
    assert!(
        matches!(outcome, Some(Err(ReaderError::HelperFailed { .. }))),
        "{outcome:?}"
    );
    // The same book reads under the helper's own limit.
    let mut cfg = config();
    cfg.limits.max_compression_ratio = u32::MAX;
    cfg.limits.max_blocks_per_chapter = 1_000_000;
    let roomy = Reader::new(cfg).unwrap().open(&heavy).unwrap();
    assert_eq!(roomy.read_chapter(0).unwrap().blocks.len(), 450_000);
}
