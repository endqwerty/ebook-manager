//! Generated EPUB fixtures. Every book here is synthetic.
#![allow(dead_code)]

use ebook_reader::{Reader, ReaderConfig};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

pub const HELPER: &str = env!("CARGO_BIN_EXE_ebook-reader-helper");

/// The system's sandbox launcher, for stand-ins that wrap the real one.
#[cfg(target_os = "linux")]
pub const SANDBOX: &str = "/usr/bin/bwrap";
#[cfg(target_os = "macos")]
pub const SANDBOX: &str = "/usr/bin/sandbox-exec";

#[cfg(target_os = "linux")]
pub const SLEEP: &str = "/usr/bin/sleep";
#[cfg(target_os = "macos")]
pub const SLEEP: &str = "/bin/sleep";

/// Whether a process with this id exists.
pub fn alive(pid: &str) -> bool {
    std::process::Command::new("/bin/kill")
        .args(["-0", pid])
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

pub fn config() -> ReaderConfig {
    ReaderConfig {
        helper_path: Some(PathBuf::from(HELPER)),
        ..ReaderConfig::default()
    }
}

pub fn reader() -> Reader {
    Reader::new(config()).expect("the sandbox must work on the test machine")
}

/// A scratch directory removed when dropped.
pub struct Scratch(pub PathBuf);

impl Scratch {
    pub fn new() -> Scratch {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ebook-reader-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }

    pub fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Pack files into a ZIP in the given order.
pub fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in files {
        let method = if *name == "mimetype" {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        w.start_file(
            *name,
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

pub const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

pub fn xhtml(title: &str, body: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>{title}</title><link rel="stylesheet" href="style.css"/></head>
<body>
{body}
</body>
</html>"#
    )
}

/// An EPUB 3 novel: nested folders, a navigation document, a file name with a
/// space, a non-linear note and the markup habits of real books.
pub fn epub3() -> Vec<u8> {
    let opf = r##"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="pub-id">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="pub-id">urn:uuid:3f1c2a9e-0000-4000-8000-00000000abcd</dc:identifier>
    <dc:identifier id="isbn">urn:isbn:9780000000019</dc:identifier>
    <dc:title id="sub">A Fixture in Three Parts</dc:title>
    <dc:title id="main">The Lighthouse
        at Low Tide</dc:title>
    <meta refines="#main" property="title-type">main</meta>
    <meta refines="#sub" property="title-type">subtitle</meta>
    <dc:creator id="a1">Mara Ellison</dc:creator>
    <meta refines="#a1" property="role" scheme="marc:relators">aut</meta>
    <meta refines="#a1" property="file-as">Ellison, Mara</meta>
    <dc:creator id="a2">Tom&#225;s Ib&#xE1;&#241;ez</dc:creator>
    <dc:creator id="ill">Pip Drawer</dc:creator>
    <meta refines="#ill" property="role">ill</meta>
    <dc:language>en</dc:language>
    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="css" href="text/style.css" media-type="text/css"/>
    <item id="cover" href="text/cover.xhtml" media-type="application/xhtml+xml"/>
    <item id="c1" href="text/chapter1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="text/chapter%202.xhtml" media-type="application/xhtml+xml"/>
    <item id="c3" href="text/chapter3.xhtml" media-type="application/xhtml+xml" properties="scripted"/>
    <item id="notes" href="text/notes.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="images/cover.png" media-type="image/png"/>
  </manifest>
  <spine>
    <itemref idref="cover"/>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
    <itemref idref="c3"/>
    <itemref idref="notes" linear="no"/>
    <itemref idref="missing-from-manifest"/>
  </spine>
</package>"##;
    let nav = xhtml(
        "Contents",
        r#"<nav epub:type="landmarks"><ol><li><a epub:type="cover" href="text/cover.xhtml">Cover</a></li></ol></nav>
<nav epub:type="toc" id="toc"><h1>Contents</h1>
<ol>
  <li><a href="text/chapter1.xhtml">Chapter One: <em>Arrival</em></a>
    <ol><li><a href="text/chapter1.xhtml#storm">The Storm</a></li></ol></li>
  <li><a href="text/chapter%202.xhtml">Chapter Two</a></li>
  <li><span>Part Two</span>
    <ol><li><a href="text/chapter3.xhtml#c3">Chapter Three</a></li></ol></li>
  <li><a href="http://example.com/extra">Online extra</a></li>
</ol></nav>"#,
    );
    let cover = xhtml(
        "Cover",
        r#"<div class="cover"><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 600 800"><image width="600" height="800" href="../images/cover.png"/></svg></div>"#,
    );
    let ch1 = xhtml(
        "The Lighthouse at Low Tide",
        r#"<section epub:type="chapter" id="ch1">
  <h1>Chapter One</h1>
  <p class="first">
     The   keeper&rsquo;s boat came in on the
     <em>morning</em> tide&mdash;late, as&nbsp;always.
  </p>
  <p>&ldquo;You&rsquo;re the new one,&rdquo; she said.<span class="pagebreak" id="page2"/></p>
  <h2 id="storm">The Storm</h2>
  <p>Wind first.<br/>
     Then rain.</p>
  <blockquote><p>Keep the lamp lit.</p></blockquote>
  <p>See the <a href="notes.xhtml#n1">note</a><sup>1</sup>, the <a href="chapter%202.xhtml#second">next chapter</a>
     and the <a href="https://example.com/lighthouses?x=1">society page</a>.</p>
  <hr/>
  <ul><li>oil</li><li>wick</li></ul>
</section>"#,
    );
    let ch2 = xhtml(
        "Chapter Two",
        r#"<h1 id="second">Chapter Two</h1>
<p>Second chapter, first paragraph.</p>
<p><img src="../images/cover.png" alt="A sketch of the lamp room"/></p>
<p>Caf&#233; na&iuml;ve &amp; co.</p>"#,
    );
    let ch3 = xhtml(
        "Chapter Three",
        r#"<h1 id="c3">Chapter Three</h1>
<script type="text/javascript">document.title = "pwned"; fetch("http://tracker.example/open");</script>
<p onclick="window.location='http://tracker.example/'">The last chapter.</p>
<img src="http://tracker.example/pixel.gif" alt=""/>
<iframe src="http://tracker.example/frame"></iframe>
<p style="display:none">invisible ink</p>
<p>The end.</p>"#,
    );
    let notes = xhtml(
        "Notes",
        r#"<p id="n1">1. A note outside the reading line.</p>"#,
    );
    zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/nav.xhtml", nav.as_bytes()),
        ("OEBPS/text/style.css", b"p { margin: 0 }"),
        ("OEBPS/text/cover.xhtml", cover.as_bytes()),
        ("OEBPS/text/chapter1.xhtml", ch1.as_bytes()),
        ("OEBPS/text/chapter 2.xhtml", ch2.as_bytes()),
        ("OEBPS/text/chapter3.xhtml", ch3.as_bytes()),
        ("OEBPS/text/notes.xhtml", notes.as_bytes()),
        (
            "OEBPS/images/cover.png",
            b"\x89PNG\r\n\x1a\nnot really a picture",
        ),
    ])
}

/// An EPUB 2 book: NCX contents, `opf:` attributes, the package at the
/// container root, a Windows-1252 chapter and loosely written HTML.
pub fn epub2() -> Vec<u8> {
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="BookId">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>Old Roads</dc:title>
    <dc:creator opf:role="aut" opf:file-as="Marsh, Edith">Edith Marsh</dc:creator>
    <dc:creator opf:role="trl">Jon Vale</dc:creator>
    <dc:identifier id="BookId" opf:scheme="UUID">urn:uuid:11111111-2222-4333-8444-555555555555</dc:identifier>
    <dc:identifier opf:scheme="ISBN">978-0-00-000002-6</dc:identifier>
    <dc:language>en-GB</dc:language>
    <meta name="cover" content="cover-image"/>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="one" href="One.html" media-type="application/xhtml+xml"/>
    <item id="two" href="two.html" media-type="application/xhtml+xml"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="one"/>
    <itemref idref="two"/>
  </spine>
  <guide><reference type="text" href="One.html"/></guide>
</package>"#;
    let ncx = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE ncx PUBLIC "-//NISO//DTD ncx 2005-1//EN" "http://www.daisy.org/z3986/2005/ncx-2005-1.dtd">
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="x"/></head>
  <docTitle><text>Old Roads</text></docTitle>
  <navMap>
    <navPoint id="p1" playOrder="1"><navLabel><text>The North Road</text></navLabel><content src="One.html"/>
      <navPoint id="p2" playOrder="2"><navLabel><text>  A Detour </text></navLabel><content src="One.html#detour"/></navPoint>
    </navPoint>
    <navPoint id="p3" playOrder="3"><navLabel><text>The South Road</text></navLabel><content src="two.html"/></navPoint>
  </navMap>
</ncx>"#;
    // The manifest says "One.html"; the archive says "one.html". HTML-style
    // markup: unclosed <p>, <br> without a slash, uppercase tags.
    let one = b"<?xml version=\"1.0\" encoding=\"windows-1252\"?>\n<html><head><title>One</title></head><body>\n<H2>The North Road</H2>\n<P>It was \x93fine\x94 weather \x97 for March.\n<P>Second paragraph<BR>with a break.\n<h3 id=detour>A Detour</h3>\n<p>Fish &amp; chips, 5&nbsp;miles on.</p>\n</body></html>".to_vec();
    let two = xhtml("Two", "<h2>The South Road</h2><p>Home again.</p>");
    zip(&[
        ("mimetype", b"application/epub+zip"),
        (
            "META-INF/container.xml",
            CONTAINER
                .replace("OEBPS/content.opf", "content.opf")
                .as_bytes(),
        ),
        ("content.opf", opf.as_bytes()),
        ("toc.ncx", ncx.as_bytes()),
        ("one.html", &one),
        ("two.html", two.as_bytes()),
    ])
}

/// The smallest valid book with the given chapter bodies and extra files.
pub fn simple(chapters: &[&str], extra: &[(&str, &[u8])]) -> Vec<u8> {
    let mut manifest = String::new();
    let mut spine = String::new();
    for i in 0..chapters.len() {
        manifest.push_str(&format!(
            r#"<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{i}"/>"#));
    }
    let opf = format!(
        r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">simple-1</dc:identifier><dc:title>Simple</dc:title><dc:language>en</dc:language></metadata>
<manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
    );
    let docs: Vec<(String, String)> = chapters
        .iter()
        .enumerate()
        .map(|(i, b)| (format!("OEBPS/c{i}.xhtml"), xhtml("c", b)))
        .collect();
    let mut files: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
    ];
    for (name, doc) in &docs {
        files.push((name, doc.as_bytes()));
    }
    files.extend_from_slice(extra);
    zip(&files)
}

pub fn write(scratch: &Scratch, name: &str, bytes: &[u8]) -> PathBuf {
    scratch.file(name, bytes)
}

pub fn exists(path: &Path) -> bool {
    path.exists()
}

/// The parts of a generated EPUB that the survey and metadata tests vary.
/// Chapters become `OEBPS/c<i>.xhtml`, linear unless listed in `nonlinear`.
#[derive(Default)]
pub struct Parts<'a> {
    pub version: &'a str,
    pub metadata: &'a str,
    pub manifest: &'a str,
    pub spine_attrs: &'a str,
    pub spine_extra: &'a str,
    pub chapters: &'a [&'a str],
    pub nonlinear: &'a [usize],
    pub files: &'a [(&'a str, &'a [u8])],
}

pub fn package(p: Parts) -> Vec<u8> {
    let mut manifest = String::from(p.manifest);
    let mut spine = String::new();
    for i in 0..p.chapters.len() {
        manifest.push_str(&format!(
            r#"<item id="c{i}" href="c{i}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        let linear = if p.nonlinear.contains(&i) {
            r#" linear="no""#
        } else {
            ""
        };
        spine.push_str(&format!(r#"<itemref idref="c{i}"{linear}/>"#));
    }
    spine.push_str(p.spine_extra);
    let version = if p.version.is_empty() {
        "3.0"
    } else {
        p.version
    };
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="{version}" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
<dc:identifier id="id">parts-1</dc:identifier><dc:title>Parts</dc:title><dc:language>en</dc:language>
{}
</metadata>
<manifest>{manifest}</manifest><spine{}>{spine}</spine></package>"#,
        p.metadata, p.spine_attrs
    );
    let docs: Vec<(String, String)> = p
        .chapters
        .iter()
        .enumerate()
        .map(|(i, b)| (format!("OEBPS/c{i}.xhtml"), xhtml("c", b)))
        .collect();
    let mut files: Vec<(&str, &[u8])> = vec![
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", CONTAINER.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
    ];
    for (name, doc) in &docs {
        files.push((name, doc.as_bytes()));
    }
    files.extend_from_slice(p.files);
    zip(&files)
}

/// An EPUB 3 book with one chapter of `text` and `image` as its declared
/// cover, stored under `name` with the given media type.
pub fn with_cover(image: &[u8], name: &str, media_type: &str, spine_attrs: &str) -> Vec<u8> {
    let manifest = format!(
        r#"<item id="cov" href="images/{name}" media-type="{media_type}" properties="cover-image"/>"#
    );
    let path = format!("OEBPS/images/{name}");
    package(Parts {
        manifest: &manifest,
        spine_attrs,
        chapters: &["<p>Three short words.</p>"],
        files: &[(&path, image)],
        ..Parts::default()
    })
}

pub const RED: [u8; 3] = [200, 30, 30];
pub const GREEN: [u8; 3] = [30, 160, 60];
pub const BLUE: [u8; 3] = [30, 30, 200];

/// A cover with a red tenth on the left, a blue tenth on the right and
/// green between, encoded in `format`.
pub fn striped(width: u32, height: u32, format: image::ImageFormat) -> Vec<u8> {
    picture(width, height, format, |x, _| {
        let c = if x < width / 10 {
            RED
        } else if x >= width - width / 10 {
            BLUE
        } else {
            GREEN
        };
        [c[0], c[1], c[2], 255]
    })
}

/// Encode a picture painted pixel by pixel as RGBA. JPEG drops the alpha.
pub fn picture(
    width: u32,
    height: u32,
    format: image::ImageFormat,
    paint: impl Fn(u32, u32) -> [u8; 4],
) -> Vec<u8> {
    let rgba = image::RgbaImage::from_fn(width, height, |x, y| image::Rgba(paint(x, y)));
    let img = if format == image::ImageFormat::Jpeg {
        image::DynamicImage::ImageRgb8(image::DynamicImage::ImageRgba8(rgba).to_rgb8())
    } else {
        image::DynamicImage::ImageRgba8(rgba)
    };
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, format).unwrap();
    out.into_inner()
}

/// Channels within `tolerance` of each other, for lossy formats.
pub fn near(actual: ebook_reader::Rgb, expected: [u8; 3], tolerance: u8) -> bool {
    [actual.r, actual.g, actual.b]
        .iter()
        .zip(expected)
        .all(|(a, e)| a.abs_diff(e) <= tolerance)
}

/// Write one helper reply to a file, framed as the helper frames it.
pub fn frame(
    dir: &Scratch,
    name: &str,
    response: &ebook_reader::wire::Response,
    blob: &[u8],
) -> PathBuf {
    let mut bytes = Vec::new();
    ebook_reader::wire::write_frame(&mut bytes, response, blob).unwrap();
    dir.file(name, &bytes)
}

/// The shell command that sends a framed reply from a file.
pub fn send(path: &Path) -> String {
    format!("cat '{}'", path.display())
}

/// A stand-in for the whole sandbox. It says the sandbox holds, then runs
/// the first command whose `case` pattern matches each request line.
pub fn stand_in(dir: &Scratch, name: &str, answers: &[(&str, String)]) -> PathBuf {
    let mut script = String::from(
        "#!/bin/sh\necho '0 {\"Ok\":{\"Probe\":{\"isolated\":true}}}'\nwhile read -r request; do\ncase \"$request\" in\n",
    );
    for (pattern, command) in answers {
        script.push_str(&format!("{pattern}) {command};;\n"));
    }
    script.push_str("esac\ndone\n");
    let path = dir.file(name, script.as_bytes());
    std::process::Command::new("/bin/chmod")
        .arg("+x")
        .arg(&path)
        .status()
        .unwrap();
    path
}
