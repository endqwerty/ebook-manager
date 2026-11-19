//! Reading generated books through the real sandbox. Statement numbers refer
//! to specs/epub-reader-mvp.md.

mod common;

use common::*;
use ebook_reader::{
    BlockKind, ChapterWarning, Identifier, Link, Position, SeriesMembership, Target,
};

fn texts(chapter: &ebook_reader::Chapter) -> Vec<String> {
    chapter.blocks.iter().map(|b| b.text()).collect()
}

/// M1, M2: title, authors and identifiers of an EPUB 3 book.
#[test]
fn inspect_returns_import_metadata_for_epub3() {
    let dir = Scratch::new();
    let path = dir.file("book.epub", &epub3());
    let m = reader().inspect(&path).unwrap();
    assert_eq!(m.title.as_deref(), Some("The Lighthouse at Low Tide"));
    let names: Vec<&str> = m.authors.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, ["Mara Ellison", "Tomás Ibáñez"]);
    assert_eq!(m.authors[0].sort_name.as_deref(), Some("Ellison, Mara"));
    assert_eq!(
        m.identifiers,
        vec![
            Identifier {
                scheme: Some("uuid".into()),
                value: "3f1c2a9e-0000-4000-8000-00000000abcd".into(),
                is_unique_id: true
            },
            Identifier {
                scheme: Some("isbn".into()),
                value: "9780000000019".into(),
                is_unique_id: false
            },
        ]
    );
    assert_eq!(m.languages, ["en"]);
    assert_eq!(m.epub_version.as_deref(), Some("3.0"));
    assert!(!m.protected);
}

/// M1, M2: the same from an EPUB 2 package with `opf:` attributes.
#[test]
fn inspect_returns_import_metadata_for_epub2() {
    let dir = Scratch::new();
    let path = dir.file("old.epub", &epub2());
    let m = reader().inspect(&path).unwrap();
    assert_eq!(m.title.as_deref(), Some("Old Roads"));
    assert_eq!(m.authors.len(), 1);
    assert_eq!(m.authors[0].name, "Edith Marsh");
    assert_eq!(m.authors[0].sort_name.as_deref(), Some("Marsh, Edith"));
    assert_eq!(m.identifiers[0].scheme.as_deref(), Some("uuid"));
    assert_eq!(
        m.identifiers[0].value,
        "11111111-2222-4333-8444-555555555555"
    );
    assert!(m.identifiers[0].is_unique_id);
    assert_eq!(
        m.identifiers[1],
        Identifier {
            scheme: Some("isbn".into()),
            value: "978-0-00-000002-6".into(),
            is_unique_id: false
        }
    );
    assert_eq!(m.epub_version.as_deref(), Some("2.0"));
}

/// M3: the same bytes give the same answer, whatever the file is called.
#[test]
fn inspect_depends_only_on_the_bytes() {
    let dir = Scratch::new();
    let bytes = epub3();
    let a = dir.file("a.epub", &bytes);
    let b = dir.file("renamed copy.bin", &bytes);
    let r = reader();
    assert_eq!(r.inspect(&a).unwrap(), r.inspect(&b).unwrap());
    assert_eq!(r.inspect(&a).unwrap(), r.inspect(&a).unwrap());
}

/// R1: chapters come in reading order, with contents labels and the linear flag.
#[test]
fn chapters_follow_the_reading_order() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("book.epub", &epub3())).unwrap();
    let got: Vec<(u32, Option<&str>, bool)> = book
        .chapters()
        .iter()
        .map(|c| (c.index, c.title.as_deref(), c.linear))
        .collect();
    assert_eq!(
        got,
        vec![
            (0, None, true),
            (1, Some("Chapter One: Arrival"), true),
            (2, Some("Chapter Two"), true),
            (3, Some("Chapter Three"), true),
            (4, None, false),
        ]
    );
    let firsts: Vec<String> = (1..4)
        .map(|i| book.read_chapter(i).unwrap().blocks[0].text())
        .collect();
    assert_eq!(firsts, ["Chapter One", "Chapter Two", "Chapter Three"]);
}

