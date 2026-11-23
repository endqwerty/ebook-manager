//! Time what reading a book asks of the sandboxed helper: starting the
//! reader, opening a book, each chapter's markup and each picture. The book
//! is generated, so every run reads the same bytes.
//!
//! "start to first page" runs from creating the reader to having the first
//! chapter's markup and its first picture, as the reader window needs them.
//!
//! Usage: reader-bench <helper> [rounds]

use ebook_reader::{Reader, ReaderConfig, ResourceId};
use std::io::{Cursor, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::{Duration, Instant};
use zip::write::SimpleFileOptions;
use zip::CompressionMethod;

const CHAPTERS: u32 = 24;
const PICTURES: u32 = 24;
const PARAGRAPHS: usize = 60;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let Some(helper) = args.get(1) else {
        eprintln!("usage: reader-bench <helper> [rounds]");
        return ExitCode::from(2);
    };
    let rounds: usize = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(3);
    let dir = std::env::temp_dir().join(format!("reader-bench-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bench.epub");
    std::fs::write(&path, book()).unwrap();
    let config = || ReaderConfig {
        helper_path: Some(PathBuf::from(helper)),
        ..ReaderConfig::default()
    };

    let mut start = Vec::new();
    let mut open = Vec::new();
    let mut chapter = Vec::new();
    let mut picture = Vec::new();
    let mut first_page = Vec::new();
    let mut markup_bytes = 0usize;
    let mut picture_bytes = 0usize;
    for _ in 0..rounds {
        let began = Instant::now();
        let reader = Reader::new(config()).expect("sandbox");
        start.push(began.elapsed());
        let t0 = Instant::now();
        let book = reader.open(&path).expect("open");
        open.push(t0.elapsed());
        markup_bytes = 0;
        picture_bytes = 0;
        for index in 0..book.chapters().len() as u32 {
            let t = Instant::now();
            let content = book.read_content(index).expect("content");
            chapter.push(t.elapsed());
            markup_bytes += content.markup.len();
            if index == 0 {
                let t = Instant::now();
                let r = book
                    .read_resource(ResourceId(first_picture(&content.markup)))
                    .expect("picture");
                picture.push(t.elapsed());
                picture_bytes += r.bytes.len();
                first_page.push(began.elapsed());
            }
        }
        for id in 0..PICTURES {
            let t = Instant::now();
            if let Ok(r) = book.read_resource(ResourceId(id)) {
                picture.push(t.elapsed());
                picture_bytes += r.bytes.len();
            }
        }
    }
    let reader = Reader::new(config()).expect("sandbox");
    let mut reopen = Vec::new();
    for _ in 0..20 * rounds {
        let t = Instant::now();
        drop(reader.open(&path).expect("open"));
        reopen.push(t.elapsed());
    }
    report("reader start", &start);
    report("open again", &reopen);
    report("open book", &open);
    report("chapter markup", &chapter);
    report("picture", &picture);
    report("start to first page", &first_page);
    println!("markup bytes per read-through: {markup_bytes}");
    println!("picture bytes per read-through: {picture_bytes}");
    let _ = std::fs::remove_dir_all(&dir);
    ExitCode::SUCCESS
}

fn first_picture(markup: &str) -> u32 {
    markup
        .split("/r/")
        .nth(1)
        .and_then(|s| s.split(|c: char| !c.is_ascii_digit()).next())
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

fn report(what: &str, samples: &[Duration]) {
    let mut ms: Vec<f64> = samples.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(f64::total_cmp);
    if ms.is_empty() {
        println!("{what}: no samples");
        return;
    }
    let at = |q: f64| ms[((ms.len() - 1) as f64 * q).round() as usize];
    println!(
        "{what}: n={} median {:.2} ms, p95 {:.2} ms, max {:.2} ms",
        ms.len(),
        at(0.5),
        at(0.95),
        at(1.0)
    );
}

fn book() -> Vec<u8> {
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("mimetype".into(), b"application/epub+zip".to_vec()),
        (
            "META-INF/container.xml".into(),
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#
                .to_vec(),
        ),
    ];
    for p in 0..PICTURES {
        manifest.push_str(&format!(
            r#"<item id="p{p}" href="images/p{p}.jpg" media-type="image/jpeg"/>"#
        ));
        files.push((format!("OEBPS/images/p{p}.jpg"), jpeg(p)));
    }
    for c in 0..CHAPTERS {
        manifest.push_str(&format!(
            r#"<item id="c{c}" href="text/c{c}.xhtml" media-type="application/xhtml+xml"/>"#
        ));
        spine.push_str(&format!(r#"<itemref idref="c{c}"/>"#));
        let mut body = format!(
            "<h1>Chapter {c}</h1><p><img src=\"../images/p{}.jpg\" alt=\"Plate {c}\"/></p>",
            c % PICTURES
        );
        for n in 0..PARAGRAPHS {
            body.push_str(&format!(
                "<p>Paragraph {n} of chapter {c}. The tide went out across the flats and the \
                 lighthouse keeper counted the <em>gulls</em> twice, because the first count \
                 never agreed with the ledger he kept beside the lamp.</p>"
            ));
        }
        files.push((
            format!("OEBPS/text/c{c}.xhtml"),
            format!(
                r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><head><title>Chapter {c}</title></head><body>{body}</body></html>"#
            )
            .into_bytes(),
        ));
    }
    manifest.push_str(
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    );
    let mut nav = String::new();
    for c in 0..CHAPTERS {
        nav.push_str(&format!(
            r#"<li><a href="text/c{c}.xhtml">Chapter {c}</a></li>"#
        ));
    }
    files.push((
        "OEBPS/nav.xhtml".into(),
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol>{nav}</ol></nav></body></html>"#
        )
        .into_bytes(),
    ));
    files.push((
        "OEBPS/content.opf".into(),
        format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:00000000-0000-4000-8000-000000000001</dc:identifier><dc:title>Bench</dc:title><dc:language>en</dc:language></metadata>
<manifest>{manifest}</manifest><spine>{spine}</spine></package>"#
        )
        .into_bytes(),
    ));
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data) in &files {
        let method = if name == "mimetype" || name.ends_with(".jpg") {
            CompressionMethod::Stored
        } else {
            CompressionMethod::Deflated
        };
        w.start_file(
            name.as_str(),
            SimpleFileOptions::default().compression_method(method),
        )
        .unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// A noisy 900×1200 picture, so it compresses about as badly as a photograph.
fn jpeg(seed: u32) -> Vec<u8> {
    let (w, h) = (900u32, 1200u32);
    let mut state = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    let img = image::RgbImage::from_fn(w, h, |x, y| {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        let n = (state & 0x3f) as u8;
        image::Rgb([(x / 4) as u8 ^ n, (y / 5) as u8 ^ n, n.wrapping_mul(3)])
    });
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
    out.into_inner()
}
