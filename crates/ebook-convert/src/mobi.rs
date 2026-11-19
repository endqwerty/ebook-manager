//! Decodes an unprotected MOBI 7 book (a Palm database of type `BOOK`, creator
//! `MOBI`) into a [`ParsedBook`]. The supported subset and its sources are listed
//! in the crate README under "MOBI decoder".

use crate::book::{Block, DecodeError, Image, ImageKind, ParsedBook, Section};
use std::collections::HashMap;

pub const MAX_INPUT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = 64 * 1024 * 1024;
const TEXT_RECORD_SIZE: usize = 4096;
const ABSENT: u32 = 0xFFFF_FFFF;

const COMPRESSION_NONE: u16 = 1;
const COMPRESSION_PALMDOC: u16 = 2;
const COMPRESSION_HUFF_CDIC: u16 = 17480;

const ENCODING_CP1252: u32 = 1252;
const ENCODING_UTF8: u32 = 65001;

const EXTH_AUTHOR: u32 = 100;
const EXTH_KF8_BOUNDARY: u32 = 121;
const EXTH_COVER_OFFSET: u32 = 201;
const EXTH_LANGUAGE: u32 = 524;

const LOSS_FORMATTING: &str = "Inline formatting such as bold, italic and font sizes was removed.";
const LOSS_LINKS: &str = "Links were removed; their text was kept.";
const LOSS_TABLES: &str = "Tables were flattened into paragraphs.";
const LOSS_LISTS: &str = "Lists were flattened into paragraphs.";
const LOSS_UNKNOWN_MARKUP: &str = "Unrecognised markup was removed; its text was kept.";
const LOSS_GIF: &str = "GIF images were dropped; only JPEG and PNG images are kept.";
const LOSS_OTHER_IMAGE: &str = "Images in formats other than JPEG and PNG were dropped.";
const LOSS_MISSING_IMAGE: &str = "References to missing image records were dropped.";
const LOSS_UNINDEXED_IMAGE: &str =
    "Images referenced by name instead of record number were dropped.";
const LOSS_BAD_CHARACTERS: &str = "Byte sequences that are not valid text were replaced.";
const LOSS_SHORT_TEXT: &str =
    "The text was shorter than its declared length; the end may be missing.";
const LOSS_TRUNCATED_MARKUP: &str = "Markup cut off at the end of the text was removed.";

pub fn decode(bytes: &[u8]) -> Result<ParsedBook, DecodeError> {
    if bytes.len() > MAX_INPUT_BYTES {
        return Err(DecodeError::Limit);
    }
    let db = PalmDatabase::parse(bytes)?;
    let header = Header::parse(&db)?;
    let mut losses = Losses::default();

    let raw = read_text(&db, &header, &mut losses)?;
    let text = decode_characters(&raw, header.encoding, &mut losses);

    let mut images = ImageTable::new(&db, header.first_image);
    let sections = build_sections(&text, &mut images, &mut losses);

    let cover = header
        .cover_offset
        .and_then(|offset| images.cover(offset, &mut losses));

    if sections.is_empty() {
        return Err(DecodeError::Invalid("the book has no readable text".into()));
    }

    Ok(ParsedBook {
        title: header.title,
        authors: header.authors,
        language: header.language,
        sections,
        images: images.kept,
        cover,
        losses: losses.0,
    })
}

#[derive(Default)]
struct Losses(Vec<String>);

impl Losses {
    fn note(&mut self, message: &str) {
        if !self.0.iter().any(|m| m == message) {
            self.0.push(message.to_string());
        }
    }
}

struct PalmDatabase<'a> {
    bytes: &'a [u8],
    name: &'a [u8],
    offsets: Vec<usize>,
}