/// R2, R3: real chapter text, block by block, with whitespace as a reader sees it.
#[test]
fn chapter_text_is_readable() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("book.epub", &epub3())).unwrap();
    let ch = book.read_chapter(1).unwrap();
    assert_eq!(ch.title.as_deref(), Some("Chapter One: Arrival"));
    assert!(ch.warnings.is_empty() && !ch.truncated);
    let got: Vec<(BlockKind, String)> = ch.blocks.iter().map(|b| (b.kind, b.text())).collect();
    assert_eq!(
        got,
        vec![
            (BlockKind::Heading { level: 1 }, "Chapter One".into()),
            (
                BlockKind::Paragraph,
                "The keeper\u{2019}s boat came in on the morning tide\u{2014}late, as\u{a0}always."
                    .into()
            ),
            (
                BlockKind::Paragraph,
                "\u{201c}You\u{2019}re the new one,\u{201d} she said.".into()
            ),
            (BlockKind::Heading { level: 2 }, "The Storm".into()),
            (BlockKind::Paragraph, "Wind first.\nThen rain.".into()),
            (BlockKind::Quote, "Keep the lamp lit.".into()),
            (
                BlockKind::Paragraph,
                "See the note1, the next chapter and the society page.".into()
            ),
            (BlockKind::Separator, "".into()),
            (
                BlockKind::ListItem {
                    depth: 1,
                    ordinal: None
                },
                "oil".into()
            ),
            (
                BlockKind::ListItem {
                    depth: 1,
                    ordinal: None
                },
                "wick".into()
            ),
        ]
    );
    let italic: Vec<&str> = ch.blocks[1]
        .spans
        .iter()
        .filter(|s| s.italic)
        .map(|s| s.text.trim())
        .collect();
    assert_eq!(italic, ["morning"]);
}

/// R4: links are data. Internal ones name a chapter; outside ones are text.
#[test]
fn links_are_returned_as_data() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("book.epub", &epub3())).unwrap();
    let ch = book.read_chapter(1).unwrap();
    let links: Vec<(String, Link)> = ch.blocks[6]
        .spans
        .iter()
        .filter_map(|s| s.link.clone().map(|l| (s.text.trim().to_owned(), l)))
        .collect();
    assert_eq!(
        links,
        vec![
            (
                "note".into(),
                Link::Internal(Target {
                    chapter: 4,
                    anchor: Some("n1".into())
                })
            ),
            (
                "next chapter".into(),
                Link::Internal(Target {
                    chapter: 2,
                    anchor: Some("second".into())
                })
            ),
            (
                "society page".into(),
                Link::External("https://example.com/lighthouses?x=1".into())
            ),
        ]
    );
    let notes = book.read_chapter(4).unwrap();
    assert_eq!(notes.block_for_anchor("n1"), Some(0));
}

/// R5: contents entries in order, nested, resolved to chapters and anchors.
#[test]
fn contents_navigate_to_chapters_and_anchors() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("book.epub", &epub3())).unwrap();
    type Row<'a> = (&'a str, u32, Option<(u32, Option<&'a str>)>);
    let got: Vec<Row> = book
        .toc()
        .iter()
        .map(|e| {
            (
                e.title.as_str(),
                e.depth,
                e.target.as_ref().map(|t| (t.chapter, t.anchor.as_deref())),
            )
        })
        .collect();
    assert_eq!(
        got,
        vec![
            ("Chapter One: Arrival", 0, Some((1, None))),
            ("The Storm", 1, Some((1, Some("storm")))),
            ("Chapter Two", 0, Some((2, None))),
            ("Part Two", 0, None),
            ("Chapter Three", 1, Some((3, Some("c3")))),
            ("Online extra", 0, None),
        ]
    );
    let storm = book.toc()[1].target.clone().unwrap();
    let ch = book.read_chapter(storm.chapter).unwrap();
    let block = ch
        .block_for_anchor(storm.anchor.as_deref().unwrap())
        .unwrap();
    assert_eq!(ch.blocks[block as usize].text(), "The Storm");
    assert_eq!(ch.block_for_anchor("page2"), Some(2));
}

