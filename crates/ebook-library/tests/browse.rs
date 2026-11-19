use ebook_library::*;
use tempfile::TempDir;

struct Shelf {
    _dir: TempDir,
    lib: Library,
    reader: BrowseReader,
    seq: u32,
}
impl Shelf {
    fn new() -> Result<Self> {
        let dir = tempfile::tempdir()?;
        let lib = Library::open(dir.path())?;
        let reader = lib.browse_reader()?;
        Ok(Self {
            _dir: dir,
            lib,
            reader,
            seq: 0,
        })
    }
    fn add(&mut self, m: ParsedMetadata) -> Result<Book> {
        self.seq += 1;
        let path = self._dir.path().join(self.seq.to_string());
        std::fs::write(&path, self.seq.to_string())?;
        let ImportOutcome::Added(b) =
            self.lib
                .import(&path, &path, &m, ImportChoice::KeepSeparate)?
        else {
            panic!("expected addition")
        };
        Ok(b)
    }
    fn view(&mut self, text: &str, terms: &[(Field, bool)]) -> Result<OpenView> {
        let sort = if terms.is_empty() {
            SortOrder::default()
        } else {
            SortOrder::new(
                terms
                    .iter()
                    .map(|&(field, descending)| SortTerm { field, descending })
                    .collect(),
            )?
        };
        self.reader.open(
            &BrowseQuery {
                text: text.into(),
                sort,
            },
            &Cancel::default(),
        )
    }
}
fn meta(title: &str) -> ParsedMetadata {
    ParsedMetadata {
        title: title.into(),
        ..Default::default()
    }
}
fn titles(v: OpenView) -> Vec<String> {
    v.first.into_iter().map(|r| r.title).collect()
}
#[test]
fn titles_file_without_articles_and_with_numbers_by_value() -> Result<()> {
    let mut s = Shelf::new()?;
    for t in ["Book 10", "The Book 2", "Atlas"] {
        s.add(ParsedMetadata {
            language: Some("en".into()),
            ..meta(t)
        })?;
    }
    assert_eq!(titles(s.view("", &[])?), ["Atlas", "The Book 2", "Book 10"]);
    Ok(())
}
#[test]
fn series_positions_sort_by_parts() -> Result<()> {
    let mut s = Shelf::new()?;
    for t in [
        "TPB10", "1-5", "TPB1", "2.2.1", "1.5", "2.10.1", "10", "2", "1",
    ] {
        s.add(ParsedMetadata {
            series: vec![("series".into(), Some(t.into()))],
            ..meta(t)
        })?;
    }
    assert_eq!(
        titles(s.view("", &[(Field::Series, false)])?),
        [
            "1", "1-5", "1.5", "2", "2.2.1", "2.10.1", "10", "TPB1", "TPB10"
        ]
    );
    Ok(())
}
#[test]
fn search_matches_every_word_across_fields() -> Result<()> {
    let mut s = Shelf::new()?;
    s.add(ParsedMetadata {
        series: vec![("Straße".into(), None)],
        authors: vec!["André".into()],
        ..meta("Sea")
    })?;
    s.add(meta("Sea"))?;
    assert_eq!(titles(s.view("  andRe STRASSE sea", &[])?), ["Sea"]);
    assert_eq!(s.view("  ", &[])?.count, 2);
    Ok(())
}
#[test]
fn orders_by_each_field_in_both_directions() -> Result<()> {
    let mut s = Shelf::new()?;
    for (i, t) in ["B", "A"].into_iter().enumerate() {
        let b = s.add(ParsedMetadata {
            authors: vec![t.into()],
            series: vec![(t.into(), None)],
            published: Some((2020 - i).to_string()),
            ..meta(t)
        })?;
        s.lib.save_position(
            &b.id,
            &ReadingPosition {
                version_id: b.version_id,
                chapter: 0,
                offset: 0,
                fraction: Some(1. - i as f32),
                last_read: Some(500 - i as i64),
            },
        )?;
    }
    for field in [
        Field::Title,
        Field::Author,
        Field::Series,
        Field::Published,
        Field::Progress,
        Field::LastRead,
    ] {
        for desc in [false, true] {
            assert_eq!(
                titles(s.view("", &[(field, desc)])?),
                if desc { ["B", "A"] } else { ["A", "B"] }
            );
        }
    }
    for desc in [false, true] {
        let v = s.view("", &[(Field::Added, desc)])?;
        assert_eq!(v.count, 2);
        assert!(v.first.windows(2).all(|w| if desc {
            w[0].added >= w[1].added
        } else {
            w[0].added <= w[1].added
        }));
    }
    Ok(())
}
#[test]
fn later_keys_order_ties() -> Result<()> {
    let mut s = Shelf::new()?;
    for (t, a, p) in [
        ("B", "Z", "2010"),
        ("C", "A", "2010"),
        ("A", "Z", "2010"),
        ("D", "Z", "2000"),
    ] {
        s.add(ParsedMetadata {
            authors: vec![a.into()],
            published: Some(p.into()),
            ..meta(t)
        })?;
    }
    assert_eq!(
        titles(s.view(
            "",
            &[
                (Field::Published, false),
                (Field::Author, true),
                (Field::Title, false)
            ]
        )?),
        ["D", "A", "B", "C"]
    );
    Ok(())
}
#[test]
fn missing_values_sort_last() -> Result<()> {
    let mut s = Shelf::new()?;
    s.add(meta("Absent"))?;
    s.add(ParsedMetadata {
        published: Some("unknown".into()),
        ..meta("Text")
    })?;
    s.add(ParsedMetadata {
        published: Some("2000".into()),
        ..meta("Year")
    })?;
    for desc in [false, true] {
        assert_eq!(
            titles(s.view("", &[(Field::Published, desc)])?),
            ["Year", "Absent", "Text"]
        );
    }
    Ok(())
}
#[test]
fn any_window_matches_the_whole_view() -> Result<()> {
    let mut s = Shelf::new()?;
    for i in (0..131).rev() {
        s.add(meta(&format!("Book {i}")))?;
    }
    let v = s.view("", &[])?;
    assert_eq!(v.first.len(), 128);
    for (start, len) in [(0, 2), (127, 30), (130, 1), (500, 10)] {
        let rows = s.reader.window(v.token, start, len)?;
        assert!(
            rows.into_iter()
                .map(|r| r.title)
                .eq((start..(start + len).min(131)).map(|i| format!("Book {i}")))
        );
    }
    Ok(())
}
#[test]
fn an_old_token_is_refused() -> Result<()> {
    let mut s = Shelf::new()?;
    s.add(meta("Book"))?;
    let old = s.view("", &[])?;
    let cancel = Cancel::default();
    cancel.cancel();
    assert!(
        s.reader
            .open(&Default::default(), &cancel)
            .unwrap_err()
            .is::<Cancelled>()
    );
    assert_eq!(s.reader.window(old.token, 0, 1)?.len(), 1);
    s.view("X", &[])?;
    assert!(
        s.reader
            .window(old.token, 0, 1)
            .unwrap_err()
            .is::<StaleView>()
    );
    Ok(())
}
#[test]
fn a_reopened_view_reflects_changes() -> Result<()> {
    let mut s = Shelf::new()?;
    let b = s.add(meta("Before"))?;
    s.add(meta("Middle"))?;
    let old = s.view("", &[(Field::Author, false)])?;
    assert!(!s.reader.changed()?);
    s.lib.edit(&b.id, "Zebra", &[])?;
    s.add(meta("Added"))?;
    assert!(s.reader.changed()?);
    // The view is still readable. It shows current values at the ranks the
    // books had when it opened, and not the book added since.
    let rows = s.reader.window(old.token, 0, 10)?;
    assert_eq!(
        rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
        ["Zebra", "Middle"]
    );
    assert_eq!(s.reader.shelf(old.token)?.thickness.len(), 2);
    assert_eq!(
        titles(s.view("", &[])?),
        ["Added", "Middle", "Zebra"],
        "a reopened view has the new order"
    );
    s.lib.save_position(
        &b.id,
        &ReadingPosition {
            version_id: b.version_id,
            chapter: 0,
            offset: 1,
            fraction: Some(0.5),
            last_read: Some(80),
        },
    )?;
    assert!(s.reader.changed()?);
    assert_eq!(s.view("zebra", &[])?.first[0].progress, Some(0.5));
    Ok(())
}
#[test]
fn a_write_after_open_does_not_stop_locate() -> Result<()> {
    let mut s = Shelf::new()?;
    let a = s.add(meta("A"))?;
    let b = s.add(meta("B"))?;
    let v = s.view("", &[])?;
    let later = s.add(meta("Aa"))?;
    assert_eq!(
        s.reader.locate(v.token, &[b.id, later.id, a.id])?,
        [Some(1), None, Some(0)]
    );
    Ok(())
}
#[test]
fn locate_finds_books_by_id() -> Result<()> {
    let mut s = Shelf::new()?;
    let z = s.add(meta("Z match"))?;
    let hidden = s.add(meta("hidden"))?;
    let a = s.add(meta("A match"))?;
    let v = s.view("match", &[])?;
    assert_eq!(
        s.reader.locate(v.token, &[z.id, hidden.id, a.id])?,
        [Some(1), None, Some(0)]
    );
    Ok(())
}
#[test]
fn shelf_strip_matches_the_view() -> Result<()> {
    let mut s = Shelf::new()?;
    s.add(meta("Missing"))?;
    s.add(ParsedMetadata {
        authors: vec!["Herman Melville".into()],
        series: vec![("Cycle".into(), None)],
        published: Some("2000-01".into()),
        survey: Some(SurveyFacts {
            words: Some(6000),
            version: 1,
            cover: None,
        }),
        ..meta("Known")
    })?;
    for (f, a, b) in [
        (Field::Author, "M", "Unknown author"),
        (Field::Series, "Cycle", "No series"),
        (Field::Published, "2000", "No date"),
    ] {
        let v = s.view("", &[(f, false)])?;
        let shelf = s.reader.shelf(v.token)?;
        assert_eq!(&*shelf.thickness, &[17, 24]);
        assert_eq!(&*shelf.groups, &[(0, a.into()), (1, b.into())]);
    }
    Ok(())
}

