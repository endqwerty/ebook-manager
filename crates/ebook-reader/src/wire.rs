//! The helper's reply format. Private to this crate's two halves.

use crate::model::{Chapter, ChapterContent, ChapterInfo, Metadata, ResourceKind, Rgb, TocEntry};
use serde::{Deserialize, Serialize};

/// Where the sandbox places the helper and the book.
pub const HELPER_IN_SANDBOX: &str = "/helper";
pub const BOOK_IN_SANDBOX: &str = "/book.epub";

#[derive(Debug, Serialize, Deserialize)]
pub enum Response {
    Ok(Payload),
    Err(WireError),
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Payload {
    Probe(ProbeReport),
    Metadata(Metadata),
    Book(BookOutline),
    Chapter(Chapter),
    Survey(WireSurvey),
    Content(ChapterContent),
    Resource(WireResource),
}

/// A resource as the helper sends it. The host checks the bytes against the
/// kind before it becomes a [`crate::Resource`].
#[derive(Debug, Serialize, Deserialize)]
pub struct WireResource {
    pub kind: ResourceKind,
    /// The bytes in base64, which costs a third more than the bytes where a
    /// JSON array of numbers would cost three times more.
    pub base64: String,
}

const BASE64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/// The UTF-16 length of the text in markup the helper wrote, or `None` when
/// the markup is not in the helper's form. That form makes this exact without
/// parsing: `<` only starts a tag, which is `<name`, `</name` with a
/// lower-case name, `>` only ends one, and the only entities in text are
/// `&amp;`, `&lt;` and `&gt;`. Comments, CDATA sections, processing
/// instructions and declarations are not in that form, so they are refused
/// and cannot hide text from the count.
pub fn markup_text_units(markup: &str) -> Option<u32> {
    let mut units = 0u32;
    let mut rest = markup;
    while let Some(c) = rest.chars().next() {
        match c {
            '<' => {
                let tag = &rest[1..rest.find('>')?];
                let name = tag.strip_prefix('/').unwrap_or(tag);
                if !name.starts_with(|c: char| c.is_ascii_lowercase()) || tag.contains('<') {
                    return None;
                }
                rest = &rest[tag.len() + 2..];
            }
            '>' => return None,
            '&' => {
                let entity = ["&amp;", "&lt;", "&gt;"]
                    .into_iter()
                    .find(|e| rest.starts_with(e))?;
                units = units.checked_add(1)?;
                rest = &rest[entity.len()..];
            }
            c => {
                units = units.checked_add(c.len_utf16() as u32)?;
                rest = &rest[c.len_utf8()..];
            }
        }
    }
    Some(units)
}

pub fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for (i, shift) in [18u32, 12, 6, 0].into_iter().enumerate() {
            if i <= chunk.len() {
                out.push(BASE64[(n >> shift & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// `None` for anything but canonical padded base64.
pub fn base64_decode(text: &str) -> Option<Vec<u8>> {
    let text = text.as_bytes();
    if !text.len().is_multiple_of(4) {
        return None;
    }
    let value = |c: u8| BASE64.iter().position(|b| *b == c).map(|p| p as u32);
    let mut out = Vec::with_capacity(text.len() / 4 * 3);
    let last = text.len() / 4;
    for (at, chunk) in text.chunks(4).enumerate() {
        let padding = chunk.iter().rev().take_while(|c| **c == b'=').count();
        if padding > 2 || (padding > 0 && at + 1 != last) {
            return None;
        }
        let mut n = 0u32;
        for c in &chunk[..4 - padding] {
            n = n << 6 | value(*c)?;
        }
        n <<= 6 * padding as u32;
        // The bits padding stands in for are 0 in canonical text.
        if n & ((1u32 << (8 * padding as u32)) - 1) != 0 {
            return None;
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..3 - padding]);
    }
    Some(out)
}

/// A survey as the helper sends it. The host checks the cover before it
/// becomes a [`crate::Thumbnail`].
#[derive(Debug, Serialize, Deserialize)]
pub struct WireSurvey {
    pub words: Option<u32>,
    pub cover: Option<WireCover>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct WireCover {
    pub width: u32,
    pub height: u32,
    /// RGB, row by row.
    pub pixels: Vec<u8>,
    pub average: Rgb,
    pub spine: Rgb,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProbeReport {
    pub isolated: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct BookOutline {
    pub metadata: Metadata,
    pub chapters: Vec<ChapterInfo>,
    pub toc: Vec<TocEntry>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WireErrorKind {
    NotEpub,
    Protected,
    Compression,
    Encoding,
    Invalid,
    Limit,
    ChapterOutOfRange,
    NotIsolated,
    Usage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireError {
    pub kind: WireErrorKind,
    pub detail: String,
}

impl WireError {
    pub fn new(kind: WireErrorKind, detail: impl Into<String>) -> Self {
        WireError {
            kind,
            detail: detail.into(),
        }
    }
    pub fn invalid(detail: impl Into<String>) -> Self {
        Self::new(WireErrorKind::Invalid, detail)
    }
    pub fn limit(detail: impl Into<String>) -> Self {
        Self::new(WireErrorKind::Limit, detail)
    }
}

#[cfg(test)]
mod tests {
    use super::{base64_decode, base64_encode};

    #[test]
    fn base64_round_trips_every_length_and_refuses_bad_text() {
        for len in 0..40usize {
            let bytes: Vec<u8> = (0..len).map(|i| (i * 37 + 250) as u8).collect();
            assert_eq!(base64_decode(&base64_encode(&bytes)), Some(bytes));
        }
        assert_eq!(base64_encode(b"Man"), "TWFu");
        assert_eq!(base64_encode(b"Ma"), "TWE=");
        assert_eq!(base64_encode(b"M"), "TQ==");
        for bad in [
            "TWF", "TW=u", "T===", "TQ==TWFu", "TW!u", "TWFu\n", "TR==", "TWF=",
        ] {
            assert_eq!(base64_decode(bad), None, "{bad}");
        }
    }
}