impl<'a> PalmDatabase<'a> {
    fn parse(bytes: &'a [u8]) -> Result<Self, DecodeError> {
        const LIST_START: usize = 78;
        if bytes.len() < LIST_START {
            return Err(invalid("file is shorter than a Palm database header"));
        }
        let kind = &bytes[60..68];
        if kind != b"BOOKMOBI" {
            return Err(match kind {
                b"TEXtREAd" => {
                    DecodeError::Unsupported("PalmDOC text without a MOBI header".into())
                }
                _ if &kind[4..] == b"MOBI" => {
                    DecodeError::Unsupported("Mobipocket database that is not a book".into())
                }
                _ => invalid("not a MOBI book database"),
            });
        }
        let count = be16(bytes, 76).unwrap_or(0) as usize;
        if count == 0 {
            return Err(invalid("database has no records"));
        }
        let list_end = LIST_START + count * 8;
        if list_end > bytes.len() {
            return Err(invalid("record list runs past the end of the file"));
        }
        let mut offsets = Vec::with_capacity(count);
        let mut previous = list_end;
        for i in 0..count {
            let offset = be32(bytes, LIST_START + i * 8).unwrap_or(ABSENT) as usize;
            if offset < previous || offset > bytes.len() {
                return Err(invalid("record offsets are out of order or out of range"));
            }
            offsets.push(offset);
            previous = offset;
        }
        let name_field = &bytes[..32];
        let name_len = name_field.iter().position(|&b| b == 0).unwrap_or(32);
        Ok(PalmDatabase {
            bytes,
            name: &name_field[..name_len],
            offsets,
        })
    }

    fn len(&self) -> usize {
        self.offsets.len()
    }

    fn record(&self, index: usize) -> Option<&'a [u8]> {
        let start = *self.offsets.get(index)?;
        let end = self
            .offsets
            .get(index + 1)
            .copied()
            .unwrap_or(self.bytes.len());
        Some(&self.bytes[start..end])
    }
}

struct Header {
    compression: u16,
    text_length: usize,
    text_records: usize,
    encoding: u32,
    extra_flags: u32,
    first_image: Option<usize>,
    cover_offset: Option<usize>,
    title: String,
    authors: Vec<String>,
    language: Option<String>,
}