/// R5: EPUB 2 contents from the NCX. R3: HTML-style markup and Windows-1252.
#[test]
fn epub2_book_reads_with_ncx_contents() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("old.epub", &epub2())).unwrap();
    let toc: Vec<(&str, u32, Option<&str>)> = book
        .toc()
        .iter()
        .map(|e| {
            (
                e.title.as_str(),
                e.depth,
                e.target.as_ref().and_then(|t| t.anchor.as_deref()),
            )
        })
        .collect();
    assert_eq!(
        toc,
        vec![
            ("The North Road", 0, None),
            ("A Detour", 1, Some("detour")),
            ("The South Road", 0, None)
        ]
    );
    assert_eq!(book.toc()[2].target.as_ref().unwrap().chapter, 1);
    let one = book.read_chapter(0).unwrap();
    assert_eq!(
        texts(&one),
        [
            "The North Road",
            "It was \u{201c}fine\u{201d} weather \u{2014} for March.",
            "Second paragraph\nwith a break.",
            "A Detour",
            "Fish & chips, 5\u{a0}miles on.",
        ]
    );
    assert_eq!(one.block_for_anchor("detour"), Some(3));
    assert_eq!(
        texts(&book.read_chapter(1).unwrap()),
        ["The South Road", "Home again."]
    );
}

/// R6: nothing active or remote comes out of a chapter.
#[test]
fn active_content_never_reaches_the_host() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("book.epub", &epub3())).unwrap();
    let ch = book.read_chapter(3).unwrap();
    assert_eq!(
        texts(&ch),
        ["Chapter Three", "The last chapter.", "", "The end."]
    );
    assert_eq!(ch.blocks[2].kind, BlockKind::Image);
    let mut all = ch.plain_text();
    for block in &ch.blocks {
        all.push_str(&format!(
            "{:?}{:?}",
            block.anchors,
            block.spans.iter().map(|s| &s.link).collect::<Vec<_>>()
        ));
    }
    for needle in [
        "tracker",
        "script",
        "pwned",
        "onclick",
        "iframe",
        "invisible",
    ] {
        assert!(!all.contains(needle), "{needle} leaked: {all}");
    }
    let cover = book.read_chapter(0).unwrap();
    assert_eq!(cover.blocks.len(), 1);
    assert_eq!(cover.blocks[0].kind, BlockKind::Image);
    let two = book.read_chapter(2).unwrap();
    assert_eq!(two.blocks[2].kind, BlockKind::Image);
    assert_eq!(two.blocks[2].text(), "A sketch of the lamp room");
    assert_eq!(two.blocks[3].text(), "Café naïve & co.");
}

/// R7: a saved position survives a restart: new reader, new book, same place.
#[test]
fn position_round_trips_through_the_host() {
    let dir = Scratch::new();
    let path = dir.file("book.epub", &epub3());
    let saved = {
        let book = reader().open(&path).unwrap();
        let ch = book.read_chapter(1).unwrap();
        let block = ch.block_for_anchor("storm").unwrap() + 1;
        Position {
            chapter: ch.index,
            block,
        }
        .to_string()
    };
    assert_eq!(saved, "epubpos1:1:4");

    let book = reader().open(&path).unwrap();
    let pos = book.clamp(saved.parse().unwrap());
    let ch = book.read_chapter(pos.chapter).unwrap();
    assert_eq!(
        ch.blocks[pos.block as usize].text(),
        "Wind first.\nThen rain."
    );

    assert_eq!(
        book.clamp(Position {
            chapter: 99,
            block: 7
        }),
        Position {
            chapter: 4,
            block: 0
        }
    );
    for bad in [
        "",
        "epubpos1:1",
        "epubpos2:1:1",
        "epubpos1:-1:0",
        "epubpos1:1:2:3",
        "epubpos1:99999999999:0",
        "epubpos1: 1:0",
    ] {
        assert!(bad.parse::<Position>().is_err(), "{bad:?} parsed");
    }
}

/// R8: a chapter with damaged markup still gives its text, with a warning.
#[test]
fn damaged_markup_is_read_as_far_as_possible() {
    let dir = Scratch::new();
    let bytes = simple(
        &[
            "<p>Good start.</p><p>Then <b>trouble</p><p>and more text.</p><p class=\"x",
            "<p>Fine.</p>",
        ],
        &[],
    );
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    let ch = book.read_chapter(0).unwrap();
    assert_eq!(ch.blocks[0].text(), "Good start.");
    assert!(ch.plain_text().contains("Then trouble"));
    assert_eq!(ch.warnings, [ChapterWarning::MalformedMarkup]);
    assert_eq!(texts(&book.read_chapter(1).unwrap()), ["Fine."]);
}

