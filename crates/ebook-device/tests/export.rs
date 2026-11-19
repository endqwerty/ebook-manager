use ebook_device::{export_epub, ExportOutcome};
use std::{fs, io, os::unix::fs::symlink};
use tempfile::tempdir;

#[test]
fn retries_keep_one_copy_and_collisions_preserve_the_original() -> io::Result<()> {
    let home = tempdir()?;
    let card = tempdir()?;
    let source = home.path().join("original");
    fs::write(&source, b"book bytes")?;
    let name = card.path().join("Harbour.epub");
    assert_eq!(
        export_epub(&source, card.path(), "Harbour")?,
        ExportOutcome::Copied(name.clone())
    );
    assert_eq!(
        export_epub(&source, card.path(), "Harbour")?,
        ExportOutcome::AlreadyPresent(name.clone())
    );
    fs::write(&source, b"revised book")?;
    let ExportOutcome::Copied(revised) = export_epub(&source, card.path(), "Harbour")? else {
        panic!("expected the revised EPUB to get its own filename");
    };
    assert_ne!(revised, name);
    assert_eq!(fs::read(&revised)?, b"revised book");
    assert_eq!(
        export_epub(&source, card.path(), "Harbour")?,
        ExportOutcome::AlreadyPresent(revised)
    );
    assert_eq!(fs::read(&name)?, b"book bytes");
    assert_eq!(fs::read_dir(card.path())?.count(), 2);
    Ok(())
}

#[test]
fn title_stays_inside_selected_folder_and_existing_links_are_preserved() -> io::Result<()> {
    let home = tempdir()?;
    let card = tempdir()?;
    let source = home.path().join("original");
    fs::write(&source, b"book bytes")?;
    for title in [
        "../escape",
        "/absolute",
        " . ",
        "CON.foo",
        &"厚".repeat(230),
    ] {
        let ExportOutcome::Copied(path) = export_epub(&source, card.path(), title)? else {
            panic!("expected new file")
        };
        assert_eq!(path.parent(), Some(card.path()));
        assert_eq!(path.extension().unwrap(), "epub");
        assert!(!path.file_name().unwrap().to_string_lossy().starts_with('.'));
        assert!(!path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("CON."));
        assert_eq!(fs::read(path)?, b"book bytes");
    }
    symlink(&source, card.path().join("Link.epub"))?;
    assert!(export_epub(&source, card.path(), "Link").is_err());
    assert!(card.path().join("Link.epub").is_symlink());
    Ok(())
}

#[test]
fn concurrent_exports_preserve_both_books_and_refuse_an_occupied_fallback() -> io::Result<()> {
    let home = tempdir()?;
    let card = tempdir()?;
    let first = home.path().join("first");
    let second = home.path().join("second");
    fs::write(&first, b"first")?;
    fs::write(&second, b"second")?;
    let copies = std::thread::scope(|s| {
        let left = s.spawn(|| export_epub(&first, card.path(), "A/B"));
        let right = s.spawn(|| export_epub(&second, card.path(), "A:B"));
        [left.join().unwrap(), right.join().unwrap()]
    });
    let primary = card.path().join("A_B.epub");
    for (result, source) in copies.into_iter().zip([first, second]) {
        let ExportOutcome::Copied(destination) = result? else {
            panic!("both publications should have been written");
        };
        assert_eq!(fs::read(&destination)?, fs::read(&source)?);
        if destination != primary {
            fs::write(&destination, b"unrelated file")?;
            assert_eq!(
                export_epub(&source, card.path(), "A:B").unwrap_err().kind(),
                io::ErrorKind::AlreadyExists
            );
            assert_eq!(fs::read(destination)?, b"unrelated file");
        }
    }
    assert_eq!(fs::read_dir(card.path())?.count(), 2);
    Ok(())
}
