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

/// A resource as the helper sends it: its kind in the reply line, its bytes
/// after it. The host checks the bytes against the kind before they become a
/// [`crate::Resource`].
#[derive(Debug, Serialize, Deserialize)]
pub struct WireResource {
    pub kind: ResourceKind,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

/// Longest request line, its newline included.
pub const MAX_REQUEST_BYTES: usize = 32;

/// One thing the host asks of a running helper. On the wire it is one line:
/// `metadata`, `open`, `survey`, `chapter <n>`, `content <n>` or
/// `resource <id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Request {
    Metadata,
    Open,
    Survey,
    Chapter(u32),
    Content(u32),
    Resource(u32),
}

impl Request {
    pub fn line(self) -> String {
        match self {
            Request::Metadata => "metadata\n".into(),
            Request::Open => "open\n".into(),
            Request::Survey => "survey\n".into(),
            Request::Chapter(n) => format!("chapter {n}\n"),
            Request::Content(n) => format!("content {n}\n"),
            Request::Resource(n) => format!("resource {n}\n"),
        }
    }

    /// `None` for anything [`Request::line`] does not write.
    pub fn parse(line: &[u8]) -> Option<Request> {
        if line.len() > MAX_REQUEST_BYTES {
            return None;
        }
        let text = std::str::from_utf8(line).ok()?;
        let words = text.strip_suffix('\n')?;
        let request = match words.split_once(' ') {
            None => match words {
                "metadata" => Request::Metadata,
                "open" => Request::Open,
                "survey" => Request::Survey,
                _ => return None,
            },
            Some((op, n)) => {
                let n: u32 = n.parse().ok()?;
                match op {
                    "chapter" => Request::Chapter(n),
                    "content" => Request::Content(n),
                    "resource" => Request::Resource(n),
                    _ => return None,
                }
            }
        };
        (request.line() == text).then_some(request)
    }
}

/// Write one reply: the length of the bytes that follow, a space, the reply
/// as one line of JSON, then the bytes. JSON escapes every newline inside a
/// string, so the line ends where the reply does.
pub fn write_frame(
    out: &mut impl std::io::Write,
    response: &Response,
    blob: &[u8],
) -> std::io::Result<()> {
    let mut line = format!("{} ", blob.len()).into_bytes();
    serde_json::to_writer(&mut line, response)?;
    line.push(b'\n');
    out.write_all(&line)?;
    out.write_all(blob)?;
    out.flush()
}

/// Why a reply could not be read.
#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// The stream ended before a reply began.
    Closed,
    /// More than the allowance, counting the line and the bytes.
    TooLarge,
    /// Anything else, an ending in the middle of a reply included.
    Malformed,
}

/// Read one reply written by [`write_frame`], refusing one over `max` bytes
/// in all, line and bytes together.
pub fn read_frame(
    input: &mut impl std::io::BufRead,
    max: u64,
) -> Result<(Response, Vec<u8>), FrameError> {
    use std::io::Read;
    let mut line = Vec::new();
    let read = std::io::BufRead::read_until(
        &mut input.by_ref().take(max.saturating_add(1)),
        b'\n',
        &mut line,
    )
    .map_err(|_| FrameError::Malformed)?;
    if read == 0 {
        return Err(FrameError::Closed);
    }
    if line.len() as u64 > max {
        return Err(FrameError::TooLarge);
    }
    let body = line.strip_suffix(b"\n").ok_or(FrameError::Malformed)?;
    let space = body
        .iter()
        .position(|b| *b == b' ')
        .ok_or(FrameError::Malformed)?;
    let digits = &body[..space];
    if digits.is_empty() || digits.len() > 19 || !digits.iter().all(u8::is_ascii_digit) {
        return Err(FrameError::Malformed);
    }
    let len: u64 = std::str::from_utf8(digits)
        .ok()
        .and_then(|d| d.parse().ok())
        .ok_or(FrameError::Malformed)?;
    if len.saturating_add(line.len() as u64) > max {
        return Err(FrameError::TooLarge);
    }
    let response: Response =
        serde_json::from_slice(&body[space + 1..]).map_err(|_| FrameError::Malformed)?;
    let mut blob = vec![0; len as usize];
    input
        .read_exact(&mut blob)
        .map_err(|_| FrameError::Malformed)?;
    Ok((response, blob))
}

