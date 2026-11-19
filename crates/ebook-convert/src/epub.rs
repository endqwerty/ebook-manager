use crate::book::*;
use ebook_convert::{Error, Metadata, OUTPUT_LIMIT};
use std::fmt::Write as _;
use std::io::{Cursor, Write};
use zip::{write::SimpleFileOptions, ZipWriter};

fn xml(value: &str) -> Result<String, Error> {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\t'
            | '\n'
            | '\r'
            | ' '..='\u{d7ff}'
            | '\u{e000}'..='\u{fffd}'
            | '\u{10000}'..='\u{10ffff}' => escaped.push(ch),
            _ => {
                return Err(Error::Invalid(
                    "text contains invalid XML characters".into(),
                ))
            }
        }
        if escaped.len() > 16 << 20 {
            return Err(Error::Limit);
        }
    }
    Ok(escaped)
}

fn page(title: &str, body: &str, lang: &str) -> String {
    format!(
        r#"<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops" xml:lang="{lang}" lang="{lang}"><head><title>{title}</title></head><body>{body}</body></html>"#
    )
}
struct Archive {
    zip: ZipWriter<Cursor<Vec<u8>>>,
    content_bytes: u64,
}
impl Archive {
    fn add(&mut self, name: &str, data: impl AsRef<[u8]>) -> Result<(), Error> {
        self.content_bytes += data.as_ref().len() as u64;
        if self.content_bytes > OUTPUT_LIMIT {
            return Err(Error::Limit);
        }
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        self.zip
            .start_file(name, options)
            .map_err(std::io::Error::other)?;
        self.zip.write_all(data.as_ref())?;
        Ok(())
    }
}
fn image_type(kind: ImageKind) -> (&'static str, &'static str) {
    match kind {
        ImageKind::Jpeg => ("jpg", "image/jpeg"),
        ImageKind::Png => ("png", "image/png"),
    }
}

