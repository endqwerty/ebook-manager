//! Chapter markup and pictures for a web view, through the real sandbox.
//! Statement numbers refer to specs/epub-reader-mvp.md.

mod common;

use common::*;
use ebook_reader::{ChapterWarning, ReaderError, ResourceId, ResourceKind};
use image::ImageFormat;
use quick_xml::events::Event;

const ELEMENTS: &str = "a abbr address article aside b bdi bdo blockquote br caption cite code col colgroup dd del details dfn div dl dt em figcaption figure footer h1 h2 h3 h4 h5 h6 header hgroup hr i img ins kbd li main mark nav ol p pre q rp rt ruby s samp section small span strong sub summary sup table tbody td tfoot th thead time tr u ul var wbr";
const ATTRIBUTES: &str =
    "id class lang dir title alt colspan rowspan start value href src data-external data-epub-type";

struct Parsed {
    elements: Vec<String>,
    attributes: Vec<(String, String, String)>,
    text: String,
}

/// Parse markup as strict XML. Panics when it is not well-formed.
fn parse(markup: &str) -> Parsed {
    let doc = format!("<body>{markup}</body>");
    let mut reader = quick_xml::Reader::from_str(&doc);
    let mut out = Parsed {
        elements: Vec::new(),
        attributes: Vec::new(),
        text: String::new(),
    };
    loop {
        match reader.read_event().expect("well-formed XML") {
            Event::Eof => break,
            Event::Start(e) | Event::Empty(e) => {
                let name = String::from_utf8(e.name().as_ref().to_vec()).unwrap();
                for a in e.attributes() {
                    let a = a.expect("well-formed, unrepeated attributes");
                    out.attributes.push((
                        name.clone(),
                        String::from_utf8(a.key.as_ref().to_vec()).unwrap(),
                        a.unescape_value().unwrap().into_owned(),
                    ));
                }
                if name != "body" {
                    out.elements.push(name);
                }
            }
            Event::Text(t) => out.text.push_str(&t.unescape().unwrap()),
            _ => {}
        }
    }
    out
}

fn assert_allow_listed(parsed: &Parsed) {
    for name in &parsed.elements {
        assert!(ELEMENTS.split(' ').any(|e| e == name), "element {name}");
    }
    for (element, key, value) in &parsed.attributes {
        assert!(ATTRIBUTES.split(' ').any(|a| a == key), "attribute {key}");
        match key.as_str() {
            "href" => assert!(element == "a" && value.starts_with("/c/"), "{value}"),
            "src" => assert!(element == "img" && value.starts_with("/r/"), "{value}"),
            _ => {}
        }
    }
}

const HOSTILE: &str = r#"<div id="top" onclick="steal()" style="background:url(http://tracker.example/bg.png)">
<script>fetch('http://tracker.example/s')</script>
<script src="http://tracker.example/s.js"></script>
<P CLASS="a" class="b">One<br>two <svg:script>x()</svg:script><b><i>three</b> four</i>
<a href="c1.xhtml#n1">note</a> <a href="https://example.org/x">site</a>
<a href="javascript:steal()">js</a> <a href="file:///etc/passwd">file</a> <a href="../../../etc/passwd">climb</a>
<img src="http://tracker.example/p.png" alt="pixel" onerror="steal()"/>
<img src="images/dot.png" alt="A dot" srcset="http://tracker.example/2x.png 2x"/>
<iframe src="http://tracker.example/frame">framed</iframe>
<object data="http://tracker.example/o">embedded</object>
<form action="http://tracker.example/form"><input name="q" value="typed"/><button>Send</button>Form words</form>
<p hidden="hidden">concealed</p>
<link rel="stylesheet" href="http://tracker.example/a.css"/>
<meta http-equiv="refresh" content="0;url=http://tracker.example/"/>
<base href="http://tracker.example/"/>
<video src="http://tracker.example/v.mp4" autoplay="autoplay">moving</video>
<math><mi>x</mi></math>
</div>"#;

fn hostile_book() -> Vec<u8> {
    let dot = picture(2, 2, ImageFormat::Png, |_, _| [9, 9, 9, 255]);
    package(Parts {
        manifest: r#"<item id="dot" href="images/dot.png" media-type="image/png"/>"#,
        chapters: &[HOSTILE, "<p id=\"n1\">A note.</p>"],
        files: &[("OEBPS/images/dot.png", &dot)],
        ..Parts::default()
    })
}

/// C1: markup is well-formed XML from the allow-list, whatever the source.
#[test]
fn markup_is_well_formed_and_keeps_only_the_allow_list() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("b.epub", &hostile_book())).unwrap();
    let content = book.read_content(0).unwrap();
    let parsed = parse(&content.markup);
    assert_allow_listed(&parsed);
    assert!(
        content
            .markup
            .contains(r#"<p class="a">One<br/>two <b><i>three</i></b> four"#),
        "{}",
        content.markup
    );
    // The same holds for books written the way real ones are.
    for bytes in [epub3(), epub2()] {
        let book = reader().open(&dir.file("real.epub", &bytes)).unwrap();
        for info in book.chapters() {
            let content = book.read_content(info.index).unwrap();
            assert_allow_listed(&parse(&content.markup));
        }
    }
}

