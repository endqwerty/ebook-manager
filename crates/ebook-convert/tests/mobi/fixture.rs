//! Builds MOBI 7 test files. Self-contained so job integration tests can include
//! it with `#[path = ".../tests/mobi/fixture.rs"] mod fixture;`.
//!
//! `MobiFixture::sample().build()` produces the bytes checked in as
//! `tests/mobi/sample.mobi`; README.md beside this file lists what decoding it
//! must yield.
#![allow(dead_code)]

pub const JPEG: &[u8] = include_bytes!("images/tiny.jpg");
pub const PNG: &[u8] = include_bytes!("images/tiny.png");
pub const GIF: &[u8] = include_bytes!("images/tiny.gif");

const ABSENT: u32 = 0xFFFF_FFFF;
const RECORD: usize = 4096;
const HEADER_LENGTH: usize = 232;

#[derive(Clone)]
pub struct MobiFixture {
    pub pdb_name: String,
    pub title: String,
    pub authors: Vec<String>,
    pub language: Option<String>,
    pub locale: u32,
    pub encoding: u32,
    pub compression: u16,
    pub encryption: u16,
    pub drm: bool,
    pub mobi_type: u32,
    pub version: u32,
    /// Adds an EXTH 121 ("KF8 BOUNDARY Offset") record holding 7.
    pub exth_kf8_boundary: bool,
    /// Adds a record holding the bytes `BOUNDARY` after the images. No allowed
    /// source describes such a record's content; the decoder must ignore it.
    pub boundary_record: bool,
    pub extra_flags: u32,
    pub markup: String,
    pub images: Vec<Vec<u8>>,
    pub cover: Option<u32>,
    pub declared_text_length: Option<u32>,
    /// Replaces the generated text records (bytes as stored, trailing entries included).
    pub raw_text_records: Option<Vec<Vec<u8>>>,
    /// Overrides the text record count in the PalmDOC header.
    pub declared_text_records: Option<u16>,
    /// Overrides the first content record number (record 0 offset 192).
    pub first_content_record: Option<u16>,
    /// Bytes stored as the full name instead of the encoded title.
    pub raw_title: Option<Vec<u8>>,
    /// Extra EXTH records appended after the generated ones.
    pub extra_exth: Vec<(u32, Vec<u8>)>,
    /// Records placed after the text records and before the 2-zero-byte record.
    pub records_after_text: Vec<Vec<u8>>,
    /// Overrides the first non-book record number (record 0 offset 80).
    pub first_non_book: Option<u32>,
}

impl Default for MobiFixture {
    fn default() -> Self {
        MobiFixture {
            pdb_name: "Fixture_Book".into(),
            title: "Fixture Book".into(),
            authors: vec!["Test Author".into()],
            language: Some("en".into()),
            locale: 1033,
            encoding: 65001,
            compression: 2,
            encryption: 0,
            drm: false,
            mobi_type: 2,
            version: 6,
            exth_kf8_boundary: false,
            boundary_record: false,
            extra_flags: 0x3,
            markup: "<html><body><p>Hello.</p></body></html>".into(),
            images: Vec::new(),
            cover: None,
            declared_text_length: None,
            raw_text_records: None,
            declared_text_records: None,
            first_content_record: None,
            raw_title: None,
            extra_exth: Vec::new(),
            records_after_text: Vec::new(),
            first_non_book: None,
        }
    }
}

/// Paragraph repeated in the sample to push its text past one 4096-byte record.
pub const SAMPLE_FILLER: &str =
    "The tide came in over the flats while the keeper counted the gulls, \
one by one, and wrote each number in the margin of the log.";

