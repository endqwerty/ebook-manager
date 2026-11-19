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
    // Exactly 60 elements and text runs: the line end before the div, the
    // div, the ul, and 19 items of li, b and text.
    assert_eq!(parsed.elements.len(), 40);
    assert_eq!(parsed.text, format!("\n{}", "item".repeat(19)));
    assert_eq!(content.length as usize, parsed.text.encode_utf16().count());
    assert!(content.markup.ends_with("</li></ul></div>"));
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

/// C5, S6: the host refuses content and resource replies that do not add up,
/// whatever a misbehaving helper sends.
#[test]
fn malformed_content_and_resource_replies_are_refused() {
    use ebook_reader::wire::{base64_encode, BookOutline, Payload, Response, WireResource};
    use ebook_reader::{ChapterContent, ChapterInfo, Metadata};
    let dir = Scratch::new();
    let book = dir.file("b.epub", &simple(&["<p>x</p>"], &[]));
    let outline = serde_json::to_string(&Response::Ok(Payload::Book(BookOutline {
        metadata: Metadata::default(),
        chapters: vec![ChapterInfo {
            index: 0,
            title: None,
            linear: true,
            size: u32::MAX,
        }],
        toc: vec![],
    })))
    .unwrap();
    let outline = dir.file("outline.json", outline.as_bytes());
    // A stand-in for the whole sandbox that answers with fixed replies.
    let fake = |name: &str, reply: &Payload| {
        let reply = match reply {
            Payload::Content(c) => Response::Ok(Payload::Content(c.clone())),
            Payload::Resource(r) => Response::Ok(Payload::Resource(WireResource {
                kind: r.kind,
                base64: r.base64.clone(),
            })),
            _ => unreachable!(),
        };
        let reply = dir.file(
            &format!("{name}.json"),
            serde_json::to_string(&reply).unwrap().as_bytes(),
        );
        let script = format!(
            "#!/bin/sh\ncase \"$*\" in\n*' -- /helper probe '*) echo '{{\"Ok\":{{\"Probe\":{{\"isolated\":true}}}}}}';;\n*' -- /helper open '*) cat '{}';;\n*) cat '{}';;\nesac\n",
            outline.display(),
            reply.display()
        );
        let path = dir.file(name, script.as_bytes());
        std::process::Command::new("/bin/chmod")
            .arg("+x")
            .arg(&path)
            .status()
            .unwrap();
        let cfg = ebook_reader::ReaderConfig {
            bwrap_path: Some(path),
            ..config()
        };
        // A program written a moment ago can be "text file busy" while
        // another test thread is between fork and exec.
        for _ in 0..50 {
            match ebook_reader::Reader::new(cfg.clone()).and_then(|r| r.open(&book)) {
                Ok(book) => return book,
                Err(e) if format!("{e:?}").to_lowercase().contains("busy") => {
                    std::thread::sleep(std::time::Duration::from_millis(20))
                }
                Err(e) => panic!("{name}: {e:?}"),
            }
        }
        panic!("{name}: the stand-in never ran");
    };
    let content = |markup: &str, length: u32| {
        Payload::Content(ChapterContent {
            index: 7,
            title: Some("A\u{7}title".into()),
            markup: markup.into(),
            length,
            styles: vec![ResourceId(3)],
            language: Some("e\u{0}n".into()),
            right_to_left: false,
            truncated: false,
            warnings: vec![],
        })
    };
    let opened = fake("good", &content("<p>a\u{1b}b\tc</p>", 4));
    // A size past the chapter limit is brought down to it.
    assert_eq!(
        opened.chapters()[0].size as u64,
        config().limits.max_chapter_bytes
    );
    let good = opened.read_content(0).unwrap();
    assert_eq!(good.index, 0);
    assert_eq!(good.markup, "<p>ab\tc</p>");
    assert_eq!(good.title.as_deref(), Some("Atitle"));
    assert_eq!(good.language.as_deref(), Some("en"));
    assert!(good.styles.is_empty());
    // The length must be the text's own, counted the way the page will.
    let astral = fake(
        "astral",
        &content("<p title=\"x&gt;y\">a\u{1F600} &amp; &lt;b&gt;</p>", 9),
    );
    assert_eq!(astral.read_content(0).unwrap().length, 9);
    for (name, markup, length) in [
        ("long", "<p>ab</p>", 10),
        ("short", "<p>x</p>", 0),
        ("scalar", "<p>a\u{1F600}</p>", 2),
        ("unclosed", "<p>x</p", 1),
        ("entity", "<p>&nbsp;</p>", 1),
    ] {
        match fake(name, &content(markup, length)).read_content(0) {
            Err(ReaderError::HelperFailed { .. }) => {}
            other => panic!("{name}: expected a refusal, got {other:?}"),
        }
    }

    let png = picture(1, 1, ImageFormat::Png, |_, _| [0, 0, 0, 255]);
    let resource = |kind, base64: String| Payload::Resource(WireResource { kind, base64 });
    let ok = fake("png", &resource(ResourceKind::Png, base64_encode(&png)))
        .read_resource(ResourceId(0))
        .unwrap();
    assert_eq!(ok.bytes, png);
    for (name, reply) in [
        (
            "mislabelled",
            resource(ResourceKind::Jpeg, base64_encode(&png)),
        ),
        (
            "not-a-picture",
            resource(ResourceKind::Png, base64_encode(b"<script>")),
        ),
        (
            "not-base64",
            resource(ResourceKind::Png, "iVBORw0KGgo!".into()),
        ),
        ("empty", resource(ResourceKind::Gif, String::new())),
        ("loose-padding", {
            // The same bytes with a bit set that canonical base64 leaves 0.
            let mut text = base64_encode(&png[..png.len() - png.len() % 3 + 1]);
            let at = text.len() - 3;
            text.replace_range(at..at + 1, "R");
            resource(ResourceKind::Png, text)
        }),
    ] {
        match fake(name, &reply).read_resource(ResourceId(0)) {
            Err(ReaderError::HelperFailed { .. }) => {}
            other => panic!("{name}: expected a refusal, got {other:?}"),
        }
    }
}

