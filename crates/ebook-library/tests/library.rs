use ebook_library::*;
use std::{fs, path::Path};
use tempfile::tempdir;

fn facts() -> ParsedMetadata {
    ParsedMetadata {
        title: "A Small Atlas".into(),
        authors: vec!["Quinn".into()],
        identifier: Some("atlas:1".into()),
        ..Default::default()
    }
}
/// Every book whose text matches, as full records in title order.
fn search(l: &Library, text: &str) -> Result<Vec<Book>> {
    let mut reader = l.browse_reader()?;
    let query = BrowseQuery {
        text: text.into(),
        ..Default::default()
    };
    let view = reader.open(&query, &Cancel::default())?;
    reader
        .window(view.token, 0, view.count)?
        .iter()
        .map(|row| l.book(&row.id))
        .collect()
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
        last_read: None,
        fraction: None,
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
    let metadata = ParsedMetadata {
        author_sorts: vec![Some("Quinn, A".into())],
        series: vec![("Maps".into(), Some("10".into()))],
        published: Some("1977-03".into()),
        language: Some("en".into()),
        right_to_left: true,
        ..facts()
    };
    let ImportOutcome::Added(b) = l.import(&stage, &stage, &metadata, ImportChoice::Review)? else {
        panic!("expected import")
    };
    let facts = SurveyFacts {
        version: 1,
        words: Some(123456),
        cover: Some(CoverFacts {
            digest: "b".repeat(64),
            average: 0x334455,
            spine: 0x112233,
        }),
    };
    l.set_survey(&b.id, &b.version_id, facts)?;
    l.edit_filing(
        &b.id,
        Some("Map".into()),
        Some(vec![Some("Dana, D".into())]),
        Some(vec![("Places".into(), Some("2.1".into()))]),
    )?;
    let edited = l.edit(&b.id, "../../The Map", &["Dana".into()])?;
    let pos = ReadingPosition {
        last_read: Some(500),
        fraction: Some(0.4),
        version_id: b.version_id.clone(),
        chapter: 1,
        offset: 42,
    };
    l.save_position(&b.id, &pos)?;
    fs::write(&stage, b"second")?;
    l.import(
        &stage,
        &stage,
        &ParsedMetadata {
            title: "North".into(),
            ..Default::default()
        },
        ImportChoice::KeepSeparate,
    )?;
    let views = |lib: &Library| -> Result<Vec<Vec<BrowseRow>>> {
        let mut reader = lib.browse_reader()?;
        [
            Field::Title,
            Field::Author,
            Field::Series,
            Field::Added,
            Field::Published,
            Field::LastRead,
            Field::Progress,
        ]
        .into_iter()
        .map(|field| {
            let sort = SortOrder::new(vec![SortTerm {
                field,
                descending: false,
            }])?;
            Ok(reader
                .open(
                    &BrowseQuery {
                        text: String::new(),
                        sort,
                    },
                    &Cancel::default(),
                )?
                .first)
        })
        .collect()
    };
    let before = views(&l)?;
    let row = before[0].iter().find(|r| r.id == b.id).unwrap();
    assert_eq!(row.progress, Some(0.4));
    assert_eq!(row.last_read, Some(500));
    assert_eq!(row.author_sort.as_deref(), Some("Dana, D"));
    assert_eq!(row.series, Some(("Places".into(), "2.1".into())));
    assert_eq!(row.cover_rgb, Some(0x334455));
    assert_eq!(row.spine_rgb, Some(0x112233));
    assert_eq!(row.published.as_deref(), Some("1977-03"));
    assert!(row.right_to_left);
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
        assert_eq!(views(&l)?, before);
        assert_eq!(search(&l, "dANA")?, vec![edited.clone()]);
        assert_eq!(search(&l, "map")?.len(), 1);
        assert_eq!(l.load_position(&b.id)?, Some(pos.clone()));
        assert_eq!(
            fs::read(l.managed_file(&b.id)?)?,
            b"opaque bytes from reader"
        );
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
        assert_eq!(search(&l, "Edited")?.len(), 1);
        drop(l);
        fs::remove_file(&dir)?;
        fs::rename(&away, &dir)?;
        let mut l = Library::open(&app)?;
        l.refresh()?;
        assert_eq!(search(&l, "Edited")?[0].id, b.id);
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
    assert!(search(&l, "")?.is_empty());
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
        last_read: None,
        fraction: None,
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
    assert_eq!(search(&l, "")?.len(), 2);
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
    assert_eq!(search(&l, "")?, vec![b]);
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
    let alias = d.path().join("second-source.epub");
    for source in [&stage, &alias] {
        let err = l
            .import(&stage, source, &facts(), ImportChoice::Review)
            .unwrap_err();
        ids.push(
            err.downcast_ref::<PublicationPending>()
                .expect("committed import")
                .book_id
                .clone(),
        );
    }
    assert_eq!(ids, vec![search(&l, "")?[0].id.clone(); 2]);
    fs::remove_file(&dir)?;
    l.refresh()?;
    drop(l);
    let mut l = Library::open(&app)?;
    fs::write(&stage, b"corrected bytes at second source")?;
    let unidentified = ParsedMetadata {
        identifier: None,
        ..facts()
    };
    let outcome = l.import(&stage, &alias, &unidentified, ImportChoice::Review)?;
    assert!(
        matches!(outcome, ImportOutcome::NeedsReview(books) if books.len()==1 && books[0].id==ids[0])
    );
    Ok(())
}