/// R9: a long chapter is cut at the block limit and flagged.
#[test]
fn long_chapters_are_cut_at_the_block_limit() {
    let dir = Scratch::new();
    let body = "<p>Line of text in a very long chapter.</p>\n".repeat(3000);
    let mut cfg = config();
    cfg.limits.max_blocks_per_chapter = 1000;
    let book = ebook_reader::Reader::new(cfg)
        .unwrap()
        .open(&dir.file("b.epub", &simple(&[&body], &[])))
        .unwrap();
    let ch = book.read_chapter(0).unwrap();
    assert_eq!(ch.blocks.len(), 1000);
    assert!(ch.truncated);
}

/// R10: a book of ordinary novel size opens and turns pages quickly.
#[test]
fn a_novel_sized_book_is_read_within_bounds() {
    let dir = Scratch::new();
    let para = "<p>It was the kind of morning that made the harbour look painted, all flat light and still water, and nobody on the quay was in any hurry to prove otherwise.</p>\n";
    let chapter = format!("<h1>Chapter</h1>{}", para.repeat(400));
    let bodies: Vec<&str> = (0..60).map(|_| chapter.as_str()).collect();
    let path = dir.file("novel.epub", &simple(&bodies, &[]));
    let r = reader();
    let started = std::time::Instant::now();
    let book = r.open(&path).unwrap();
    assert_eq!(book.chapters().len(), 60);
    let mut words = 0usize;
    for info in book.chapters() {
        let ch = book.read_chapter(info.index).unwrap();
        assert_eq!(ch.blocks.len(), 401);
        words += ch.plain_text().split_whitespace().count();
    }
    assert!(words > 600_000, "{words}");
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs() < 30,
        "reading every chapter took {elapsed:?}"
    );
}

/// R6: a chapter that refers to remote resources says so; nothing is fetched.
#[test]
fn remote_references_are_flagged() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("book.epub", &epub3())).unwrap();
    assert_eq!(
        book.read_chapter(3).unwrap().warnings,
        [ChapterWarning::RemoteReferences]
    );
    assert!(book.read_chapter(2).unwrap().warnings.is_empty());
}

/// R7: a saved block index is brought into the chapter that was read, and an
/// empty chapter has nowhere to scroll.
#[test]
fn saved_blocks_are_clamped_to_the_chapter() {
    let dir = Scratch::new();
    let opf = br#"<package version="3.0"><metadata/><manifest>
        <item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
        <item id="f" href="font.otf" media-type="font/otf"/></manifest>
        <spine><itemref idref="a"/><itemref idref="f"/></spine></package>"#;
    let bytes = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf),
        (
            "OEBPS/a.xhtml",
            b"<html><body><p>One.</p><p>Two.</p></body></html>",
        ),
        ("OEBPS/font.otf", b"not text"),
    ]);
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    let text = book.read_chapter(0).unwrap();
    assert_eq!(text.clamp_block(0), Some(0));
    assert_eq!(text.clamp_block(1), Some(1));
    assert_eq!(text.clamp_block(u32::MAX), Some(1));
    let empty = book.read_chapter(1).unwrap();
    assert_eq!(empty.warnings, [ChapterWarning::UnsupportedContent]);
    assert_eq!(empty.clamp_block(5), None);
}

/// R8: a picture in the reading order, SVG or raster, is a placeholder.
#[test]
fn pictures_in_the_reading_order_are_placeholders() {
    let dir = Scratch::new();
    let opf = br#"<package version="3.0"><metadata/><manifest>
        <item id="s" href="plate.svg" media-type="image/svg+xml"/>
        <item id="p" href="plate.png" media-type="image/png"/>
        <item id="g" href="gone.png" media-type="image/png"/></manifest>
        <spine><itemref idref="s"/><itemref idref="p"/><itemref idref="g"/></spine></package>"#;
    let bytes = zip(&[
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf),
        (
            "OEBPS/plate.svg",
            b"<svg xmlns='http://www.w3.org/2000/svg'><text>drawn words</text></svg>",
        ),
        ("OEBPS/plate.png", b"\x89PNG"),
    ]);
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    for index in [0, 1] {
        let ch = book.read_chapter(index).unwrap();
        assert_eq!(ch.blocks.len(), 1);
        assert_eq!(ch.blocks[0].kind, BlockKind::Image);
        assert!(ch.warnings.is_empty());
    }
    assert!(matches!(
        book.read_chapter(2),
        Err(ebook_reader::ReaderError::InvalidBook { .. })
    ));
}

