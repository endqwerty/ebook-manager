use ebook_bridge::{
    BookEdit, BookProtection, BridgeError, ExportStatus, ImportOutcomeKind, Library, ReviewChoice,
    SortField, SortRule, StyleProperty, binding_palette_index, wrap_chapter_document,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};
use std::{
    fs,
    io::{Cursor, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::OnceLock,
};
use tempfile::TempDir;
use zip::{CompressionMethod, ZipWriter, write::SimpleFileOptions};

fn ensure_helper_beside_test_exe() {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    let helper = BUILT.get_or_init(|| {
        let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let mut cmd = Command::new(cargo);
        cmd.args([
            "build",
            "--quiet",
            "--message-format=json",
            "-p",
            "ebook-reader",
            "--bin",
            "ebook-reader-helper",
        ]);
        if !cfg!(debug_assertions) {
            cmd.arg("--release");
        }
        let output = cmd.output().expect("cargo build ebook-reader-helper");
        assert!(
            output.status.success(),
            "building ebook-reader-helper failed"
        );
        let stdout = String::from_utf8_lossy(&output.stdout);
        let built = stdout
            .lines()
            .filter(|line| line.contains("\"ebook-reader-helper\""))
            .find_map(|line| {
                let marker = "\"executable\":\"";
                let rest = line.split(marker).nth(1)?;
                let path = rest.split('"').next()?;
                (path != "null").then(|| PathBuf::from(path))
            })
            .expect("helper executable path in cargo json output");
        assert!(built.is_file(), "expected helper at {}", built.display());
        let exe = std::env::current_exe().expect("current test executable");
        let exe_dir = exe.parent().expect("test executable dir");
        let beside = exe_dir.join("ebook-reader-helper");
        let tmp = exe_dir.join(format!("ebook-reader-helper.tmp.{}", std::process::id()));
        fs::copy(&built, &tmp).expect("stage helper beside test binary");
        fs::rename(&tmp, &beside).expect("install helper beside test binary");
        beside
    });
    assert!(helper.is_file());
}

fn coredump_count() -> Option<usize> {
    let out = Command::new("coredumpctl")
        .args(["list", "--no-pager", "-q"])
        .output()
        .ok()?;
    Some(String::from_utf8_lossy(&out.stdout).lines().count())
}

fn zip_entries(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        let method = if *name == "mimetype" {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        writer
            .start_file(
                *name,
                SimpleFileOptions::default().compression_method(method),
            )
            .unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

fn cover_png_bytes(r: u8, g: u8, b: u8) -> Vec<u8> {
    let img = RgbaImage::from_fn(120, 180, |_, _| Rgba([r, g, b, 255]));
    let mut out = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(img)
        .write_to(&mut out, ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn synthetic_epub(uuid: &str, title: &str, author: &str, ch2_extra: &str) -> Vec<u8> {
    let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
    let opf = format!(
        r##"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:{uuid}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator id="a1">{author}</dc:creator>
    <meta refines="#a1" property="role" scheme="marc:relators">aut</meta>
    <meta refines="#a1" property="file-as">Field, Mira</meta>
    <meta id="s1" property="belongs-to-collection">Coastal Guides</meta>
    <meta refines="#s1" property="collection-type">series</meta>
    <meta refines="#s1" property="group-position">2</meta>
    <dc:date>2025-06-15</dc:date>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="c2.xhtml" media-type="application/xhtml+xml"/>
    <item id="cov" href="cover.png" media-type="image/png" properties="cover-image"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
  </spine>
</package>"##
    );
    let nav = r##"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>Contents</title></head>
<body>
<nav epub:type="toc"><ol>
  <li><a href="c1.xhtml">Harbor Lights</a></li>
  <li><a href="c2.xhtml#north">Northern Shore</a></li>
</ol></nav>
</body></html>"##;
    let c1 = r##"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Harbor Lights</title></head>
<body><h1>Harbor Lights</h1><p>First chapter at low tide.</p><p><img src="cover.png" alt="Beacon sketch"/></p></body></html>"##;
    let c2 = format!(
        r##"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Northern Shore</title></head>
<body><h1 id="north">Northern Shore</h1><p>Second chapter text. {ch2_extra}</p></body></html>"##
    );
    let cover = cover_png_bytes(48, 92, 140);
    zip_entries(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/nav.xhtml", nav.as_bytes()),
        ("OEBPS/c1.xhtml", c1.as_bytes()),
        ("OEBPS/c2.xhtml", c2.as_bytes()),
        ("OEBPS/cover.png", &cover),
    ])
}

fn protected_epub() -> Vec<u8> {
    let base = synthetic_epub(
        "77777777-2222-4333-8444-555555555555",
        "Locked Harbor",
        "Mira Field",
        "Protected edition.",
    );
    let mut reader = zip::ZipArchive::new(Cursor::new(base)).unwrap();
    let mut entries: Vec<(String, Vec<u8>)> = Vec::new();
    for i in 0..reader.len() {
        let mut f = reader.by_index(i).unwrap();
        let mut buf = Vec::new();
        io::Read::read_to_end(&mut f, &mut buf).unwrap();
        entries.push((f.name().to_owned(), buf));
    }
    entries.push((
        "META-INF/license.lcpl".to_owned(),
        br#"{"id":"synthetic-lcp"}"#.to_vec(),
    ));
    let refs: Vec<(&str, &[u8])> = entries
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice()))
        .collect();
    zip_entries(&refs)
}

use std::io;

fn write_file(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, bytes).unwrap();
    path
}

#[test]
fn folder_import_handles_book_duplicate_revision_and_broken_file() {
    ensure_helper_beside_test_exe();
    let dumps_before = coredump_count();
    let root = TempDir::new().unwrap();
    let lib_dir = root.path().join("library");
    let incoming = root.path().join("incoming");
    fs::create_dir_all(&incoming).unwrap();

    let original = synthetic_epub(
        "bbf9ffdf-0ec0-4962-a077-5375e799a012",
        "The Lantern Atlas",
        "Mira Field",
        "First printing.",
    );
    let revised = synthetic_epub(
        "bbf9ffdf-0ec0-4962-a077-5375e799a012",
        "The Lantern Atlas",
        "Mira Field",
        "Revised second printing with extra observations.",
    );
    write_file(&incoming, "01-book.epub", &original);
    write_file(&incoming, "02-duplicate.epub", &original);
    write_file(&incoming, "03-revised.epub", &revised);
    write_file(&incoming, "04-broken.epub", b"not a zip archive at all");

    let library = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
    let results = library.import_paths(vec![incoming.to_string_lossy().into_owned()]);
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].source_name, "01-book.epub");
    assert_eq!(results[1].source_name, "02-duplicate.epub");
    assert_eq!(results[2].source_name, "03-revised.epub");
    assert_eq!(results[3].source_name, "04-broken.epub");

    let added_id = match &results[0].outcome {
        ImportOutcomeKind::Added { book } => {
            assert_eq!(book.title, "The Lantern Atlas");
            assert!(book.has_cover);
            assert!(book.words.unwrap_or(0) > 0);
            book.id.clone()
        }
        other => panic!("expected Added, got {other:?}"),
    };
    assert!(matches!(
        &results[1].outcome,
        ImportOutcomeKind::Duplicate { book } if book.id == added_id
    ));
    let review_token = match &results[2].outcome {
        ImportOutcomeKind::NeedsReview {
            review_token,
            candidates,
            ..
        } => {
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0].id, added_id);
            review_token.clone()
        }
        other => panic!("expected NeedsReview, got {other:?}"),
    };
    assert!(matches!(
        &results[3].outcome,
        ImportOutcomeKind::Failed { message } if !message.is_empty()
    ));

    let before_settle = library.browse(String::new(), vec![]).unwrap();
    assert_eq!(before_settle.count, 1);
    assert_eq!(before_settle.rows.len(), 1);
    assert_eq!(before_settle.rows[0].id, added_id);

    let replaced = library
        .settle_review(
            review_token,
            ReviewChoice::Replace {
                book_id: added_id.clone(),
            },
        )
        .unwrap();
    assert!(!replaced.publication_pending);
    let replaced = match replaced.outcome {
        ImportOutcomeKind::Replaced { book } => book,
        other => panic!("expected Replaced, got {other:?}"),
    };
    assert_eq!(replaced.id, added_id);

    let after_replace = library.browse(String::new(), vec![]).unwrap();
    assert_eq!(after_replace.count, 1);
    let opened = library.clone().open_book(added_id).unwrap();
    let ch2 = opened.read_content(1).unwrap();
    assert!(ch2.markup.contains("Revised second printing"));

    let third = synthetic_epub(
        "bbf9ffdf-0ec0-4962-a077-5375e799a012",
        "The Lantern Atlas Companion",
        "Mira Field",
        "Separate volume.",
    );
    let third_path = write_file(root.path(), "companion.epub", &third);
    let third_res = library.import_paths(vec![third_path.to_string_lossy().into_owned()]);
    let token_two = match &third_res[0].outcome {
        ImportOutcomeKind::NeedsReview { review_token, .. } => review_token.clone(),
        other => panic!("expected NeedsReview, got {other:?}"),
    };
    let kept = library
        .settle_review(token_two, ReviewChoice::KeepSeparate)
        .unwrap();
    let kept = match kept.outcome {
        ImportOutcomeKind::Added { book } => book,
        other => panic!("expected Added, got {other:?}"),
    };
    assert_ne!(kept.id, replaced.id);
    assert_eq!(library.browse(String::new(), vec![]).unwrap().count, 2);

    if let Some(before) = dumps_before {
        assert_eq!(coredump_count(), Some(before), "unexpected core dump");
    }
}