impl Header {
    fn parse(db: &PalmDatabase<'_>) -> Result<Self, DecodeError> {
        let r0 = db
            .record(0)
            .ok_or_else(|| invalid("missing header record"))?;
        if r0.len() < 16 {
            return Err(invalid("header record is too short"));
        }
        if be16(r0, 12) != Some(0) {
            return Err(DecodeError::Protected);
        }
        if r0.len() < 24 || &r0[16..20] != b"MOBI" {
            return Err(invalid("header record has no MOBI header"));
        }
        let header_length = be32(r0, 20).unwrap_or(0) as usize;
        let header_end = 16usize.saturating_add(header_length);
        if header_length < 8 || header_end > r0.len() {
            return Err(invalid("MOBI header length is out of range"));
        }
        let field = |offset: usize| -> Option<u32> {
            if offset + 4 <= header_end {
                be32(r0, offset)
            } else {
                None
            }
        };

        let drm_offset = field(168).unwrap_or(ABSENT);
        let drm_count = field(172).unwrap_or(ABSENT);
        if drm_offset != ABSENT && drm_count != ABSENT && drm_count != 0 {
            return Err(DecodeError::Protected);
        }

        let compression = be16(r0, 0).unwrap_or(0);
        match compression {
            COMPRESSION_NONE | COMPRESSION_PALMDOC => {}
            COMPRESSION_HUFF_CDIC => {
                return Err(DecodeError::Unsupported("HUFF/CDIC compression".into()))
            }
            other => {
                return Err(DecodeError::Unsupported(format!(
                    "compression type {other}"
                )))
            }
        }

        let mobi_type = field(24).unwrap_or(0);
        let version = field(36).unwrap_or(0);
        if mobi_type == 248 || version >= 8 {
            return Err(DecodeError::Unsupported("KF8 (AZW3) book".into()));
        }
        if mobi_type != 2 && mobi_type != 232 {
            return Err(DecodeError::Unsupported(format!("MOBI type {mobi_type}")));
        }

        let encoding = field(28).unwrap_or(0);
        if encoding != ENCODING_CP1252 && encoding != ENCODING_UTF8 {
            return Err(DecodeError::Unsupported(format!(
                "text encoding {encoding}"
            )));
        }

        let record_size = be16(r0, 10).unwrap_or(0) as usize;
        if record_size != TEXT_RECORD_SIZE {
            return Err(DecodeError::Unsupported(format!(
                "text record size {record_size}"
            )));
        }
        let text_length = be32(r0, 4).unwrap_or(0) as usize;
        if text_length > MAX_TEXT_BYTES {
            return Err(DecodeError::Limit);
        }
        let text_records = be16(r0, 8).unwrap_or(0) as usize;
        if text_records == 0 || text_records >= db.len() {
            return Err(invalid("text record count is out of range"));
        }
        if text_length > text_records * TEXT_RECORD_SIZE {
            return Err(invalid("declared text length exceeds its records"));
        }

        if db
            .offsets
            .iter()
            .skip(1)
            .any(|&o| db.bytes[o..].starts_with(b"BOUNDARY"))
        {
            return Err(DecodeError::Unsupported(
                "combined MOBI 7 and KF8 book".into(),
            ));
        }

        let extra_flags = field(240).unwrap_or(0);
        let first_image = field(108)
            .filter(|&v| v != ABSENT && v != 0)
            .map(|v| v as usize);

        let mut header = Header {
            compression,
            text_length,
            text_records,
            encoding,
            extra_flags,
            first_image,
            cover_offset: None,
            title: String::new(),
            authors: Vec::new(),
            language: None,
        };

        let exth_flags = field(128).unwrap_or(0);
        if exth_flags & 0x40 != 0 {
            header.read_exth(&r0[header_end..])?;
        }

        let name_offset = field(84).unwrap_or(0) as usize;
        let name_length = field(88).unwrap_or(0) as usize;
        if name_length > 0 {
            let name = name_offset
                .checked_add(name_length)
                .and_then(|end| r0.get(name_offset..end))
                .ok_or_else(|| invalid("full name runs past the header record"))?;
            header.title = decode_metadata(name, encoding);
        }
        if header.title.is_empty() {
            header.title = decode_metadata(db.name, ENCODING_CP1252);
        }
        if header.language.is_none() && field(92).map(|locale| locale & 0xFF) == Some(0x09) {
            header.language = Some("en".into());
        }
        Ok(header)
    }

    fn read_exth(&mut self, region: &[u8]) -> Result<(), DecodeError> {
        if region.len() < 12 || &region[..4] != b"EXTH" {
            return Err(invalid("EXTH header is missing"));
        }
        let length = be32(region, 4).unwrap_or(0) as usize;
        if length < 12 || length > region.len() {
            return Err(invalid("EXTH length is out of range"));
        }
        let count = be32(region, 8).unwrap_or(0) as usize;
        let mut at = 12;
        for _ in 0..count {
            let (kind, size) = match (be32(region, at), be32(region, at + 4)) {
                (Some(k), Some(s)) => (k, s as usize),
                _ => return Err(invalid("EXTH record runs past the EXTH header")),
            };
            let end = at.checked_add(size).filter(|&e| size >= 8 && e <= length);
            let end = end.ok_or_else(|| invalid("EXTH record runs past the EXTH header"))?;
            let data = &region[at + 8..end];
            match kind {
                EXTH_AUTHOR => {
                    let author = decode_metadata(data, self.encoding);
                    if !author.is_empty() {
                        self.authors.push(author);
                    }
                }
                EXTH_LANGUAGE => {
                    let language = decode_metadata(data, self.encoding);
                    if !language.is_empty() {
                        self.language = Some(language);
                    }
                }
                EXTH_COVER_OFFSET if data.len() == 4 => {
                    let offset = be32(data, 0).unwrap_or(ABSENT);
                    if offset != ABSENT {
                        self.cover_offset = Some(offset as usize);
                    }
                }
                EXTH_KF8_BOUNDARY => {
                    return Err(DecodeError::Unsupported(
                        "combined MOBI 7 and KF8 book".into(),
                    ))
                }
                _ => {}
            }
            at = end;
        }
        Ok(())
    }
}