#[test]
fn survey_facts_survive_import() -> Result<()> {
    let d = tempdir()?;
    let stage = d.path().join("stage");
    fs::write(&stage, b"bytes")?;
    let mut lib = Library::open(&d.path().join("app"))?;
    let mut raw = serde_json::to_value(facts())?;
    raw["survey"] = serde_json::json!({"version":1,"words":14500,"cover":null});
    let ImportOutcome::Added(book) = lib.import(
        &stage,
        &stage,
        &serde_json::from_value(raw.clone())?,
        ImportChoice::Review,
    )?
    else {
        panic!("expected book")
    };
    assert_eq!(
        serde_json::to_value(book.imported)?["survey"],
        raw["survey"]
    );
    Ok(())
}

#[test]
fn reopen_reads_one_journal_file_unless_the_tip_moved() -> Result<()> {
    let d = tempdir()?;
    let app = d.path().join("app");
    let stage = d.path().join("stage");
    fs::write(&stage, b"book")?;
    let mut library = Library::open(&app)?;
    let b = add(&mut library, &stage)?;
    library.edit(&b.id, "edited", &[])?;
    drop(library);
    let first = app.join("portable/changes/00000000000000000001.json");
    let tip = app.join("portable/changes/00000000000000000002.json");
    let successor = app.join("portable/changes/00000000000000000003.json");
    let original_first = fs::read(&first)?;
    let original_tip = fs::read(&tip)?;
    fs::write(&first, b"broken")?;
    let mut library = Library::open(&app)?;
    assert!(library.refresh().is_err());
    drop(library);
    fs::remove_file(&tip)?;
    assert!(Library::open(&app).is_err());
    fs::write(&first, original_first)?;
    drop(Library::open(&app)?);
    assert_eq!(fs::read(&tip)?, original_tip);
    fs::write(&tip, b"inconsistent tip")?;
    assert!(Library::open(&app).is_err());
    fs::write(&tip, original_tip)?;
    fs::write(successor, b"invalid successor")?;
    assert!(Library::open(&app).is_err());
    Ok(())
}

#[test]
fn reopen_alone_publishes_an_interrupted_import() -> Result<()> {
    for component in ["changes", "sidecars"] {
        let dir = tempdir()?;
        let app = dir.path().join("app");
        let stage = dir.path().join("stage");
        fs::write(&stage, b"original")?;
        let mut lib = Library::open(&app)?;
        let blocked = app.join("portable").join(component);
        fs::remove_dir(&blocked)?;
        fs::write(&blocked, b"unavailable")?;
        let err = lib
            .import(&stage, &stage, &facts(), ImportChoice::Review)
            .unwrap_err();
        let id = err
            .downcast_ref::<PublicationPending>()
            .unwrap()
            .book_id
            .clone();
        drop(lib);
        fs::remove_file(blocked)?;
        let mut lib = Library::open(&app)?;
        assert_eq!(fs::read(lib.managed_file(&id)?)?, b"original");
        let changes = app.join("portable/changes");
        assert_eq!(changes.read_dir()?.count(), 1);
        assert!(changes.join("00000000000000000001.json").is_file());
        let sidecars = app.join("portable/sidecars");
        assert_eq!(sidecars.read_dir()?.count(), 1);
        let sidecar: Book =
            serde_json::from_slice(&fs::read(sidecars.read_dir()?.next().unwrap()?.path())?)?;
        assert_eq!(sidecar, lib.book(&id)?);
        fs::write(&stage, b"new bytes")?;
        let outcome = lib.import(
            &stage,
            &stage,
            &ParsedMetadata {
                identifier: None,
                ..facts()
            },
            ImportChoice::Review,
        )?;
        assert!(matches!(outcome,ImportOutcome::NeedsReview(books) if books==[sidecar]));
    }
    Ok(())
}