fn series(name: &str, position: Option<&str>) -> SeriesMembership {
    SeriesMembership {
        name: name.into(),
        position: position.map(Into::into),
    }
}

/// M5: EPUB 3 series collections, in package order, with their positions.
/// Collections of another type are not series.
#[test]
fn epub3_series_come_in_order_with_positions() {
    let bytes = package(Parts {
        metadata: r##"
<meta property="belongs-to-collection" id="s1">The Tide Cycle</meta>
<meta refines="#s1" property="collection-type">series</meta>
<meta refines="#s1" property="group-position">2.5</meta>
<meta property="belongs-to-collection" id="set">Boxed Edition</meta>
<meta refines="#set" property="collection-type">set</meta>
<meta property="belongs-to-collection" id="s2">Harbour  Stories</meta>
<meta refines="#s2" property="collection-type">series</meta>
<meta property="belongs-to-collection" id="s3">Lamps</meta>
<meta refines="#s3" property="collection-type">series</meta>
<meta refines="#s3" property="group-position">TPB1</meta>"##,
        chapters: &["<p>x</p>"],
        ..Parts::default()
    });
    let dir = Scratch::new();
    let m = reader().inspect(&dir.file("b.epub", &bytes)).unwrap();
    assert_eq!(
        m.series,
        [
            series("The Tide Cycle", Some("2.5")),
            series("Harbour Stories", None),
            series("Lamps", Some("TPB1")),
        ]
    );
}

/// M5: the EPUB 2 series names, with a whole-number index written as a
/// decimal shown as the whole number.
#[test]
fn epub2_series_is_read_from_its_meta_names() {
    let dir = Scratch::new();
    let read = |index: &str| {
        let metadata = format!(
            r#"<meta name="calibre:series" content="Old Roads"/><meta name="calibre:series_index" content="{index}"/>"#
        );
        let bytes = package(Parts {
            version: "2.0",
            metadata: &metadata,
            chapters: &["<p>x</p>"],
            ..Parts::default()
        });
        reader()
            .inspect(&dir.file("b.epub", &bytes))
            .unwrap()
            .series
    };
    assert_eq!(read("3.0"), [series("Old Roads", Some("3"))]);
    assert_eq!(read("1.5"), [series("Old Roads", Some("1.5"))]);
    assert_eq!(read(""), [series("Old Roads", None)]);
    assert!(reader()
        .inspect(&dir.file("none.epub", &epub2()))
        .unwrap()
        .series
        .is_empty());
}

/// M6: the publication date as written, and the page direction.
#[test]
fn publication_date_and_page_direction_are_returned() {
    let dir = Scratch::new();
    let epub2_dates = package(Parts {
        version: "2.0",
        metadata: r#"<dc:date opf:event="modification">2020-01-01</dc:date><dc:date opf:event="publication">1931-05</dc:date>"#,
        chapters: &["<p>x</p>"],
        ..Parts::default()
    });
    let m = reader().inspect(&dir.file("a.epub", &epub2_dates)).unwrap();
    assert_eq!(m.published.as_deref(), Some("1931-05"));
    assert!(!m.right_to_left);

    let rtl = package(Parts {
        metadata: "<dc:date> 2019-04-02 </dc:date>",
        spine_attrs: r#" page-progression-direction="rtl""#,
        chapters: &["<p>x</p>"],
        ..Parts::default()
    });
    let m = reader().inspect(&dir.file("b.epub", &rtl)).unwrap();
    assert_eq!(m.published.as_deref(), Some("2019-04-02"));
    assert!(m.right_to_left);
    assert!(
        reader()
            .open(&dir.file("c.epub", &rtl))
            .unwrap()
            .metadata()
            .right_to_left
    );

    let undated = reader().inspect(&dir.file("d.epub", &epub3())).unwrap();
    assert_eq!(undated.published, None);
}
