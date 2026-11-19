use ebook_library::*;
use std::{fs, path::Path};
use tempfile::tempdir;

fn facts() -> ParsedMetadata {
    ParsedMetadata {
        title: "A Small Atlas".into(),
        authors: vec!["Quinn".into()],
        identifier: Some("atlas:1".into()),
    }
}
fn add(l: &mut Library, path: &Path) -> Result<Book> {
    match l.import(path, path, &facts(), ImportChoice::Review)? {
        ImportOutcome::Added(b) => Ok(b),
        other => panic!("expected import, got {other:?}"),
    }
}
#[test]
fn exact_duplicate_and_review_preserve_both_revisions() -> Result<()> {
    let d = tempdir()?;
    let staged = d.path().join("stage");
    fs::write(&staged, b"original package")?;
    let mut l = Library::open(&d.path().join("app"))?;
    let b = add(&mut l, &staged)?;
    let original = l.managed_file(&b.id)?;
    let alias = d.path().join("another copy.epub");
    assert!(
        matches!(l.import(&staged,&alias,&facts(),ImportChoice::Review)?,ImportOutcome::Duplicate(found) if found==b)
    );
    l.edit(&b.id, "New title", &b.authors)?;
    let pos = ReadingPosition {
        chapter: 1,
        offset: 30,
        version_id: b.version_id.clone(),
    };
    l.save_position(&b.id, &pos)?;
    fs::write(&staged, b"corrected package")?;
    let meta = ParsedMetadata {
        identifier: None,
        ..facts()
    };
    assert!(
        matches!(l.import(&staged,&alias,&meta,ImportChoice::Review)?,ImportOutcome::NeedsReview(c) if c.len()==1 && c[0].id==b.id)
    );
    let ImportOutcome::Replaced(new) =
        l.import(&staged, &alias, &meta, ImportChoice::Replace(b.id.clone()))?
    else {
        panic!()
    };
    assert_eq!(
        (&new.id, &new.file_id, &new.edition_id),
        (&b.id, &b.file_id, &b.edition_id)
    );
    assert_ne!(new.version_id, b.version_id);
    assert_eq!(new.title, "New title");
    assert_eq!(fs::read(original)?, b"original package");
    assert_eq!(fs::read(l.managed_file(&new.id)?)?, b"corrected package");
    assert!(l.load_position(&b.id)?.is_none());
    assert!(l.save_position(&b.id, &pos).is_err());
    Ok(())
}
#[test]
fn portable_only_rebuild_restores_bytes_metadata_and_position() -> Result<()> {
    let d = tempdir()?;
    let app = d.path().join("app");
    let stage = d.path().join("stage");
    fs::write(&stage, b"opaque bytes from reader")?;
    let mut l = Library::open(&app)?;
    let b = add(&mut l, &stage)?;
    let edited = l.edit(&b.id, "../../The Map", &["Dana".into()])?;
    let pos = ReadingPosition {
        version_id: b.version_id.clone(),
        chapter: 1,
        offset: 42,
    };
    l.save_position(&b.id, &pos)?;
    drop(l);
    let mut root = app;
    for relocate in [false, true] {
        if relocate {
            let fresh = d.path().join("restored");
            fs::create_dir(&fresh)?;
            fs::rename(root.join("portable"), fresh.join("portable"))?;
            root = fresh;
        }
        let mut l = Library::open(&root)?;
        l.refresh()?;
        assert_eq!(l.search("dANA")?, vec![edited.clone()]);
        assert_eq!(l.search("map")?.len(), 1);
        assert_eq!(l.load_position(&b.id)?, Some(pos.clone()));
        assert_eq!(fs::read(l.managed_file(&b.id)?)?, fs::read(&stage)?);
    }
    Ok(())
}
#[test]
fn publication_interrupted_before_and_after_journal_install_retries_once() -> Result<()> {
    for component in ["changes", "sidecars"] {
        let d = tempdir()?;
        let app = d.path().join("app");
        let stage = d.path().join("stage");
        fs::write(&stage, b"first version")?;
        let mut l = Library::open(&app)?;
        let b = add(&mut l, &stage)?;
        let dir = app.join("portable").join(component);
        let away = d.path().join("saved-dir");
        fs::rename(&dir, &away)?;
        fs::write(&dir, b"occupied")?;
        let err = l.edit(&b.id, "Edited", &[]).unwrap_err();
        assert_eq!(
            err.downcast_ref::<PublicationPending>().unwrap().book_id,
            b.id
        );
        assert_eq!(l.search("Edited")?.len(), 1);
        drop(l);
        fs::remove_file(&dir)?;
        fs::rename(&away, &dir)?;
        let mut l = Library::open(&app)?;
        l.refresh()?;
        assert_eq!(l.search("Edited")?[0].id, b.id);
        assert_eq!(fs::read_dir(app.join("portable/changes"))?.count(), 2);
    }
    Ok(())
}
#[cfg(unix)]
#[test]
fn refuses_symlinks_and_rejects_tampered_managed_bytes() -> Result<()> {
    use std::os::unix::fs::symlink;
    let d = tempdir()?;
    let stage = d.path().join("file");
    fs::write(&stage, b"bytes")?;
    let link = d.path().join("link");
    symlink(&stage, &link)?;
    let mut l = Library::open(&d.path().join("app"))?;
    assert!(
        l.import(&link, &stage, &facts(), ImportChoice::Review)
            .is_err()
    );
    assert!(l.search("")?.is_empty());
    let b = add(&mut l, &stage)?;
    let blob = l.managed_file(&b.id)?;
    fs::remove_file(&blob)?;
    symlink(&stage, &blob)?;
    assert!(l.managed_file(&b.id).is_err());
    fs::remove_file(&blob)?;
    fs::write(&blob, b"altered")?;
    assert!(l.managed_file(&b.id).is_err());
    assert!(matches!(
        l.import(&stage, &stage, &facts(), ImportChoice::Review)?,
        ImportOutcome::Duplicate(_)
    ));
    assert_eq!(fs::read(l.managed_file(&b.id)?)?, b"bytes");
    Ok(())
}

