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
    let err = export_epub(&source, card.path(), "Harbour").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(fs::read(&name)?, b"book bytes");
    assert_eq!(fs::read_dir(card.path())?.count(), 1);
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