fn read_text(
    db: &PalmDatabase<'_>,
    header: &Header,
    losses: &mut Losses,
) -> Result<Vec<u8>, DecodeError> {
    let mut text = Vec::with_capacity(header.text_length);
    for index in 1..=header.text_records {
        let record = db
            .record(index)
            .ok_or_else(|| invalid("missing text record"))?;
        let content = strip_trailing_entries(record, header.extra_flags)?;
        let before = text.len();
        match header.compression {
            COMPRESSION_PALMDOC => palmdoc_decompress(content, &mut text)?,
            _ => {
                if content.len() > TEXT_RECORD_SIZE {
                    return Err(invalid("text record is larger than 4096 bytes"));
                }
                text.extend_from_slice(content);
            }
        }
        if text.len() - before > TEXT_RECORD_SIZE {
            return Err(invalid("text record expands past 4096 bytes"));
        }
        if text.len() > header.text_length {
            return Err(invalid("text is longer than its declared length"));
        }
    }
    if text.len() < header.text_length {
        losses.note(LOSS_SHORT_TEXT);
    }
    Ok(text)
}

/// Removes the trailing entries that the extra data flags say follow each text
/// record. Entries sit in flag-bit order, so the highest flag's entry is last in
/// the record and comes off first. Bit 0 (multibyte overlap) ends in a byte whose
/// low two bits count the overlapping bytes; that count plus the byte itself is
/// removed. Every other entry ends in a backward-encoded size that includes itself.
fn strip_trailing_entries(record: &[u8], flags: u32) -> Result<&[u8], DecodeError> {
    let mut end = record.len();
    for bit in (1..32).rev() {
        if flags & (1 << bit) == 0 {
            continue;
        }
        let size = backward_varint(&record[..end])?;
        if size == 0 || size > end {
            return Err(invalid("trailing entry is larger than its record"));
        }
        end -= size;
    }
    if flags & 1 != 0 {
        let last = *record[..end]
            .last()
            .ok_or_else(|| invalid("multibyte trailing entry is missing"))?;
        let size = (last & 0x3) as usize + 1;
        if size > end {
            return Err(invalid(
                "multibyte trailing entry is larger than its record",
            ));
        }
        end -= size;
    }
    Ok(&record[..end])
}

fn backward_varint(bytes: &[u8]) -> Result<usize, DecodeError> {
    let mut value = 0usize;
    for (taken, &byte) in bytes.iter().rev().take(4).enumerate() {
        value |= ((byte & 0x7F) as usize) << (7 * taken);
        if byte & 0x80 != 0 {
            return Ok(value);
        }
    }
    Err(invalid("trailing entry size is malformed"))
}

/// PalmDOC decompression. Output per record is capped at 4096 bytes, so a hostile
/// record cannot expand without bound.
fn palmdoc_decompress(input: &[u8], out: &mut Vec<u8>) -> Result<(), DecodeError> {
    let start = out.len();
    let limit = start + TEXT_RECORD_SIZE;
    let overflow = || invalid("text record expands past 4096 bytes");
    let mut i = 0;
    while i < input.len() {
        let byte = input[i];
        i += 1;
        match byte {
            0x00 | 0x09..=0x7F => out.push(byte),
            0x01..=0x08 => {
                let run = byte as usize;
                let literal = input
                    .get(i..i + run)
                    .ok_or_else(|| invalid("literal run passes the end of the record"))?;
                out.extend_from_slice(literal);
                i += run;
            }
            0x80..=0xBF => {
                let next = *input
                    .get(i)
                    .ok_or_else(|| invalid("back reference passes the end of the record"))?;
                i += 1;
                let pair = (((byte as usize) << 8) | next as usize) & 0x3FFF;
                let distance = pair >> 3;
                let length = (pair & 0x7) + 3;
                if distance == 0 || distance > out.len() - start {
                    return Err(invalid("back reference points before the record"));
                }
                if out.len() + length > limit {
                    return Err(overflow());
                }
                for _ in 0..length {
                    out.push(out[out.len() - distance]);
                }
            }
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(byte ^ 0x80);
            }
        }
        if out.len() > limit {
            return Err(overflow());
        }
    }
    Ok(())
}

