//! Refusals, limits and the sandbox boundary. Statement numbers refer to
//! specs/epub-reader-mvp.md.

mod common;

use common::*;
use ebook_reader::{Protection, Reader, ReaderConfig, ReaderError, Unsupported};
use image::ImageFormat;
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
        sandbox_path: Some("/nonexistent/bwrap".into()),
        ..config()
    };
    assert!(matches!(
        Reader::new(no_bwrap),
        Err(ReaderError::SandboxUnavailable { .. })
    ));

    // A "bwrap" that sandboxes nothing: it exits without running the helper.
    for stand_in in ["/usr/bin/false", "/usr/bin/true"] {
        let cfg = ReaderConfig {
            sandbox_path: Some(stand_in.into()),
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
    let script =
        format!("#!/bin/sh\nwhile [ \"$1\" != session ]; do shift; done\nexec '{HELPER}' \"$@\"\n");
    let fake = dir.file("fake-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        sandbox_path: Some(fake),
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
    // A request after the refusal gets no reply: the helper has stopped.
    let mut child = Command::new(HELPER)
        .arg("session")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    let _ = child.stdin.take().unwrap().write_all(b"open\nmetadata\n");
    let out = child.wait_with_output().unwrap();
    let reply = String::from_utf8_lossy(&out.stdout);
    assert!(reply.starts_with("0 {\"Err\""), "{reply}");
    assert!(reply.contains("NotIsolated"), "{reply}");
    assert_eq!(reply.lines().count(), 1, "{reply}");
}

/// S3: inside the sandbox there is no network, no home, no writable disk, and
/// no host file other than the one book. Checked from inside with a shell.
#[cfg(target_os = "linux")]
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

/// S3 on macOS: under the profile the host really uses, the helper program
/// sees the book and nothing else of the host's. A stand-in launcher keeps
/// the host's own arguments and swaps the helper's for its `visible-probe`.
#[cfg(target_os = "macos")]
#[test]
fn sandbox_sees_only_the_book() {
    let dir = Scratch::new();
    let book = dir.file("b.epub", &epub3());
    let book = book.canonicalize().unwrap();
    let secret = dir.file("secret.txt", b"private");
    let home = std::env::var("HOME").unwrap();
    let script = format!(
        "#!/bin/sh\ncase \"$*\" in *BOOK=*) ;; *) exec {SANDBOX} \"$@\";; esac\n\
         n=0; for a in \"$@\"; do [ \"$a\" = session ] && break; n=$((n+1)); done\n\
         i=0; for a in \"$@\"; do [ $i -lt $n ] && set -- \"$@\" \"$a\"; i=$((i+1)); done; shift $i\n\
         exec {SANDBOX} \"$@\" visible-probe '{book}' '{secret}' '{dir}' '{home}' /etc/passwd /private/tmp /dev/null\n",
        book = book.display(),
        secret = secret.display(),
        dir = dir.0.display(),
    );
    let fake = dir.file("probe-launcher", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        sandbox_path: Some(fake),
        ..config()
    };
    let seen = settle(
        || match Reader::new(cfg.clone()).and_then(|r| r.open(&book)) {
            Err(ReaderError::HelperFailed { detail }) => detail,
            other => panic!("expected the probe's report, got {other:?}"),
        },
    );
    assert_eq!(
        seen,
        format!("exit status: 5; reads {}", book.display()),
        "the probe names each path it could see, read or write"
    );
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
    assert_eq!(m.protection, Some(Protection::Unrecognised));
    assert_eq!(m.title.as_deref(), Some("Simple"));

    // Obfuscated fonts alone do not lock the text.
    let fonts = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#">
  <enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
  <enc:CipherData><enc:CipherReference URI="OEBPS/font.otf"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let bytes = simple(&["<p>open</p>"], &[("META-INF/encryption.xml", fonts)]);
    let book = reader().open(&dir.file("fonts.epub", &bytes)).unwrap();
    assert_eq!(book.metadata().protection, None);
    assert_eq!(book.read_chapter(0).unwrap().plain_text(), "open\n");
}

/// Sets the ZIP format's encryption flag, bit 0 of the general-purpose flags,
/// in the local and central headers of each named entry.
fn mark_encrypted(bytes: &mut [u8], names: &[&str]) {
    for name in names.iter().map(|n| n.as_bytes()) {
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
        assert_eq!(marked, 2, "{}", String::from_utf8_lossy(name));
    }
}

const LOCKED_CHAPTER: &[u8] = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/><enc:CipherData><enc:CipherReference URI="OEBPS/c0.xhtml"/></enc:CipherData></enc:EncryptedData></encryption>"#;

/// M4: the scheme is named from what the container declares, the first match
/// in the order Readium LCP, Adobe, Apple, a password, then unrecognised.
#[test]
fn the_protection_scheme_is_named_from_the_container() {
    let adobe: &[u8] = br#"<?xml version="1.0"?><adept:rights xmlns:adept="http://ns.adobe.com/adept"><adept:licenseToken/></adept:rights>"#;
    let other: &[u8] =
        br#"<?xml version="1.0"?><rights xmlns="urn:example:rights"><grant/></rights>"#;
    let lcp: &[u8] = br#"{"id":"x"}"#;
    let sinf: &[u8] = br#"<?xml version="1.0"?><fairplay:sinfs xmlns:fairplay="http://itunes.apple.com/ns/epub"/>"#;
    let enc = ("META-INF/encryption.xml", LOCKED_CHAPTER);
    let cases: Vec<(&str, Vec<(&str, &[u8])>, Option<Protection>)> = vec![
        (
            "lcp",
            vec![enc, ("META-INF/license.lcpl", lcp)],
            Some(Protection::ReadiumLcp),
        ),
        (
            "adobe",
            vec![enc, ("META-INF/rights.xml", adobe)],
            Some(Protection::Adobe),
        ),
        (
            "apple",
            vec![enc, ("META-INF/sinf.xml", sinf)],
            Some(Protection::Apple),
        ),
        (
            "other rights",
            vec![enc, ("META-INF/rights.xml", other)],
            Some(Protection::Unrecognised),
        ),
        ("encryption only", vec![enc], Some(Protection::Unrecognised)),
        (
            "rights alone",
            vec![("META-INF/rights.xml", adobe)],
            Some(Protection::Adobe),
        ),
        (
            "lcp licence alone",
            vec![("META-INF/license.lcpl", lcp)],
            Some(Protection::ReadiumLcp),
        ),
        (
            "apple marker alone",
            vec![("META-INF/sinf.xml", sinf)],
            Some(Protection::Apple),
        ),
        (
            "lcp before adobe",
            vec![
                enc,
                ("META-INF/rights.xml", adobe),
                ("META-INF/license.lcpl", lcp),
            ],
            Some(Protection::ReadiumLcp),
        ),
        (
            "adobe before apple",
            vec![
                enc,
                ("META-INF/sinf.xml", sinf),
                ("META-INF/rights.xml", adobe),
            ],
            Some(Protection::Adobe),
        ),
    ];
    let dir = Scratch::new();
    for (name, extra, expected) in cases {
        let path = dir.file(&format!("{name}.epub"), &simple(&["<p>x</p>"], &extra));
        let m = reader().inspect(&path).unwrap();
        assert_eq!(m.protection, expected, "{name}");
        assert_eq!(m.title.as_deref(), Some("Simple"), "{name}");
        assert_eq!(
            reader().open(&path).unwrap_err(),
            ReaderError::Unsupported(Unsupported::Protected),
            "{name}"
        );
    }
    // A password on one entry, with a package that can be read.
    let mut bytes = simple(
        &["<p>one</p>", "<p>two</p>"],
        &[("META-INF/sinf.xml", sinf)],
    );
    mark_encrypted(&mut bytes, &["OEBPS/c1.xhtml"]);
    let m = reader().inspect(&dir.file("both.epub", &bytes)).unwrap();
    assert_eq!(m.protection, Some(Protection::Apple));
    let mut bytes = simple(&["<p>one</p>", "<p>two</p>"], &[]);
    mark_encrypted(&mut bytes, &["OEBPS/c1.xhtml"]);
    let m = reader()
        .inspect(&dir.file("password.epub", &bytes))
        .unwrap();
    assert_eq!(m.protection, Some(Protection::Password));
}

/// M4: a marker file that cannot be read is a lock, not a broken book.
#[test]
fn an_unreadable_marker_file_still_names_a_lock() {
    let dir = Scratch::new();
    let adobe: &[u8] = br#"<adept:rights xmlns:adept="http://ns.adobe.com/adept"/>"#;
    for marker in ["META-INF/rights.xml", "META-INF/encryption.xml"] {
        let body = if marker.ends_with("rights.xml") {
            adobe
        } else {
            LOCKED_CHAPTER
        };
        let mut bytes = simple(&["<p>x</p>"], &[(marker, body)]);
        mark_encrypted(&mut bytes, &[marker]);
        let m = reader()
            .inspect(&dir.file("sealed-marker.epub", &bytes))
            .unwrap();
        assert_eq!(m.protection, Some(Protection::Password), "{marker}");
        assert_eq!(m.title.as_deref(), Some("Simple"), "{marker}");
    }
    // Cut short: what parsed lists nothing, but the file is not whole.
    for truncated in [
        &b"<encryption><"[..],
        &LOCKED_CHAPTER[..LOCKED_CHAPTER.len() / 2],
        &b""[..],
        &b"not xml at all <<<>>>"[..],
        &b"<other/>"[..],
    ] {
        let bytes = simple(&["<p>x</p>"], &[("META-INF/encryption.xml", truncated)]);
        let m = reader().inspect(&dir.file("cut.epub", &bytes)).unwrap();
        assert_eq!(m.protection, Some(Protection::Unrecognised));
        assert_eq!(m.title.as_deref(), Some("Simple"));
    }
    // Over the element limit of a package document.
    let huge = format!("<rights>{}</rights>", "<a/>".repeat(400_001));
    let bytes = simple(&["<p>x</p>"], &[("META-INF/rights.xml", huge.as_bytes())]);
    let m = reader()
        .inspect(&dir.file("huge-rights.epub", &bytes))
        .unwrap();
    assert_eq!(m.protection, Some(Protection::Unrecognised));
    assert_eq!(m.title.as_deref(), Some("Simple"));
}

/// M4: the package is read before the marker files, so a reader whose
/// unpacking allowance fits the package still gets the book's metadata when
/// a marker does not fit.
#[test]
fn the_package_is_read_before_the_marker_files() {
    let big = format!(
        "<adept:rights xmlns:adept=\"http://ns.adobe.com/adept\">{}</adept:rights>",
        "<a/>".repeat(2000)
    );
    let bytes = simple(&["<p>x</p>"], &[("META-INF/rights.xml", big.as_bytes())]);
    let dir = Scratch::new();
    let path = dir.file("rights.epub", &bytes);
    let mut cfg = config();
    cfg.limits.max_unpacked_bytes = 2048;
    let m = Reader::new(cfg).unwrap().inspect(&path).unwrap();
    assert_eq!(m.title.as_deref(), Some("Simple"));
    assert!(m.protection.is_some());
}

/// M4: an archive whose container file and package are encrypted is still
/// inspected, as a book locked by a password with nothing else known.
#[test]
fn a_wholly_encrypted_archive_is_inspected_as_locked() {
    let mut bytes = simple(&["<p>one</p>"], &[]);
    mark_encrypted(
        &mut bytes,
        &[
            "META-INF/container.xml",
            "OEBPS/content.opf",
            "OEBPS/c0.xhtml",
        ],
    );
    let dir = Scratch::new();
    let path = dir.file("sealed.epub", &bytes);
    let m = reader().inspect(&path).unwrap();
    assert_eq!(m.protection, Some(Protection::Password));
    assert_eq!(m.title, None);
    assert!(m.authors.is_empty());
    assert_eq!(
        reader().open(&path).unwrap_err(),
        ReaderError::Unsupported(Unsupported::Protected)
    );
    let s = reader().survey(&path).unwrap();
    assert_eq!((s.words, s.cover.is_some()), (None, false));
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
    // The helper exits instead of aborting, so the desktop reports no crash
    // for a book that is merely too big.
    match &err {
        ReaderError::HelperFailed { detail } => {
            assert!(detail.starts_with("exit status: 4;"), "{detail}");
            assert!(detail.contains("memory allocation"), "{detail}");
        }
        other => panic!("{other:?}"),
    }
    assert!(starved.inspect(&path).is_ok());

    // A stand-in sandbox that says it is ready and then never replies.
    let stuck = stand_in(&dir, "stuck-bwrap", &[("*", format!("exec {SLEEP} 30"))]);
    let cfg = ReaderConfig {
        sandbox_path: Some(stuck),
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
    assert_eq!(m.protection, Some(Protection::Password));
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
    // A descriptor number no other test thread will be handed while this
    // one runs, inheritable on purpose. The tests open far fewer files.
    #[cfg(target_os = "linux")]
    const LEAKY: i32 = 900;
    #[cfg(target_os = "macos")]
    const LEAKY: i32 = 200;
    // SAFETY: dup2 onto an unused number; the copy is closed below.
    assert_eq!(unsafe { libc::dup2(secret.as_raw_fd(), LEAKY) }, LEAKY);

    // A stand-in for bwrap that reports whether it was handed that descriptor.
    let script = format!(
        "#!/bin/sh\nif [ -e /dev/fd/{LEAKY} ]; then echo 'bwrap: LEAKED' >&2; else echo 'bwrap: CLEAN' >&2; fi\nexit 1\n"
    );
    let fake = dir.file("fd-bwrap", script.as_bytes());
    Command::new("/bin/chmod")
        .arg("+x")
        .arg(&fake)
        .status()
        .unwrap();
    let cfg = ReaderConfig {
        sandbox_path: Some(fake),
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
        let reply = dir.file(
            &format!("{name}.frame"),
            format!("0 {open_reply}\n").as_bytes(),
        );
        let path = stand_in(&dir, name, &[("*", send(&reply))]);
        ReaderConfig {
            sandbox_path: Some(path),
            ..config()
        }
    };
    let meta = r#"{"title":null,"authors":[],"identifiers":[],"languages":[],"epub_version":null,"protection":null,"series":[],"published":null,"right_to_left":false}"#;

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

/// F6: every call is charged for the container and package, and for the
/// contents when it uses them, as if its helper had just read them, whether
/// it is the helper's first call or a later one.
#[test]
fn every_call_counts_the_package_and_contents_it_uses() {
    let dir = Scratch::new();
    let bytes = epub3();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes.clone())).unwrap();
    let mut size = |name: &str| zip.by_name(name).unwrap().size();
    let package = size("META-INF/container.xml") + size("OEBPS/content.opf");
    let contents = size("OEBPS/nav.xhtml");
    let chapter = size("OEBPS/text/chapter1.xhtml");
    let path = dir.file("b.epub", &bytes);
    let reader = |allowance: u64| {
        let mut cfg = config();
        cfg.limits.max_unpacked_bytes = allowance;
        Reader::new(cfg).unwrap()
    };
    let total = |r: Result<(), ReaderError>| match r {
        Err(ReaderError::LimitExceeded { what }) => assert!(what.contains("total"), "{what}"),
        other => panic!("expected a total-size refusal, got {other:?}"),
    };
    // Opening reads the package and the contents.
    assert!(reader(package + contents).open(&path).is_ok());
    total(reader(package + contents - 1).open(&path).map(|_| ()));
    // A later chapter is charged for both again, on the helper that read them.
    let book = reader(package + contents + chapter).open(&path).unwrap();
    assert!(book.read_chapter(1).is_ok());
    assert!(book.read_chapter(1).is_ok());
    let book = reader(package + contents + chapter - 1)
        .open(&path)
        .unwrap();
    total(book.read_chapter(1).map(|_| ()));
}

/// F7: an application that already runs under a lower memory ceiling than the
/// reader's own still gets a working reader, held to that lower ceiling.
#[cfg(target_os = "linux")]
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
        sandbox_path: Some(wrapped),
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

/// F7: no way the helper can fail is a crash. Every signal that would dump
/// core, and a stack overflow, ends it with exit status 4 instead.
#[test]
fn fatal_signals_end_the_helper_without_a_crash() {
    let fatal = [
        libc::SIGABRT,
        libc::SIGSEGV,
        libc::SIGBUS,
        libc::SIGILL,
        libc::SIGFPE,
        libc::SIGTRAP,
        libc::SIGSYS,
        libc::SIGQUIT,
        libc::SIGXCPU,
        libc::SIGXFSZ,
    ];
    let cases = fatal
        .iter()
        .map(|s| s.to_string())
        .chain(["stack".to_owned()]);
    for case in cases {
        // Also with every signal blocked, as a parent could leave them.
        for blocked in [false, true] {
            let mut cmd = Command::new(HELPER);
            cmd.args(["fatal-probe", &case]);
            if blocked {
                // SAFETY: only async-signal-safe calls between fork and exec.
                unsafe {
                    std::os::unix::process::CommandExt::pre_exec(&mut cmd, || {
                        let mut all: libc::sigset_t = std::mem::zeroed();
                        libc::sigfillset(&mut all);
                        libc::sigprocmask(libc::SIG_BLOCK, &all, std::ptr::null_mut());
                        Ok(())
                    });
                }
            }
            let out = cmd.output().unwrap();
            let stderr = String::from_utf8_lossy(&out.stderr);
            assert_eq!(
                out.status.code(),
                Some(4),
                "{case} blocked={blocked}: {:?} {stderr}",
                out.status
            );
            assert!(stderr.contains("helper: stopped"), "{case}: {stderr}");
        }
    }
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
#[cfg(target_os = "linux")]
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
        sandbox_path: Some(wrapped),
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

fn survey_with(cfg: ReaderConfig, bytes: &[u8]) -> ebook_reader::Survey {
    let dir = Scratch::new();
    Reader::new(cfg)
        .unwrap()
        .survey(&dir.file("b.epub", bytes))
        .unwrap()
}

/// V4: a cover that cannot be used gives no cover, never an error, and the
/// word count is still returned.
#[test]
fn unusable_covers_leave_the_word_count() {
    let png = striped(100, 150, ImageFormat::Png);
    let cases: Vec<(&str, Vec<u8>, &str)> = vec![
        // 50 by 75 would have to grow more than three times.
        ("tiny", striped(50, 75, ImageFormat::Png), "image/png"),
        ("corrupt", {
            let mut b = png.clone();
            b.truncate(40);
            b.extend_from_slice(&[0xAB; 64]);
            b
        }, "image/png"),
        // The first half of a JPEG: its decoder would fill in the rest.
        ("cut short", {
            let mut b = striped(240, 360, ImageFormat::Jpeg);
            b.truncate(b.len() / 2);
            b
        }, "image/jpeg"),
        ("svg", br#"<svg xmlns="http://www.w3.org/2000/svg" width="600" height="900"><rect width="600" height="900"/></svg>"#.to_vec(), "image/svg+xml"),
        // The cover is declared but its file is not in the book.
        ("missing", Vec::new(), "image/png"),
    ];
    for (what, image, media) in cases {
        let bytes = if what == "missing" {
            package(Parts {
                manifest: r#"<item id="cov" href="gone.png" media-type="image/png" properties="cover-image"/>"#,
                chapters: &["<p>Three short words.</p>"],
                ..Parts::default()
            })
        } else {
            with_cover(&image, "cover.img", media, "")
        };
        let s = survey_with(config(), &bytes);
        assert_eq!(s.cover, None, "{what}");
        assert_eq!(s.words, Some(3), "{what}");
    }
    // A whole JPEG is used whatever follows its end, even bytes that look
    // like the start of another scan.
    let mut trailed = striped(240, 360, ImageFormat::Jpeg);
    trailed.extend_from_slice(b"ordinary trailing bytes\xFF\xDA");
    assert!(
        survey_with(config(), &with_cover(&trailed, "c.jpg", "image/jpeg", ""))
            .cover
            .is_some()
    );
    // The same picture, intact, is used.
    assert!(
        survey_with(config(), &with_cover(&png, "c.png", "image/png", ""))
            .cover
            .is_some()
    );
}

/// V4: a picture at the pixel limit and one pixel tall is scaled within the
/// helper's memory, and the count comes back with it.
#[test]
fn a_very_wide_cover_at_the_pixel_limit_is_still_surveyed() {
    let wide = image::GrayImage::from_pixel(40_000_000, 1, image::Luma([255]));
    let mut png = Vec::new();
    image::DynamicImage::ImageLuma8(wide)
        .write_to(&mut std::io::Cursor::new(&mut png), ImageFormat::Png)
        .unwrap();
    let s = survey_with(config(), &with_cover(&png, "wide.png", "image/png", ""));
    assert_eq!(s.words, Some(3));
    let cover = s.cover.expect("a cover");
    assert_eq!(
        (cover.thumbnail.width(), cover.thumbnail.height()),
        (256, 1)
    );
}

/// V4, F6: a cover file over the size limit, or declaring more pixels than
/// the limit, is not decoded.
#[test]
fn oversized_covers_are_not_decoded() {
    let noisy = picture(120, 180, ImageFormat::Png, |x, y| {
        let v = (x.wrapping_mul(2_654_435_761) ^ y.wrapping_mul(40_503)) as u8;
        [v, v.rotate_left(3), v.rotate_left(5), 255]
    });
    let book = with_cover(&noisy, "c.png", "image/png", "");
    assert!(noisy.len() > 4096, "{}", noisy.len());
    assert!(survey_with(config(), &book).cover.is_some());

    let mut small_file = config();
    small_file.limits.max_cover_bytes = 4096;
    let s = survey_with(small_file, &book);
    assert_eq!((s.cover, s.words), (None, Some(3)));

    let mut few_pixels = config();
    few_pixels.limits.max_cover_pixels = 120 * 180 - 1;
    let s = survey_with(few_pixels, &book);
    assert_eq!((s.cover, s.words), (None, Some(3)));

    // A header that claims 60,000 by 60,000 pixels, with no data behind it.
    let mut huge = b"GIF89a".to_vec();
    huge.extend_from_slice(&60_000u16.to_le_bytes());
    huge.extend_from_slice(&60_000u16.to_le_bytes());
    huge.extend_from_slice(&[0x00, 0, 0]);
    huge.extend_from_slice(&[0x2C, 0, 0, 0, 0]);
    huge.extend_from_slice(&60_000u16.to_le_bytes());
    huge.extend_from_slice(&60_000u16.to_le_bytes());
    huge.extend_from_slice(&[0x00, 0x08, 0x02, 0x4C, 0x01, 0x00, 0x3B]);
    let s = survey_with(config(), &with_cover(&huge, "c.gif", "image/gif", ""));
    assert_eq!((s.cover, s.words), (None, Some(3)));
}

/// V2: the count is unknown when a linear chapter is missing, not text this
/// build reads, or cut at the block limit. The cover is still returned.
#[test]
fn an_incomplete_reading_order_has_no_word_count() {
    let image = striped(300, 450, ImageFormat::Png);
    let cover = r#"<item id="cov" href="c.png" media-type="image/png" properties="cover-image"/>"#;
    let unknown = |manifest: &str, spine_extra: &str, cfg: ReaderConfig| {
        let manifest = format!("{cover}{manifest}");
        let bytes = package(Parts {
            manifest: &manifest,
            spine_extra,
            chapters: &["<p>One.</p><p>Two.</p>"],
            files: &[("OEBPS/c.png", &image)],
            ..Parts::default()
        });
        let s = survey_with(cfg, &bytes);
        assert!(s.cover.is_some());
        s.words
    };
    assert_eq!(unknown("", "", config()), Some(2));
    let absent = r#"<item id="gone" href="gone.xhtml" media-type="application/xhtml+xml"/>"#;
    assert_eq!(
        unknown(absent, r#"<itemref idref="gone"/>"#, config()),
        None
    );
    // A non-linear item that is missing does not matter.
    assert_eq!(
        unknown(absent, r#"<itemref idref="gone" linear="no"/>"#, config()),
        Some(2)
    );
    assert_eq!(
        unknown("", r#"<itemref idref="undeclared"/>"#, config()),
        None
    );
    let audio = r#"<item id="snd" href="s.mp3" media-type="audio/mpeg"/>"#;
    assert_eq!(unknown(audio, r#"<itemref idref="snd"/>"#, config()), None);
    let mut short = config();
    short.limits.max_blocks_per_chapter = 1;
    assert_eq!(unknown("", "", short), None);
}

/// V1: a protected book's survey has no word count, and its cover only when
/// the cover's file is not encrypted.
#[test]
fn a_protected_book_surveys_its_cover_only_when_it_is_not_encrypted() {
    let s = survey_with(
        config(),
        &simple(
            &["<p>Locked.</p>"],
            &[("META-INF/encryption.xml", LOCKED_CHAPTER)],
        ),
    );
    assert_eq!((s.words, s.cover), (None, None));

    let png = striped(100, 150, ImageFormat::Png);
    let open_cover = with_cover_and(&png, &[("META-INF/encryption.xml", LOCKED_CHAPTER)]);
    let s = survey_with(config(), &open_cover);
    assert_eq!(s.words, None);
    assert!(s.cover.is_some());

    // Listed in the encryption file, under a percent-encoded name.
    let listed = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/><enc:CipherData><enc:CipherReference URI="OEBPS/images/c%6Fver.png"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let s = survey_with(
        config(),
        &with_cover_and(&png, &[("META-INF/encryption.xml", listed)]),
    );
    assert_eq!((s.words, s.cover.is_some()), (None, false));

    // Listed under another case than the package uses: the archive finds
    // the entry whichever case asks for it.
    let upper = br#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container" xmlns:enc="http://www.w3.org/2001/04/xmlenc#"><enc:EncryptedData><enc:EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/><enc:CipherData><enc:CipherReference URI="OEBPS/IMAGES/COVER.PNG"/></enc:CipherData></enc:EncryptedData></encryption>"#;
    let s = survey_with(
        config(),
        &with_cover_and(&png, &[("META-INF/encryption.xml", upper)]),
    );
    assert_eq!((s.words, s.cover.is_some()), (None, false));

    // An encryption list that cannot be read could list the cover.
    let s = survey_with(
        config(),
        &with_cover_and(&png, &[("META-INF/encryption.xml", b"<encryption><")]),
    );
    assert_eq!((s.words, s.cover.is_some()), (None, false));

    // In an encrypted ZIP entry.
    let mut sealed = with_cover_and(&png, &[]);
    mark_encrypted(&mut sealed, &["OEBPS/images/cover.png"]);
    let s = survey_with(config(), &sealed);
    assert_eq!((s.words, s.cover.is_some()), (None, false));
}

/// A book with one chapter and `cover.png` as its declared cover, and more
/// files.
fn with_cover_and(image: &[u8], extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut files: Vec<(&str, &[u8])> = vec![("OEBPS/images/cover.png", image)];
    files.extend_from_slice(extra);
    package(Parts {
        manifest: r#"<item id="cov" href="images/cover.png" media-type="image/png" properties="cover-image"/>"#,
        chapters: &["<p>Three short words.</p>"],
        files: &files,
        ..Parts::default()
    })
}

/// V5: the host refuses a cover reply whose pixels do not fill its size, or
/// whose size is zero or over the thumbnail's bounds.
#[test]
fn malformed_cover_replies_are_refused() {
    let dir = Scratch::new();
    let book = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let fake = |name: &str, width: u32, height: u32, len: usize| {
        let reply = format!(
            r#"{{"Ok":{{"Survey":{{"words":9,"cover":{{"width":{width},"height":{height},"average":{{"r":1,"g":2,"b":3}},"spine":{{"r":4,"g":5,"b":6}}}}}}}}}}"#
        );
        let line = format!("{len} {reply}\n");
        let mut bytes = line.into_bytes();
        bytes.extend(std::iter::repeat_n(7u8, len));
        let reply_file = dir.file(&format!("{name}.frame"), &bytes);
        let path = stand_in(&dir, name, &[("*", send(&reply_file))]);
        let cfg = ReaderConfig {
            sandbox_path: Some(path),
            ..config()
        };
        let mut outcome = None;
        settle(|| match Reader::new(cfg.clone()) {
            Ok(r) => {
                outcome = Some(r.survey(&book));
                String::new()
            }
            Err(e) => format!("{e:?}"),
        });
        outcome.expect("a reader over the stand-in")
    };

    let good = fake("good", 2, 3, 18).unwrap();
    assert_eq!(good.words, Some(9));
    let cover = good.cover.unwrap();
    assert_eq!((cover.thumbnail.width(), cover.thumbnail.height()), (2, 3));
    assert_eq!(cover.spine, ebook_reader::Rgb { r: 4, g: 5, b: 6 });

    for (name, w, h, len) in [
        ("short", 2, 3, 17),
        ("long", 2, 3, 19),
        ("zero-wide", 0, 3, 0),
        ("zero-high", 2, 0, 0),
        ("too-wide", 257, 1, 257 * 3),
        ("too-high", 1, 385, 385 * 3),
        ("overflowing", u32::MAX, u32::MAX, 3),
    ] {
        match fake(name, w, h, len) {
            Err(ReaderError::HelperFailed { .. }) => {}
            other => panic!("{name}: expected a refusal, got {other:?}"),
        }
    }
}