pub fn write(
    mut book: ParsedBook,
    metadata: Metadata,
    cover: Option<Image>,
) -> Result<(Vec<String>, Vec<u8>), Error> {
    if let Some(image) = cover {
        book.cover = Some(book.images.len());
        book.images.push(image);
    }
    if book.sections.is_empty() || book.sections.len() > 512 || book.images.len() > 4096 {
        return Err(Error::Limit);
    }
    if book.cover.is_some_and(|i| i >= book.images.len()) {
        return Err(Error::Invalid("missing cover image".into()));
    }
    let mut zip = Archive {
        zip: ZipWriter::new(Cursor::new(Vec::new())),
        content_bytes: 0,
    };
    zip.add("mimetype", "application/epub+zip")?;
    zip.add("META-INF/container.xml", r#"<container xmlns="urn:oasis:names:tc:opendocument:xmlns:container" version="1.0"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#)?;
    let mut manifest = String::from(
        r#"<item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>"#,
    );
    let mut spine = String::new();
    let mut toc = String::new();
    let title = xml(metadata.title.as_deref().unwrap_or(&book.title).trim())?;
    if title.is_empty() {
        return Err(Error::Invalid("missing title".into()));
    }
    let lang = metadata
        .language
        .or(book.language)
        .unwrap_or_else(|| "und".into());
    if lang.parse::<language_tags::LanguageTag>().is_err() {
        return Err(Error::Invalid("invalid publication language".into()));
    }
    for (i, image) in book.images.iter().enumerate() {
        let (ext, mime) = image_type(image.kind);
        let signature: &[u8] = match image.kind {
            ImageKind::Png => b"\x89PNG\r\n\x1a\n",
            ImageKind::Jpeg => b"\xff\xd8\xff",
        };
        if !image.bytes.starts_with(signature) {
            return Err(Error::Invalid("picture has the wrong media type".into()));
        }
        let attr = if Some(i) == book.cover {
            r#" properties="cover-image""#
        } else {
            ""
        };
        write!(
            manifest,
            r#"<item id="im{i}" href="im{i}.{ext}" media-type="{mime}"{attr}/>"#
        )
        .unwrap();
        zip.add(&format!("im{i}.{ext}"), &image.bytes)?;
    }
    for (i, section) in book.sections.iter().enumerate() {
        let heading = if section.title.trim().is_empty() {
            format!("Section {}", i + 1)
        } else {
            xml(&section.title)?
        };
        let mut body = String::new();
        for block in &section.blocks {
            match block {
                Block::Heading(s) => write!(body, "<h2>{}</h2>", xml(s)?).unwrap(),
                Block::Paragraph(s) => write!(body, "<p>{}</p>", xml(s)?).unwrap(),
                Block::Image(n) => {
                    let image = book
                        .images
                        .get(*n)
                        .ok_or_else(|| Error::Invalid("missing referenced picture".into()))?;
                    let (ext, _) = image_type(image.kind);
                    write!(body, r#"<p><img alt="" src="im{n}.{ext}"/></p>"#).unwrap();
                }
            }
            if body.len() > 16 << 20 {
                return Err(Error::Limit);
            }
        }
        zip.add(&format!("s{i}.xhtml"), page(&heading, &body, &lang))?;
        write!(toc, r#"<li><a href="s{i}.xhtml">{heading}</a></li>"#).unwrap();
        write!(
            manifest,
            r#"<item id="s{i}" href="s{i}.xhtml" media-type="application/xhtml+xml"/>"#
        )
        .unwrap();
        write!(spine, r#"<itemref idref="s{i}"/>"#).unwrap();
    }
    let mut creators = String::new();
    for author in metadata.authors.unwrap_or(book.authors) {
        if !author.trim().is_empty() {
            write!(creators, "<dc:creator>{}</dc:creator>", xml(&author)?).unwrap();
        }
    }
    let id = uuid::Uuid::new_v4();
    let modified = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    zip.add(
        "nav.xhtml",
        page(
            "Contents",
            &format!(r#"<nav epub:type="toc"><h1>Contents</h1><ol>{toc}</ol></nav>"#),
            &lang,
        ),
    )?;
    zip.add("book.opf", format!(r#"<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:{id}</dc:identifier><dc:title>{title}</dc:title><dc:language>{lang}</dc:language>{creators}<meta property="dcterms:modified">{modified}</meta></metadata><manifest>{manifest}</manifest><spine>{spine}</spine></package>"#))?;
    let bytes = zip
        .zip
        .finish()
        .map_err(std::io::Error::other)?
        .into_inner();
    if bytes.len() as u64 > OUTPUT_LIMIT {
        return Err(Error::Limit);
    }
    Ok((book.losses, bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    fn book() -> ParsedBook {
        ParsedBook {
            title: "Original".into(),
            authors: vec!["Source Author".into()],
            language: Some("en".into()),
            sections: vec![Section {
                title: "A & B".into(),
                blocks: vec![Block::Paragraph("<img> & fish".into())],
            }],
            ..Default::default()
        }
    }
    #[test]
    fn e2_catalogue_values_and_literal_text() {
        let settings = Metadata {
            title: Some("Latest & Best".into()),
            authors: Some(vec!["Ada".into()]),
            language: Some("fr".into()),
        };
        let (_, bytes) = write(book(), settings, None).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut read = |name: &str| {
            let mut s = String::new();
            archive
                .by_name(name)
                .unwrap()
                .read_to_string(&mut s)
                .unwrap();
            s
        };
        let package = read("book.opf");
        assert!(
            package.contains("<dc:title>Latest &amp; Best</dc:title>")
                && package.contains("<dc:creator>Ada</dc:creator>")
                && package.contains("<dc:language>fr</dc:language>")
        );
        assert!(read("s0.xhtml").contains("<p>&lt;img&gt; &amp; fish</p>"));
        assert!(read("nav.xhtml").contains("A &amp; B"));
    }
    #[test]
    fn e2_refuses_an_unresolved_image() {
        let mut parsed = book();
        parsed.sections[0].blocks.push(Block::Image(12));
        assert!(matches!(
            write(parsed, Metadata::default(), None),
            Err(Error::Invalid(_))
        ));
    }
}