fn decode_characters(raw: &[u8], encoding: u32, losses: &mut Losses) -> String {
    if encoding == ENCODING_UTF8 {
        match String::from_utf8_lossy(raw) {
            std::borrow::Cow::Borrowed(s) => s.to_string(),
            std::borrow::Cow::Owned(s) => {
                losses.note(LOSS_BAD_CHARACTERS);
                s
            }
        }
    } else {
        let mut replaced = false;
        let s = raw
            .iter()
            .map(|&b| {
                let c = cp1252(b);
                replaced |= c == '\u{FFFD}';
                c
            })
            .collect();
        if replaced {
            losses.note(LOSS_BAD_CHARACTERS);
        }
        s
    }
}

fn decode_metadata(bytes: &[u8], encoding: u32) -> String {
    let text = if encoding == ENCODING_UTF8 {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        bytes.iter().map(|&b| cp1252(b)).collect()
    };
    text.trim_matches(|c: char| c.is_whitespace() || c == '\0')
        .to_string()
}

fn cp1252(byte: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    match byte {
        0x80..=0x9F => HIGH[(byte - 0x80) as usize],
        _ => byte as char,
    }
}

struct ImageTable<'a, 'b> {
    db: &'b PalmDatabase<'a>,
    first_image: Option<usize>,
    by_record: HashMap<usize, Option<usize>>,
    kept: Vec<Image>,
}

impl<'a, 'b> ImageTable<'a, 'b> {
    fn new(db: &'b PalmDatabase<'a>, first_image: Option<usize>) -> Self {
        ImageTable {
            db,
            first_image,
            by_record: HashMap::new(),
            kept: Vec::new(),
        }
    }

    /// `recindex` attributes count from 1 at the first image record.
    fn by_recindex(&mut self, recindex: usize, losses: &mut Losses) -> Option<usize> {
        match recindex.checked_sub(1) {
            Some(offset) => self.by_offset(offset, losses),
            None => {
                losses.note(LOSS_MISSING_IMAGE);
                None
            }
        }
    }

    /// The EXTH cover value counts from 0 at the first image record.
    fn cover(&mut self, offset: usize, losses: &mut Losses) -> Option<usize> {
        self.by_offset(offset, losses)
    }

    fn by_offset(&mut self, offset: usize, losses: &mut Losses) -> Option<usize> {
        let record = self.first_image.and_then(|first| first.checked_add(offset));
        let record = match record {
            Some(r) if r < self.db.len() => r,
            _ => {
                losses.note(LOSS_MISSING_IMAGE);
                return None;
            }
        };
        if let Some(&known) = self.by_record.get(&record) {
            return known;
        }
        let bytes = self.db.record(record).unwrap_or(&[]);
        let kept = match sniff(bytes) {
            Sniffed::Kept(kind) => {
                self.kept.push(Image {
                    kind,
                    bytes: bytes.to_vec(),
                });
                Some(self.kept.len() - 1)
            }
            Sniffed::Gif => {
                losses.note(LOSS_GIF);
                None
            }
            Sniffed::Other => {
                losses.note(LOSS_OTHER_IMAGE);
                None
            }
        };
        self.by_record.insert(record, kept);
        kept
    }
}

enum Sniffed {
    Kept(ImageKind),
    Gif,
    Other,
}

fn sniff(bytes: &[u8]) -> Sniffed {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Sniffed::Kept(ImageKind::Jpeg)
    } else if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]) {
        Sniffed::Kept(ImageKind::Png)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Sniffed::Gif
    } else {
        Sniffed::Other
    }
}