/// C2: nothing that runs, loads or leads out of the book is in the markup.
#[test]
fn hostile_markup_is_removed() {
    let dir = Scratch::new();
    let book = reader().open(&dir.file("b.epub", &hostile_book())).unwrap();
    let content = book.read_content(0).unwrap();
    let parsed = parse(&content.markup);
    for gone in [
        "steal",
        "fetch",
        "x()",
        "framed",
        "embedded",
        "typed",
        "Send",
        "concealed",
        "moving",
        "tracker.example",
        "javascript",
        "file:",
        "passwd",
        "script",
        "style=",
        "onclick",
    ] {
        assert!(
            !content.markup.contains(gone),
            "{gone} in {}",
            content.markup
        );
    }
    let addresses: Vec<&str> = parsed
        .attributes
        .iter()
        .filter(|(_, k, _)| k == "href" || k == "src" || k == "data-external")
        .map(|(_, _, v)| v.as_str())
        .collect();
    assert_eq!(addresses, ["/c/1#n1", "https://example.org/x", "/r/0"]);
    assert!(content
        .markup
        .contains(r#"<a data-external="https://example.org/x">site</a>"#));
    for kept in ["Form words", "pixel", "js", "climb"] {
        assert!(parsed.text.contains(kept), "{kept}");
    }
    assert!(content.warnings.contains(&ChapterWarning::RemoteReferences));
    assert!(content
        .warnings
        .contains(&ChapterWarning::UnsupportedContent));
    assert!(!content.truncated);
    assert!(content.styles.is_empty());
}

/// C3: the length is the UTF-16 length of the text in the markup.
#[test]
fn length_counts_the_markup_text() {
    let dir = Scratch::new();
    let body = "<h1>Tide &amp; <em>Time</em></h1>\r\n<p>a\u{1F600}b &lt;c&gt; caf\u{e9}</p><img src=\"nowhere.png\" alt=\"Lost\"/>";
    let book = reader()
        .open(&dir.file("b.epub", &simple(&[body], &[])))
        .unwrap();
    let content = book.read_content(0).unwrap();
    let parsed = parse(&content.markup);
    assert_eq!(content.length as usize, parsed.text.encode_utf16().count());
    assert!(parsed.text.contains("a\u{1F600}b <c> caf\u{e9}"));
    assert!(parsed.text.contains("caf\u{e9}Lost"), "{:?}", parsed.text);
    assert!(!content.markup.contains('\r'));
    assert_eq!(content.title.as_deref(), Some("Tide & Time"));
    assert_eq!(content.language.as_deref(), Some("en"));
    assert!(!content.right_to_left);
    for bytes in [epub3(), epub2()] {
        let book = reader().open(&dir.file("real.epub", &bytes)).unwrap();
        for info in book.chapters() {
            let c = book.read_content(info.index).unwrap();
            assert_eq!(
                c.length as usize,
                parse(&c.markup).text.encode_utf16().count(),
                "chapter {}",
                info.index
            );
        }
    }
}

/// C4: a picture reference is an id that returns that picture's bytes.
#[test]
fn a_picture_id_returns_its_bytes() {
    let dir = Scratch::new();
    let first = picture(3, 2, ImageFormat::Png, |_, _| [1, 2, 3, 255]);
    let second = picture(2, 3, ImageFormat::Jpeg, |_, _| [200, 100, 50, 255]);
    let bytes = package(Parts {
        manifest: r#"<item id="css" href="s.css" media-type="text/css"/>
<item id="one" href="img/one.png" media-type="image/png"/>
<item id="two" href="img/two.jpg" media-type="image/jpeg"/>
<item id="again" href="img/one.png" media-type="image/png"/>
<item id="lone" href="img/lone.png" media-type="image/png"/>"#,
        chapters: &[
            r#"<p><img src="img/two.jpg" alt="Two"/> <img src="img/one.png"/> <img src="s.css" alt="Sheet"/></p>
<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><image xlink:href="img/two.jpg"/></svg>"#,
        ],
        spine_extra: r#"<itemref idref="lone"/>"#,
        files: &[
            ("OEBPS/s.css", b"p{}"),
            ("OEBPS/img/one.png", &first),
            ("OEBPS/img/two.jpg", &second),
            ("OEBPS/img/lone.png", &first),
        ],
        ..Parts::default()
    });
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    let content = book.read_content(0).unwrap();
    let sources: Vec<String> = parse(&content.markup)
        .attributes
        .into_iter()
        .filter(|(_, k, _)| k == "src")
        .map(|(_, _, v)| v)
        .collect();
    assert_eq!(sources, ["/r/2", "/r/1", "/r/2"]);
    assert!(content.markup.contains("Sheet"));
    let two = book.read_resource(ResourceId(2)).unwrap();
    assert_eq!((two.kind, &two.bytes), (ResourceKind::Jpeg, &second));
    let one = book.read_resource(ResourceId(1)).unwrap();
    assert_eq!((one.kind, &one.bytes), (ResourceKind::Png, &first));
    // A picture that is a reading-order item of its own is one `img`.
    let lone = book.read_content(1).unwrap();
    assert_eq!(lone.markup, r#"<img src="/r/4" alt=""/>"#);
    assert_eq!(lone.length, 0);
}

/// C5: a resource is refused unless it is a picture of the type it claims.
#[test]
fn resources_are_checked_by_signature() {
    let dir = Scratch::new();
    let png = picture(2, 2, ImageFormat::Png, |_, _| [0, 0, 0, 255]);
    let bytes = package(Parts {
        manifest: r#"<item id="liar" href="liar.png" media-type="image/png"/>
<item id="swap" href="swap.jpg" media-type="image/jpeg"/>
<item id="svg" href="v.svg" media-type="image/svg+xml"/>
<item id="css" href="s.css" media-type="text/css"/>
<item id="gone" href="gone.png" media-type="image/png"/>"#,
        chapters: &["<p>x</p>"],
        files: &[
            ("OEBPS/liar.png", b"<html><script>x()</script></html>"),
            ("OEBPS/swap.jpg", &png),
            ("OEBPS/v.svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>"),
            ("OEBPS/s.css", b"p{}"),
        ],
        ..Parts::default()
    });
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    for id in 0..5 {
        assert!(
            matches!(
                book.read_resource(ResourceId(id)),
                Err(ReaderError::InvalidBook { .. })
            ),
            "resource {id}"
        );
    }
    // Position 5 is the chapter document, and 6 is past the end.
    for id in [5, 6, u32::MAX] {
        assert!(matches!(
            book.read_resource(ResourceId(id)),
            Err(ReaderError::InvalidBook { .. })
        ));
    }
    let mut small = config();
    small.limits.max_resource_bytes = 16;
    let tight = ebook_reader::Reader::new(small).unwrap();
    let ok = package(Parts {
        manifest: r#"<item id="p" href="p.png" media-type="image/png"/>"#,
        chapters: &["<p>x</p>"],
        files: &[("OEBPS/p.png", &png)],
        ..Parts::default()
    });
    let path = dir.file("ok.epub", &ok);
    assert!(matches!(
        tight.open(&path).unwrap().read_resource(ResourceId(0)),
        Err(ReaderError::LimitExceeded { .. })
    ));
    assert!(reader()
        .open(&path)
        .unwrap()
        .read_resource(ResourceId(0))
        .is_ok());
}

/// C6: a chapter past the limit is cut, flagged and still well-formed.
#[test]
fn truncated_markup_still_parses() {
    let dir = Scratch::new();
    let body = format!("<div><ul>{}</ul></div>", "<li><b>item</b></li>".repeat(200));
    let mut cfg = config();
    cfg.limits.max_markup_nodes = 60;
    let book = ebook_reader::Reader::new(cfg)
        .unwrap()
        .open(&dir.file("b.epub", &simple(&[&body], &[])))
        .unwrap();
    let content = book.read_content(0).unwrap();
    assert!(content.truncated);
    let parsed = parse(&content.markup);
    assert!(parsed.elements.len() <= 60);
    assert_eq!(content.length as usize, parsed.text.encode_utf16().count());
    assert!(content.markup.ends_with("</ul></div>"));
}

/// C7: a chapter's size is what the archive states, capped at the limit.
#[test]
fn chapter_sizes_are_capped() {
    let dir = Scratch::new();
    let long = format!("<p>{}</p>", "word ".repeat(2000));
    let bytes = simple(&["<p>short</p>", &long], &[]);
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    let sizes: Vec<u32> = book.chapters().iter().map(|c| c.size).collect();
    assert_eq!(sizes[0] as usize, xhtml("c", "<p>short</p>").len());
    assert_eq!(sizes[1] as usize, xhtml("c", &long).len());
    let mut cfg = config();
    cfg.limits.max_chapter_bytes = 1000;
    let capped = ebook_reader::Reader::new(cfg)
        .unwrap()
        .open(&dir.file("c.epub", &bytes))
        .unwrap();
    let sizes: Vec<u32> = capped.chapters().iter().map(|c| c.size).collect();
    assert_eq!(sizes, [xhtml("c", "<p>short</p>").len() as u32, 1000]);
}

/// C8: content calls keep the refusals of chapter reads.
#[test]
fn content_calls_refuse_what_chapter_reads_refuse() {
    let dir = Scratch::new();
    let path = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let book = reader().open(&path).unwrap();
    assert!(matches!(
        book.read_content(1),
        Err(ReaderError::ChapterOutOfRange { index: 1, count: 1 })
    ));
    std::fs::write(&path, simple(&["<p>changed and longer</p>"], &[])).unwrap();
    assert!(matches!(
        book.read_content(0),
        Err(ReaderError::SourceChanged)
    ));
    assert!(matches!(
        book.read_resource(ResourceId(0)),
        Err(ReaderError::SourceChanged)
    ));
}