#[test]
fn author_groups_follow_the_letter_as_filed() -> Result<()> {
    // Plain letters, so the answer is the same in every locale. Letters
    // with marks and locale rules are checked beside `Keys`.
    let mut s = Shelf::new()?;
    for name in [
        "John Adams",
        "jane austen",
        "Umberto Eco",
        "George ELIOT",
        "10cc",
    ] {
        s.add(ParsedMetadata {
            authors: vec![name.into()],
            ..meta(name)
        })?;
    }
    let v = s.view("", &[(Field::Author, false)])?;
    let shelf = s.reader.shelf(v.token)?;
    assert_eq!(
        &*shelf.groups,
        &[(0, "#".into()), (1, "A".into()), (3, "E".into())]
    );
    Ok(())
}

#[test]
fn a_new_key_profile_refiles_existing_rows() -> Result<()> {
    let mut s = Shelf::new()?;
    s.add(meta("Book 10"))?;
    s.add(meta("Book 2"))?;
    let Shelf {
        _dir: dir,
        lib,
        reader,
        ..
    } = s;
    drop(reader);
    drop(lib);
    let db = rusqlite::Connection::open(dir.path().join("library.sqlite"))?;
    db.execute(
        "UPDATE meta SET value='older profile' WHERE key='profile'",
        [],
    )?;
    db.execute("UPDATE book SET title_key=x'00'", [])?;
    drop(db);
    let lib = Library::open(dir.path())?;
    let mut reader = lib.browse_reader()?;
    assert_eq!(
        titles(reader.open(&Default::default(), &Cancel::default())?),
        ["Book 2", "Book 10"]
    );
    Ok(())
}