#[test]
fn a_record_from_before_keeps_its_sidecar() -> Result<()> {
    use sha2::{Digest, Sha256};
    let dir = tempdir()?;
    let portable = dir.path().join("portable");
    fs::create_dir_all(portable.join("changes"))?;
    fs::create_dir(portable.join("sidecars"))?;
    let id = "00000000-0000-4000-8000-000000000001";
    let snapshot = format!(
        r#"{{
  "id": "{id}",
  "edition_id": "{id}",
  "file_id": "{id}",
  "version_id": "{id}",
  "title": "Old",
  "authors": [],
  "imported": {{
    "title": "Old",
    "authors": [],
    "identifier": null
  }},
  "sha256": "{}"
}}"#,
        "0".repeat(64)
    );
    let path = portable.join(format!(
        "sidecars/{id}-{:x}.json",
        Sha256::digest(snapshot.as_bytes())
    ));
    fs::write(&path, b"my edits")?;
    fs::write(
        portable.join("changes/00000000000000000001.json"),
        format!(r#"{{"format":1,"id":"{id}","parent":null,"change":{{"Book":{snapshot}}}}}"#),
    )?;
    let lib = Library::open(dir.path())?;
    assert!(lib.book(id)?.added.is_none());
    assert_eq!(fs::read_to_string(&path)?, snapshot);
    assert!(
        portable
            .join("sidecars")
            .read_dir()?
            .flatten()
            .any(|entry| fs::read(entry.path()).is_ok_and(|v| v == b"my edits"))
    );
    Ok(())
}

#[test]
fn changing_catalogue_version_preserves_old_database_and_replays_portable_history() -> Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("stage");
    fs::write(&path, b"edition")?;
    let mut library = Library::open(dir.path())?;
    let book = add(&mut library, &path)?;
    drop(library);
    let db = rusqlite::Connection::open(dir.path().join("library.sqlite"))?;
    db.execute("UPDATE meta SET value=200 WHERE key='version'", [])?;
    drop(db);
    let library = Library::open(dir.path())?;
    assert_eq!(library.book(&book.id)?, book);
    let backup = dir
        .path()
        .read_dir()?
        .flatten()
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("library.sqlite.old-")
        })
        .unwrap();
    let db = rusqlite::Connection::open(backup.path())?;
    assert_eq!(
        db.query_row("SELECT value FROM meta WHERE key='version'", [], |r| r
            .get::<_, u32>(0))?,
        200
    );
    Ok(())
}

#[test]
fn an_incomplete_portable_copy_still_fails_on_a_second_open() -> Result<()> {
    let dir = tempdir()?;
    let changes = dir.path().join("portable/changes");
    fs::create_dir_all(&changes)?;
    fs::write(changes.join("00000000000000000002.json"), b"orphan")?;
    for _ in 0..2 {
        assert!(Library::open(dir.path()).is_err());
    }
    Ok(())
}

#[test]
fn survey_applies_only_to_the_version_examined() -> Result<()> {
    let d = tempdir()?;
    let path = d.path().join("book");
    fs::write(&path, b"old")?;
    let mut lib = Library::open(d.path())?;
    let b = add(&mut lib, &path)?;
    fs::write(&path, b"replacement")?;
    let ImportOutcome::Replaced(new) =
        lib.import(&path, &path, &facts(), ImportChoice::Replace(b.id.clone()))?
    else {
        panic!("expected replacement")
    };
    let facts = SurveyFacts {
        version: 1,
        words: None,
        cover: None,
    };
    lib.set_survey(&b.id, &b.version_id, facts.clone())?;
    assert_eq!(lib.unsurveyed(1)?.as_slice(), std::slice::from_ref(&new));
    lib.set_survey(&b.id, &new.version_id, facts.clone())?;
    assert!(lib.unsurveyed(1)?.is_empty());
    assert_eq!(lib.book(&b.id)?.imported.survey, Some(facts));
    Ok(())
}