#[derive(PartialEq)]
enum Pending {
    Paragraph,
    Heading,
}

struct Builder {
    sections: Vec<Section>,
    blocks: Vec<Block>,
    text: String,
    pending: Pending,
    space: bool,
}

impl Builder {
    fn push_text(&mut self, s: &str) {
        for c in s.chars() {
            if c.is_whitespace() && c != '\u{00A0}' {
                self.space = true;
            } else {
                if self.space && !self.text.is_empty() {
                    self.text.push(' ');
                }
                self.space = false;
                self.text.push(c);
            }
        }
    }

    fn flush(&mut self) {
        let text = std::mem::take(&mut self.text);
        self.space = false;
        let text = text.trim_matches('\u{00A0}').trim();
        if text.is_empty() {
            return;
        }
        let text = text.to_string();
        self.blocks.push(match self.pending {
            Pending::Heading => Block::Heading(text),
            Pending::Paragraph => Block::Paragraph(text),
        });
    }

    fn end_section(&mut self) {
        self.flush();
        if self.blocks.is_empty() {
            return;
        }
        let blocks = std::mem::take(&mut self.blocks);
        let title = blocks
            .iter()
            .find_map(|b| match b {
                Block::Heading(t) => Some(t.clone()),
                _ => None,
            })
            .unwrap_or_else(|| format!("Section {}", self.sections.len() + 1));
        self.sections.push(Section { title, blocks });
    }
}