impl MobiFixture {
    /// The checked-in sample: three chapters, two kept images, one GIF that is
    /// reported as lost, a link, a table and multibyte text across records.
    pub fn sample() -> MobiFixture {
        let mut filler = String::new();
        for i in 1..=60 {
            filler.push_str(&format!("<p>{i}. {SAMPLE_FILLER}</p>"));
        }
        let markup = format!(
            "<html><head><guide><reference type=\"toc\" title=\"Contents\" filepos=0000000000 /></guide></head><body>\
<h1>Chapter One: The Lamp</h1>\
<p>The lamp at Gull Point had burned for <i>ninety</i> years.</p>\
<p>Its keeper, Marta Ruiz &amp; her cat &#8212; kept the log in caf\u{e9} ink: \u{201c}All&nbsp;clear.\u{201d}</p>\
<p><img recindex=\"00001\" align=\"baseline\"></img></p>\
<mbp:pagebreak/>\
<h1>Chapter Two: The Storm</h1>\
<p>Wind rose from the west.<br/>Waves broke on the rocks.</p>\
{filler}\
<p>See <a filepos=0000000010>chapter one</a> for the lamp.</p>\
<table><tr><td>Night</td><td>Ships</td></tr></table>\
<p><img recindex=\"00003\"></img></p>\
<mbp:pagebreak></mbp:pagebreak>\
<h2>Chapter Three: Morning</h2>\
<ul><li>Gulls: 41</li></ul>\
<p><img recindex=\"00002\"/></p>\
<p>The end. \u{1F30A}</p>\
</body></html>"
        );
        MobiFixture {
            pdb_name: "The_Lantern_Keeper".into(),
            title: "The Lantern Keeper".into(),
            authors: vec!["Ada Marlow".into(), "Ben Ortiz".into()],
            language: Some("en".into()),
            markup,
            images: vec![JPEG.to_vec(), PNG.to_vec(), GIF.to_vec()],
            cover: Some(1),
            ..MobiFixture::default()
        }
    }