#[test]
fn saved_reading_place_and_edits_survive_closing_and_reopening_library() {
    ensure_helper_beside_test_exe();
    let root = TempDir::new().unwrap();
    let lib_dir = root.path().join("library");
    let epub_bytes = synthetic_epub(
        "12345678-0ec0-4962-a077-5375e799a012",
        "The Tide Tables",
        "Mira Field",
        "Longer second chapter for reading place persistence.",
    );
    let epub_path = write_file(root.path(), "tides.epub", &epub_bytes);

    let book_id = {
        let library = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
        let imported = library.import_paths(vec![epub_path.to_string_lossy().into_owned()]);
        let id = match &imported[0].outcome {
            ImportOutcomeKind::Added { book } => book.id.clone(),
            other => panic!("expected Added, got {other:?}"),
        };

        let edited = library
            .edit_book(
                id.clone(),
                BookEdit {
                    title: "A Harbor Almanac".into(),
                    authors: vec!["Clara Vance".into()],
                    sort_title: Some("Harbor Almanac".into()),
                    author_sorts: vec![Some("Vance, Clara".into())],
                    series_name: Some("Sea Manuals".into()),
                    series_position: Some("1.5".into()),
                },
            )
            .unwrap();
        assert!(!edited.publication_pending);
        let edited = edited.book;
        assert_eq!(edited.title, "A Harbor Almanac");
        assert_eq!(edited.series[0].name, "Sea Manuals");
        assert_eq!(edited.series[0].position.as_deref(), Some("1.5"));

        let open = library.clone().open_book(id.clone()).unwrap();
        assert_eq!(open.chapters().len(), 2);
        assert_eq!(open.toc().len(), 2);
        assert_eq!(open.load_place().unwrap(), None);

        let ch0 = open.read_content(0).unwrap();
        assert!(ch0.markup.contains("First chapter at low tide."));
        assert!(ch0.markup.contains("/r/"));
        let wrapped = wrap_chapter_document(
            ch0.markup.clone(),
            ch0.language.clone(),
            ch0.right_to_left,
            vec![StyleProperty {
                name: "--page".into(),
                value: "#f4ecd8".into(),
            }],
        );
        assert!(wrapped.contains("xmlns=\"http://www.w3.org/1999/xhtml\""));
        assert!(wrapped.contains("--page:#f4ecd8;"));

        let pic_id: u32 = ch0
            .markup
            .split("/r/")
            .nth(1)
            .unwrap()
            .split('"')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let resource = open.read_resource(pic_id).unwrap();
        assert_eq!(resource.media_type, "image/png");
        assert!(resource.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

        open.save_place(1, 24, 1, 4).unwrap();
        id
    };

    let reopened = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
    let matched = reopened
        .browse(
            "vance manuals".into(),
            vec![SortRule {
                field: SortField::Author,
                descending: false,
            }],
        )
        .unwrap();
    assert_eq!(matched.count, 1);
    let window = reopened.browse_window(matched.view_id, 0, 10).unwrap();
    assert_eq!(window.len(), 1);
    assert_eq!(window[0].title, "A Harbor Almanac");
    assert_eq!(window[0].series_name.as_deref(), Some("Sea Manuals"));
    assert_eq!(window[0].series_position.as_deref(), Some("1.5"));
    assert!(window[0].progress.unwrap_or(0.0) > 0.0);

    let jump = reopened.quick_jump("vance".into(), 5).unwrap();
    assert_eq!(jump.books.len(), 1);
    assert_eq!(jump.books[0].id, book_id);
    assert_eq!(jump.authors.len(), 1);
    assert_eq!(jump.authors[0].name, "Clara Vance");

    let open_again = reopened.clone().open_book(book_id.clone()).unwrap();
    let place = open_again.load_place().unwrap().expect("saved place");
    assert_eq!(place.chapter, 1);
    assert_eq!(place.offset, 24);
    assert!(place.fraction.unwrap_or(0.0) > 0.0);
}

#[test]
fn cover_thumbnail_repairs_tampered_cache_and_export_detects_existing_copy() {
    ensure_helper_beside_test_exe();
    let root = TempDir::new().unwrap();
    let lib_dir = root.path().join("library");
    let device_dir = root.path().join("device");
    fs::create_dir_all(&device_dir).unwrap();

    let epub_bytes = synthetic_epub(
        "99999999-0ec0-4962-a077-5375e799a012",
        "Beacon Notes",
        "Mira Field",
        "Export check.",
    );
    let epub_path = write_file(root.path(), "beacon.epub", &epub_bytes);
    let locked_path = write_file(root.path(), "locked.epub", &protected_epub());

    let library = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
    let imported = library.import_paths(vec![
        epub_path.to_string_lossy().into_owned(),
        locked_path.to_string_lossy().into_owned(),
    ]);
    let book = match &imported[0].outcome {
        ImportOutcomeKind::Added { book } => book.clone(),
        other => panic!("expected Added, got {other:?}"),
    };
    let locked = match &imported[1].outcome {
        ImportOutcomeKind::Added { book } => book.clone(),
        other => panic!("expected locked book Added, got {other:?}"),
    };
    assert_eq!(locked.protection, Some(BookProtection::Lcp));
    match library.clone().open_book(locked.id) {
        Err(BridgeError::Locked { protection, .. }) => {
            assert_eq!(protection, BookProtection::Lcp);
        }
        other => panic!("expected BridgeError::Locked, got {}", other.is_ok()),
    }

    let png = library
        .cover_thumbnail(book.id.clone())
        .unwrap()
        .expect("cover png");
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let digest = book.cover_digest.expect("cover digest");
    let cached_file = lib_dir.join("cache/thumbs").join(format!("{digest}.png"));
    fs::write(&cached_file, b"tampered bytes").unwrap();
    let repaired = library
        .cover_thumbnail(book.id.clone())
        .unwrap()
        .expect("repaired cover png");
    assert_eq!(repaired, png);

    let first_export = library
        .export_book(book.id.clone(), device_dir.to_string_lossy().into_owned())
        .unwrap();
    let exported_path = match first_export {
        ExportStatus::Copied { destination } => PathBuf::from(destination),
        other => panic!("expected Copied, got {other:?}"),
    };
    assert_eq!(fs::read(&exported_path).unwrap(), epub_bytes);

    let second_export = library
        .export_book(book.id, device_dir.to_string_lossy().into_owned())
        .unwrap();
    assert_eq!(
        second_export,
        ExportStatus::AlreadyPresent {
            destination: exported_path.to_string_lossy().into_owned()
        }
    );

    assert_eq!(binding_palette_index("abc".into()), 7);
    assert_eq!(binding_palette_index(String::new()), 2);
}

fn block_portable_changes(lib_dir: &Path) -> PathBuf {
    let changes = lib_dir.join("portable").join("changes");
    let away = lib_dir.join("changes-away");
    fs::rename(&changes, &away).unwrap();
    fs::write(&changes, b"occupied").unwrap();
    away
}

fn unblock_portable_changes(lib_dir: &Path, away: &Path) {
    let changes = lib_dir.join("portable").join("changes");
    fs::remove_file(&changes).unwrap();
    fs::rename(away, &changes).unwrap();
}

#[test]
fn edit_saves_every_field_together_while_publication_is_pending() {
    ensure_helper_beside_test_exe();
    let root = TempDir::new().unwrap();
    let lib_dir = root.path().join("library");
    let path = write_file(
        root.path(),
        "pending.epub",
        &synthetic_epub(
            "0f1e2d3c-0ec0-4962-a077-5375e799a012",
            "Pending Shores",
            "Mira Field",
            "Chapter text.",
        ),
    );
    let library = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
    let id = match &library.import_paths(vec![path.to_string_lossy().into_owned()])[0].outcome {
        ImportOutcomeKind::Added { book } => book.id.clone(),
        other => panic!("expected Added, got {other:?}"),
    };
    let away = block_portable_changes(&lib_dir);
    let edit = BookEdit {
        title: "Shores Retitled".into(),
        authors: vec!["Clara Vance".into()],
        sort_title: Some("Retitled Shores".into()),
        author_sorts: vec![Some("Vance, Clara".into())],
        series_name: Some("Coast Notes".into()),
        series_position: Some("3".into()),
    };
    let returned = library.edit_book(id.clone(), edit.clone()).unwrap();
    assert!(returned.publication_pending);
    let saved = library.book(id.clone()).unwrap();
    assert_eq!(returned.book, saved);
    assert_eq!(saved.title, "Shores Retitled");
    assert_eq!(saved.sort_title.as_deref(), Some("Retitled Shores"));
    assert_eq!(saved.author_sorts, vec![Some("Vance, Clara".to_owned())]);
    assert_eq!(saved.series[0].name, "Coast Notes");
    assert_eq!(saved.series[0].position.as_deref(), Some("3"));
    assert!(
        library
            .edit_book(id.clone(), edit.clone())
            .unwrap()
            .publication_pending
    );
    unblock_portable_changes(&lib_dir, &away);
    let retried = library.edit_book(id.clone(), edit).unwrap();
    assert!(!retried.publication_pending);
    assert_eq!(retried.book, saved);
    assert_eq!(
        lib_dir.join("portable/changes").read_dir().unwrap().count(),
        2,
        "one import and one edit"
    );
}

#[test]
fn pending_imports_and_settlements_count_as_saved_and_spend_their_token() {
    ensure_helper_beside_test_exe();
    let root = TempDir::new().unwrap();
    let lib_dir = root.path().join("library");
    let uuid = "7a6b5c4d-0ec0-4962-a077-5375e799a012";
    let first = write_file(
        root.path(),
        "first.epub",
        &synthetic_epub(uuid, "Harbor Lights", "Mira Field", "One."),
    );
    let library = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
    let away = block_portable_changes(&lib_dir);

    let added = &library.import_paths(vec![first.to_string_lossy().into_owned()])[0];
    assert!(added.publication_pending);
    let added_id = match &added.outcome {
        ImportOutcomeKind::Added { book } => book.id.clone(),
        other => panic!("expected Added, got {other:?}"),
    };
    let again = &library.import_paths(vec![first.to_string_lossy().into_owned()])[0];
    assert!(again.publication_pending);
    assert!(matches!(&again.outcome, ImportOutcomeKind::Duplicate { book } if book.id == added_id));

    let revised = write_file(
        root.path(),
        "revised.epub",
        &synthetic_epub(uuid, "Harbor Lights", "Mira Field", "Two, revised."),
    );
    let token = match &library.import_paths(vec![revised.to_string_lossy().into_owned()])[0].outcome
    {
        ImportOutcomeKind::NeedsReview { review_token, .. } => review_token.clone(),
        other => panic!("expected NeedsReview, got {other:?}"),
    };
    let missing = ReviewChoice::Replace {
        book_id: "not-a-candidate".into(),
    };
    assert!(library.settle_review(token.clone(), missing).is_err());
    let settled = library
        .settle_review(token.clone(), ReviewChoice::KeepSeparate)
        .unwrap();
    assert!(settled.publication_pending);
    assert_eq!(settled.source_name, "revised.epub");
    assert!(matches!(&settled.outcome, ImportOutcomeKind::Added { book } if book.id != added_id));
    assert!(matches!(
        library.settle_review(token, ReviewChoice::KeepSeparate),
        Err(BridgeError::InvalidInput { .. })
    ));
    assert_eq!(library.browse(String::new(), vec![]).unwrap().count, 2);

    unblock_portable_changes(&lib_dir, &away);
    library.refresh().unwrap();
    assert_eq!(
        lib_dir.join("portable/changes").read_dir().unwrap().count(),
        2
    );
}

#[test]
fn skipped_and_discarded_reviews_release_their_staged_files() {
    ensure_helper_beside_test_exe();
    let root = TempDir::new().unwrap();
    let lib_dir = root.path().join("library");
    let uuid = "1b2c3d4e-0ec0-4962-a077-5375e799a012";
    let library = Library::open(lib_dir.to_string_lossy().into_owned()).unwrap();
    let original = write_file(
        root.path(),
        "a.epub",
        &synthetic_epub(uuid, "Quay", "Mira Field", "One."),
    );
    library.import_paths(vec![original.to_string_lossy().into_owned()]);
    let mut tokens = Vec::new();
    for n in 0..2 {
        let path = write_file(
            root.path(),
            &format!("b{n}.epub"),
            &synthetic_epub(uuid, "Quay", "Mira Field", &format!("Revision {n}.")),
        );
        match &library.import_paths(vec![path.to_string_lossy().into_owned()])[0].outcome {
            ImportOutcomeKind::NeedsReview { review_token, .. } => {
                tokens.push(review_token.clone())
            }
            other => panic!("expected NeedsReview, got {other:?}"),
        }
    }
    let skipped = library
        .settle_review(tokens[0].clone(), ReviewChoice::Skip)
        .unwrap();
    assert_eq!(skipped.outcome, ImportOutcomeKind::Skipped);
    assert_eq!(skipped.source_name, "b0.epub");
    library.discard_reviews(vec![tokens[1].clone(), "unknown".into()]);
    for token in tokens {
        assert!(matches!(
            library.settle_review(token, ReviewChoice::KeepSeparate),
            Err(BridgeError::InvalidInput { .. })
        ));
    }
    assert_eq!(library.browse(String::new(), vec![]).unwrap().count, 1);
}