#[test]
fn shared_identifier_does_not_hide_an_exact_version() -> Result<()> {
    let d = tempdir()?;
    let a = d.path().join("a");
    let z = d.path().join("z");
    fs::write(&a, b"A")?;
    fs::write(&z, b"Z")?;
    let mut l = Library::open(&d.path().join("app"))?;
    let b = add(&mut l, &a)?;
    let p = ReadingPosition {
        version_id: b.version_id.clone(),
        chapter: 2,
        offset: 8,
    };
    l.save_position(&b.id, &p)?;
    assert!(matches!(
        l.import(&z, &z, &facts(), ImportChoice::KeepSeparate)?,
        ImportOutcome::Added(_)
    ));
    for choice in [
        ImportChoice::Review,
        ImportChoice::KeepSeparate,
        ImportChoice::Replace(b.id.clone()),
    ] {
        assert!(
            matches!(l.import(&a,&a,&facts(),choice)?,ImportOutcome::Duplicate(found) if found==b)
        );
    }
    assert_eq!(l.search("")?.len(), 2);
    assert_eq!(l.load_position(&b.id)?, Some(p));
    Ok(())
}
#[test]
fn edited_sidecars_survive_revision_reuse_and_portable_restore() -> Result<()> {
    let d = tempdir()?;
    let app = d.path().join("app");
    let stage = d.path().join("stage");
    fs::write(&stage, b"A")?;
    let mut l = Library::open(&app)?;
    let b = add(&mut l, &stage)?;
    let snapshots = app.join("portable/sidecars");
    let snapshot = snapshots.read_dir()?.next().unwrap()?.path();
    fs::write(&snapshot, b"hand edit")?;
    l.edit(&b.id, "Another title", &b.authors)?;
    l.edit(&b.id, &b.title, &b.authors)?;
    fs::write(app.join("portable/changes/.directory"), b"file manager")?;
    fs::write(&snapshot, b"second hand edit")?;
    drop(l);
    let fresh = d.path().join("fresh");
    fs::create_dir(&fresh)?;
    fs::rename(app.join("portable"), fresh.join("portable"))?;
    let l = Library::open(&fresh)?;
    assert_eq!(l.search("")?, vec![b]);
    assert!(
        fresh
            .join("portable/sidecars")
            .read_dir()?
            .flatten()
            .any(|e| fs::read(e.path()).is_ok_and(|v| v == b"hand edit"))
    );
    Ok(())
}

#[test]
fn retry_reports_the_committed_book_while_publication_is_pending() -> Result<()> {
    let d = tempdir()?;
    let app = d.path().join("app");
    let stage = d.path().join("stage");
    fs::write(&stage, b"123")?;
    let mut l = Library::open(&app)?;
    let dir = app.join("portable/changes");
    fs::remove_dir(&dir)?;
    fs::write(&dir, b"occupied")?;
    let mut ids = Vec::new();
    for _ in 0..2 {
        let err = l
            .import(&stage, &stage, &facts(), ImportChoice::Review)
            .unwrap_err();
        ids.push(
            err.downcast_ref::<PublicationPending>()
                .expect("committed import")
                .book_id
                .clone(),
        );
    }
    assert_eq!(ids, vec![l.search("")?[0].id.clone(); 2]);
    fs::remove_file(&dir)?;
    l.refresh()?;
    Ok(())
}