/// Every element the markup writer writes. The host accepts no other.
pub const MARKUP_ELEMENTS: [&str; 71] = [
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "i",
    "img",
    "ins",
    "kbd",
    "li",
    "main",
    "mark",
    "nav",
    "ol",
    "p",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "section",
    "small",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "u",
    "ul",
    "var",
    "wbr",
];
/// Attributes the writer copies from the book, by the name it writes.
pub const MARKUP_ATTRIBUTES: [&str; 11] = [
    "id",
    "class",
    "lang",
    "dir",
    "title",
    "alt",
    "colspan",
    "rowspan",
    "start",
    "value",
    "data-epub-type",
];

/// `/c/<n>` with an optional `#<anchor>`, or `/r/<n>`: the only addresses
/// the writer writes.
fn book_address(value: &str, prefix: &str, fragment: bool) -> bool {
    let Some(rest) = value.strip_prefix(prefix) else {
        return false;
    };
    let (digits, anchor) = match rest.split_once('#') {
        Some((d, a)) if fragment => (d, a),
        Some(_) => return false,
        None => (rest, ""),
    };
    !digits.is_empty()
        && digits.len() <= 10
        && digits.bytes().all(|b| b.is_ascii_digit())
        && anchor
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'%'))
}

/// The UTF-16 length of the text in markup the helper wrote, or `None` when
/// the markup is not in the writer's form. The form is small enough to
/// check without an XML parser:
///
/// - text, with `&amp;`, `&lt;` and `&gt;` as its only entities, no `>`, and
///   no control character but tab and line feed;
/// - `<name attr="value" ...>`, the same ending in `/>`, and `</name>`, with
///   one space before each attribute and values in double quotes;
/// - `name` from [`MARKUP_ELEMENTS`]; `attr` from [`MARKUP_ATTRIBUTES`], or
///   `href` or `data-external` on `a`, or `src` on `img`, none twice;
/// - `href` only as `/c/<n>` or `/c/<n>#<anchor>`, and `src` only as `/r/<n>`;
/// - in a value, no `<`, `>`, `"` or control character, and the entities
///   above plus `&quot;`.
///
/// Comments, CDATA sections, processing instructions, declarations,
/// namespace declarations, carriage returns and character references are
/// outside it. Nesting is not checked: the view's XML parser refuses markup
/// that is not balanced.
pub fn markup_text_units(markup: &str) -> Option<u32> {
    fn plain(c: char) -> bool {
        !c.is_control() && !matches!(c, '\u{feff}' | '\u{fffe}' | '\u{ffff}')
    }
    fn entity<'a>(rest: &'a str, allowed: &[&str]) -> Option<&'a str> {
        let found = allowed.iter().find(|e| rest.starts_with(**e))?;
        Some(&rest[found.len()..])
    }
    /// A name at the start of `rest`, and what follows it.
    fn name(rest: &str) -> Option<(&str, &str)> {
        let end = rest
            .find(|c: char| !(c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'))
            .unwrap_or(rest.len());
        (end > 0).then(|| rest.split_at(end))
    }
    /// The rest after one tag that starts at `<`.
    fn tag(rest: &str) -> Option<&str> {
        let rest = &rest[1..];
        if let Some(end) = rest.strip_prefix('/') {
            let (element, rest) = name(end)?;
            return MARKUP_ELEMENTS
                .contains(&element)
                .then(|| rest.strip_prefix('>'))?;
        }
        let (element, mut rest) = name(rest)?;
        if !MARKUP_ELEMENTS.contains(&element) {
            return None;
        }
        let mut seen: Vec<&str> = Vec::new();
        loop {
            if let Some(done) = rest.strip_prefix("/>").or_else(|| rest.strip_prefix('>')) {
                return Some(done);
            }
            let (attribute, after) = name(rest.strip_prefix(' ')?)?;
            let known = MARKUP_ATTRIBUTES.contains(&attribute)
                || (element == "a" && matches!(attribute, "href" | "data-external"))
                || (element == "img" && attribute == "src");
            if !known || seen.contains(&attribute) {
                return None;
            }
            seen.push(attribute);
            let value = after.strip_prefix("=\"")?;
            rest = value;
            loop {
                let c = rest.chars().next()?;
                rest = match c {
                    '"' => break,
                    '<' | '>' => return None,
                    '&' => entity(rest, &["&amp;", "&lt;", "&gt;", "&quot;"])?,
                    c if plain(c) => &rest[c.len_utf8()..],
                    _ => return None,
                };
            }
            let value = &value[..value.len() - rest.len()];
            let placed = match attribute {
                "href" => book_address(value, "/c/", true),
                "src" => book_address(value, "/r/", false),
                _ => true,
            };
            if !placed {
                return None;
            }
            rest = &rest[1..];
        }
    }
    let mut units = 0u32;
    let mut rest = markup;
    while let Some(c) = rest.chars().next() {
        match c {
            '<' => rest = tag(rest)?,
            '>' => return None,
            '&' => {
                rest = entity(rest, &["&amp;", "&lt;", "&gt;"])?;
                units = units.checked_add(1)?;
            }
            c if plain(c) || c == '\t' || c == '\n' => {
                units = units.checked_add(c.len_utf16() as u32)?;
                rest = &rest[c.len_utf8()..];
            }
            _ => return None,
        }
    }
    Some(units)
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
    /// RGB, row by row. Sent after the reply line.
    #[serde(skip)]
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
    use super::{read_frame, write_frame, FrameError, Payload, ProbeReport, Request, Response};
    use std::io::Cursor;

    #[test]
    fn requests_parse_only_in_the_form_they_are_written() {
        for request in [
            Request::Metadata,
            Request::Open,
            Request::Survey,
            Request::Chapter(0),
            Request::Content(u32::MAX),
            Request::Resource(17),
        ] {
            assert_eq!(Request::parse(request.line().as_bytes()), Some(request));
        }
        for bad in [
            "open",
            "open \n",
            " open\n",
            "chapter\n",
            "chapter 01\n",
            "chapter -1\n",
            "chapter +1\n",
            "chapter 4294967296\n",
            "content 1 2\n",
            "probe\n",
            "OPEN\n",
            "resource 1\r\n",
        ] {
            assert_eq!(Request::parse(bad.as_bytes()), None, "{bad:?}");
        }
        assert_eq!(Request::parse(&[b'x'; 64]), None);
    }

    #[test]
    fn frames_round_trip_and_refuse_what_does_not_fit() {
        let ready = Response::Ok(Payload::Probe(ProbeReport { isolated: true }));
        let mut wire = Vec::new();
        write_frame(&mut wire, &ready, &[0, b'\n', 255]).unwrap();
        write_frame(&mut wire, &ready, &[]).unwrap();
        let mut input = Cursor::new(wire.clone());
        let (_, blob) = read_frame(&mut input, 1024).unwrap();
        assert_eq!(blob, [0, b'\n', 255]);
        let (_, blob) = read_frame(&mut input, 1024).unwrap();
        assert!(blob.is_empty());
        assert_eq!(
            read_frame(&mut input, 1024).unwrap_err(),
            FrameError::Closed
        );

        let first = wire.iter().position(|b| *b == b'\n').unwrap() + 1;
        assert_eq!(
            read_frame(&mut Cursor::new(&wire[..first + 3]), first as u64 + 2).unwrap_err(),
            FrameError::TooLarge
        );
        assert_eq!(
            read_frame(&mut Cursor::new(&wire[..first + 2]), 1024).unwrap_err(),
            FrameError::Malformed
        );
        for bad in [
            &b"{\"Ok\":{\"Probe\":{\"isolated\":true}}}\n"[..],
            b"x {}\n",
            b"0 {\"Ok\":{\"Probe\":{\"isolated\":true}}}",
            b"99999999999999999999 {}\n",
            b"-1 {}\n",
        ] {
            assert_eq!(
                read_frame(&mut Cursor::new(bad), 1 << 40).unwrap_err(),
                FrameError::Malformed,
                "{}",
                String::from_utf8_lossy(bad)
            );
        }
    }
}