    pub fn encoded_text(&self) -> Vec<u8> {
        if self.encoding == 1252 {
            self.markup.chars().map(cp1252_byte).collect()
        } else {
            self.markup.as_bytes().to_vec()
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let text = self.encoded_text();
        let text_records = match &self.raw_text_records {
            Some(records) => records.clone(),
            None => text_records(&text, self.compression, self.extra_flags),
        };
        let text_length = self.declared_text_length.unwrap_or(text.len() as u32);

        let mut records: Vec<Vec<u8>> = vec![Vec::new()];
        records.extend(text_records.iter().cloned());
        records.extend(self.records_after_text.iter().cloned());
        let first_non_book = self.first_non_book.map_or(records.len(), |n| n as usize);
        records.push(vec![0, 0]);
        let first_image = if self.images.is_empty() {
            ABSENT
        } else {
            records.len() as u32
        };
        records.extend(self.images.iter().cloned());
        if self.boundary_record {
            records.push(b"BOUNDARY".to_vec());
        }
        records.push(flis());
        records.push(fcis(text_length));
        records.push(vec![0xE9, 0x8E, 0x0D, 0x0A]);

        let count = self
            .declared_text_records
            .map_or(text_records.len(), usize::from);
        records[0] = self.record0(text_length, count, first_non_book, first_image);
        palm_database(&self.pdb_name, &records)
    }

    fn record0(
        &self,
        text_length: u32,
        text_records: usize,
        first_non_book: usize,
        first_image: u32,
    ) -> Vec<u8> {
        let mut r = Vec::new();
        put16(&mut r, self.compression);
        put16(&mut r, 0);
        put32(&mut r, text_length);
        put16(&mut r, text_records as u16);
        put16(&mut r, RECORD as u16);
        put16(&mut r, self.encryption);
        put16(&mut r, 0);

        let exth = self.exth();
        let name = match &self.raw_title {
            Some(bytes) => bytes.clone(),
            None => self.encode(&self.title),
        };
        let name_offset = 16 + HEADER_LENGTH + exth.len();

        let mut h = vec![0u8; HEADER_LENGTH];
        h[0..4].copy_from_slice(b"MOBI");
        set32(&mut h, 4, HEADER_LENGTH as u32);
        set32(&mut h, 8, self.mobi_type);
        set32(&mut h, 12, self.encoding);
        set32(&mut h, 16, 0x5EED_0001);
        set32(&mut h, 20, self.version);
        for at in (24..64).step_by(4) {
            set32(&mut h, at, ABSENT);
        }
        set32(&mut h, 64, first_non_book as u32);
        set32(&mut h, 68, name_offset as u32);
        set32(&mut h, 72, name.len() as u32);
        set32(&mut h, 76, self.locale);
        set32(&mut h, 88, self.version);
        set32(&mut h, 92, first_image);
        set32(&mut h, 112, if exth.is_empty() { 0 } else { 0x50 });
        set32(&mut h, 152, if self.drm { 0x200 } else { ABSENT });
        set32(&mut h, 156, if self.drm { 1 } else { ABSENT });
        h[176..178].copy_from_slice(&self.first_content_record.unwrap_or(1).to_be_bytes());
        h[178..180].copy_from_slice(&(text_records as u16).to_be_bytes());
        set32(&mut h, 224, self.extra_flags);
        set32(&mut h, 228, ABSENT);
        r.extend_from_slice(&h);
        r.extend_from_slice(&exth);
        r.extend_from_slice(&name);
        r.extend_from_slice(&[0, 0]);
        while r.len() % 4 != 0 {
            r.push(0);
        }
        r
    }

    fn exth(&self) -> Vec<u8> {
        let mut entries: Vec<(u32, Vec<u8>)> = Vec::new();
        for author in &self.authors {
            entries.push((100, self.encode(author)));
        }
        if let Some(language) = &self.language {
            entries.push((524, self.encode(language)));
        }
        if let Some(cover) = self.cover {
            entries.push((201, cover.to_be_bytes().to_vec()));
        }
        if self.exth_kf8_boundary {
            entries.push((121, 7u32.to_be_bytes().to_vec()));
        }
        entries.extend(self.extra_exth.iter().cloned());
        if entries.is_empty() {
            return Vec::new();
        }
        let mut body = Vec::new();
        for (kind, data) in &entries {
            put32(&mut body, *kind);
            put32(&mut body, data.len() as u32 + 8);
            body.extend_from_slice(data);
        }
        let mut exth = b"EXTH".to_vec();
        put32(&mut exth, 12 + body.len() as u32);
        put32(&mut exth, entries.len() as u32);
        exth.extend_from_slice(&body);
        while !exth.len().is_multiple_of(4) {
            exth.push(0);
        }
        exth
    }

    fn encode(&self, s: &str) -> Vec<u8> {
        if self.encoding == 1252 {
            s.chars().map(cp1252_byte).collect()
        } else {
            s.as_bytes().to_vec()
        }
    }
}

/// Splits text into 4096-byte records, compresses each, and appends trailing
/// entries: a multibyte overlap entry for bit 0 and a one-byte filler entry for
/// every other set bit.
pub fn text_records(text: &[u8], compression: u16, extra_flags: u32) -> Vec<Vec<u8>> {
    let mut records = Vec::new();
    for start in (0..text.len()).step_by(RECORD) {
        let end = (start + RECORD).min(text.len());
        let chunk = &text[start..end];
        let mut record = if compression == 2 {
            palmdoc_compress(chunk)
        } else {
            chunk.to_vec()
        };
        if extra_flags & 1 != 0 {
            let overlap = utf8_overlap(text, end);
            record.extend_from_slice(&text[end..end + overlap]);
            record.push(overlap as u8);
        }
        for bit in 1..32 {
            if extra_flags & (1 << bit) != 0 {
                record.push(0x00);
                record.push(0x82);
            }
        }
        records.push(record);
    }
    records
}

/// How many bytes of the character that starts before `end` lie after it.
fn utf8_overlap(text: &[u8], end: usize) -> usize {
    if end >= text.len() {
        return 0;
    }
    let mut start = end;
    while start > 0 && end - start < 4 && text[start - 1] & 0xC0 == 0x80 {
        start -= 1;
    }
    if start == 0 {
        return 0;
    }
    let lead = text[start - 1];
    let width = match lead {
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => return 0,
    };
    (start - 1 + width).saturating_sub(end).min(3)
}

/// PalmDOC compression with a short greedy back-reference search.
pub fn palmdoc_compress(input: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < input.len() {
        let (mut best_len, mut best_dist) = (0, 0);
        for dist in 1..=i.min(512) {
            let mut len = 0;
            while len < 10 && i + len < input.len() && input[i + len] == input[i + len - dist] {
                len += 1;
            }
            if len > best_len {
                best_len = len;
                best_dist = dist;
            }
        }
        if best_len >= 3 {
            let pair = 0x8000 | (best_dist << 3) | (best_len - 3);
            out.push((pair >> 8) as u8);
            out.push(pair as u8);
            i += best_len;
            continue;
        }
        let byte = input[i];
        if byte == b' ' && i + 1 < input.len() && (0x40..=0x7F).contains(&input[i + 1]) {
            out.push(input[i + 1] ^ 0x80);
            i += 2;
        } else if byte == 0 || (0x09..=0x7F).contains(&byte) {
            out.push(byte);
            i += 1;
        } else {
            let mut run = 0;
            while run < 8 && i + run < input.len() {
                let b = input[i + run];
                if b == 0 || (0x09..=0x7F).contains(&b) {
                    break;
                }
                run += 1;
            }
            out.push(run as u8);
            out.extend_from_slice(&input[i..i + run]);
            i += run;
        }
    }
    out
}

pub fn palm_database(name: &str, records: &[Vec<u8>]) -> Vec<u8> {
    let mut out = vec![0u8; 32];
    let name = name.as_bytes();
    let n = name.len().min(31);
    out[..n].copy_from_slice(&name[..n]);
    out.extend_from_slice(&[0u8; 28]);
    out.extend_from_slice(b"BOOKMOBI");
    put32(&mut out, 0);
    put32(&mut out, 0);
    put16(&mut out, records.len() as u16);
    let mut offset = 78 + records.len() * 8 + 2;
    for (i, record) in records.iter().enumerate() {
        put32(&mut out, offset as u32);
        let uid = (2 * i) as u32;
        out.push(0);
        out.extend_from_slice(&uid.to_be_bytes()[1..]);
        offset += record.len();
    }
    out.extend_from_slice(&[0, 0]);
    for record in records {
        out.extend_from_slice(record);
    }
    out
}

fn flis() -> Vec<u8> {
    let mut r = b"FLIS".to_vec();
    for v in [8u32, 65, 0, 0, ABSENT, 1, 3, 3, 1, ABSENT].iter().take(8) {
        put32(&mut r, *v);
    }
    r
}

fn fcis(text_length: u32) -> Vec<u8> {
    let mut r = b"FCIS".to_vec();
    for v in &[20u32, 16, 1, 0, text_length, 0, 32, 8, 1] {
        put32(&mut r, *v);
    }
    r
}

pub fn cp1252_byte(c: char) -> u8 {
    const HIGH: [u32; 32] = [
        0x20AC, 0, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039,
        0x0152, 0, 0x017D, 0, 0, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC,
        0x2122, 0x0161, 0x203A, 0x0153, 0, 0x017E, 0x0178,
    ];
    let code = c as u32;
    if code < 0x80 || (0xA0..=0xFF).contains(&code) {
        return code as u8;
    }
    match HIGH.iter().position(|&h| h == code && h != 0) {
        Some(i) => 0x80 + i as u8,
        None => b'?',
    }
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_be_bytes());
}

fn set32(h: &mut [u8], at: usize, v: u32) {
    h[at..at + 4].copy_from_slice(&v.to_be_bytes());
}