/// One left-to-right pass over the MOBI 7 markup with no recursion: tags that
/// start or end a block flush the text gathered so far; `mbp:pagebreak` starts a
/// new section; `img recindex` places an image; everything else keeps its text.
fn build_sections(
    text: &str,
    images: &mut ImageTable<'_, '_>,
    losses: &mut Losses,
) -> Vec<Section> {
    let mut b = Builder {
        sections: Vec::new(),
        blocks: Vec::new(),
        text: String::new(),
        pending: Pending::Paragraph,
        space: false,
    };
    let mut skip_until: Option<&'static str> = None;
    let mut rest = text;
    while !rest.is_empty() {
        let lt = rest.find('<').unwrap_or(rest.len());
        if skip_until.is_none() {
            b.push_text(&decode_entities(&rest[..lt]));
        }
        rest = &rest[lt..];
        if rest.is_empty() {
            break;
        }
        if rest.starts_with("<!--") {
            match rest.find("-->") {
                Some(end) => rest = &rest[end + 3..],
                None => {
                    losses.note(LOSS_TRUNCATED_MARKUP);
                    break;
                }
            }
            continue;
        }
        let Some(gt) = rest.find('>') else {
            losses.note(LOSS_TRUNCATED_MARKUP);
            break;
        };
        let tag = Tag::parse(&rest[1..gt]);
        rest = &rest[gt + 1..];

        if let Some(end) = skip_until {
            if tag.closing && tag.name == end {
                skip_until = None;
            }
            continue;
        }
        match tag.name.as_str() {
            "head" | "script" | "style" if !tag.closing => {
                skip_until = Some(match tag.name.as_str() {
                    "head" => "head",
                    "script" => "script",
                    _ => "style",
                });
            }
            "mbp:pagebreak" => {
                if !tag.closing {
                    b.end_section();
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                b.flush();
                b.pending = if tag.closing {
                    Pending::Paragraph
                } else {
                    Pending::Heading
                };
            }
            "p" | "div" | "blockquote" | "br" | "hr" | "center" | "pre" | "dl" | "dt" | "dd" => {
                b.flush();
            }
            "table" | "tr" | "td" | "th" | "caption" | "thead" | "tbody" => {
                losses.note(LOSS_TABLES);
                b.flush();
            }
            "ul" | "ol" | "li" => {
                losses.note(LOSS_LISTS);
                b.flush();
            }
            "img" if !tag.closing => {
                match tag
                    .attr("recindex")
                    .and_then(|v| v.trim().parse::<usize>().ok())
                {
                    Some(recindex) => {
                        if let Some(index) = images.by_recindex(recindex, losses) {
                            b.flush();
                            b.blocks.push(Block::Image(index));
                        }
                    }
                    None => losses.note(LOSS_UNINDEXED_IMAGE),
                }
            }
            "a" => {
                if !tag.closing && (tag.attr("filepos").is_some() || tag.attr("href").is_some()) {
                    losses.note(LOSS_LINKS);
                }
            }
            "b" | "i" | "u" | "em" | "strong" | "font" | "tt" | "sub" | "sup" | "small" | "big"
            | "span" | "s" | "strike" | "del" | "ins" | "cite" | "code" | "var" => {
                losses.note(LOSS_FORMATTING);
            }
            "html" | "body" | "img" | "mbp:section" | "mbp:nu" | "mbp:frameset" | "head"
            | "script" | "style" | "guide" | "reference" | "" => {}
            _ => losses.note(LOSS_UNKNOWN_MARKUP),
        }
    }
    b.end_section();
    b.sections
}

struct Tag {
    name: String,
    closing: bool,
    attrs: String,
}

impl Tag {
    fn parse(inner: &str) -> Tag {
        let inner = inner.trim();
        let (closing, inner) = match inner.strip_prefix('/') {
            Some(rest) => (true, rest.trim_start()),
            None => (false, inner),
        };
        let inner = inner.strip_suffix('/').unwrap_or(inner);
        let split = inner
            .find(|c: char| c.is_whitespace())
            .unwrap_or(inner.len());
        Tag {
            name: inner[..split].to_ascii_lowercase(),
            closing,
            attrs: inner[split..].to_string(),
        }
    }

    /// Finds `name=value`, `name="value"` or `name='value'` among the attributes.
    fn attr(&self, name: &str) -> Option<String> {
        let mut rest = self.attrs.as_str();
        loop {
            rest = rest.trim_start();
            if rest.is_empty() {
                return None;
            }
            let key_end = rest
                .find(|c: char| c == '=' || c.is_whitespace())
                .unwrap_or(rest.len());
            let key = &rest[..key_end];
            rest = rest[key_end..].trim_start();
            let value = if let Some(after) = rest.strip_prefix('=') {
                let after = after.trim_start();
                let (value, remaining) = match after.chars().next() {
                    Some(q @ ('"' | '\'')) => {
                        let body = &after[1..];
                        let end = body.find(q).unwrap_or(body.len());
                        (&body[..end], body.get(end + 1..).unwrap_or(""))
                    }
                    _ => {
                        let end = after.find(char::is_whitespace).unwrap_or(after.len());
                        (&after[..end], &after[end..])
                    }
                };
                rest = remaining;
                Some(value)
            } else {
                None
            };
            if key.eq_ignore_ascii_case(name) {
                return Some(value.unwrap_or("").to_string());
            }
        }
    }
}

fn decode_entities(s: &str) -> std::borrow::Cow<'_, str> {
    if !s.contains('&') {
        return std::borrow::Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let end = rest.bytes().take(12).position(|b| b == b';');
        let decoded = end.and_then(|end| entity(&rest[1..end]).map(|c| (c, end)));
        match decoded {
            Some((c, end)) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    std::borrow::Cow::Owned(out)
}

fn entity(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse::<u32>().ok()?,
        };
        return Some(char::from_u32(code).unwrap_or('\u{FFFD}'));
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{00A0}',
        _ => return None,
    })
}

fn invalid(reason: &str) -> DecodeError {
    DecodeError::Invalid(reason.to_string())
}

fn be16(bytes: &[u8], at: usize) -> Option<u16> {
    let b = bytes.get(at..at.checked_add(2)?)?;
    Some(u16::from_be_bytes([b[0], b[1]]))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at.checked_add(4)?)?;
    Some(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}