/// C4, C8: the first manifest item to name a file decides what it is, and a
/// reading-order item is a picture only when it says so itself.
#[test]
fn a_file_is_what_its_first_manifest_item_says() {
    let dir = Scratch::new();
    let png = picture(2, 2, ImageFormat::Png, |_, _| [0, 0, 0, 255]);
    let bytes = package(Parts {
        manifest: r#"<item id="text" href="shared.png" media-type="text/plain"/>
<item id="png" href="shared.png" media-type="image/png"/>
<item id="svg" href="shared.png" media-type="image/svg+xml"/>
<item id="other" href="notes.txt" media-type="text/plain"/>
<item id="absent" href="absent.xhtml" media-type="application/xhtml+xml"/>"#,
        chapters: &[r#"<p><img src="shared.png" alt="Shared"/></p>"#],
        spine_extra: r#"<itemref idref="svg"/><itemref idref="other"/><itemref idref="absent"/>"#,
        files: &[("OEBPS/shared.png", &png), ("OEBPS/notes.txt", b"notes")],
        ..Parts::default()
    });
    let book = reader().open(&dir.file("b.epub", &bytes)).unwrap();
    let first = book.read_content(0).unwrap();
    assert_eq!(first.markup.trim(), "<p>Shared</p>");
    for index in [1, 2] {
        let item = book.read_content(index).unwrap();
        assert_eq!(item.markup, "", "item {index}");
        assert_eq!(item.warnings, [ChapterWarning::UnsupportedContent]);
    }
    // C7: a file the archive lacks has size 0.
    let sizes: Vec<u32> = book.chapters().iter().map(|c| c.size).collect();
    assert_eq!(sizes[3], 0);
    assert_eq!(sizes[2], 5);
    // C8: the title falls back to the first heading alone.
    let titled = simple(&["<h2>First</h2><p>para</p><h3>Second</h3>"], &[]);
    let book = reader().open(&dir.file("t.epub", &titled)).unwrap();
    assert_eq!(
        book.read_content(0).unwrap().title.as_deref(),
        Some("First")
    );
}
