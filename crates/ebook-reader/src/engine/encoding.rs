//! Turn a document's bytes into text. UTF-8 and UTF-16 are what EPUB allows;
//! Latin-1 and Windows-1252 are accepted because older books declare them.

use crate::wire::{WireError, WireErrorKind};

pub fn decode(bytes: &[u8]) -> Result<String, WireError> {
    if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return Ok(String::from_utf8_lossy(rest).into_owned());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        return Ok(utf16(rest, false));
    }
    if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        return Ok(utf16(rest, true));
    }
    if bytes.starts_with(&[b'<', 0, b'?', 0]) {
        return Ok(utf16(bytes, false));
    }
    if bytes.starts_with(&[0, b'<', 0, b'?']) {
        return Ok(utf16(bytes, true));
    }
    match declared(bytes).as_deref() {
        None | Some("utf-8") | Some("utf8") | Some("us-ascii") | Some("ascii") => {
            Ok(String::from_utf8_lossy(bytes).into_owned())
        }
        Some("iso-8859-1") | Some("latin1") | Some("iso8859-1") | Some("latin-1") => {
            Ok(bytes.iter().map(|&b| b as char).collect())
        }
        Some("windows-1252") | Some("cp1252") | Some("cp-1252") => {
            Ok(bytes.iter().map(|&b| cp1252(b)).collect())
        }
        Some(other) => Err(WireError::new(
            WireErrorKind::Encoding,
            format!("text encoding {}", super::clean_inline(other)),
        )),
    }
}

fn utf16(bytes: &[u8], big_endian: bool) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|p| {
        if big_endian {
            u16::from_be_bytes([p[0], p[1]])
        } else {
            u16::from_le_bytes([p[0], p[1]])
        }
    });
    char::decode_utf16(units).map(|r| r.unwrap_or('\u{fffd}')).collect()
}

/// The `encoding` named in an XML declaration at the start of the document.
fn declared(bytes: &[u8]) -> Option<String> {
    let head = &bytes[..bytes.len().min(200)];
    let head = String::from_utf8_lossy(head);
    let head = head.trim_start();
    if !head.starts_with("<?xml") {
        return None;
    }
    let decl = &head[..head.find("?>")?];
    let after = &decl[decl.find("encoding")? + "encoding".len()..];
    let after = after.trim_start().strip_prefix('=')?.trim_start();
    let quote = after.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let value = &after[1..];
    let value = &value[..value.find(quote)?];
    Some(value.trim().to_ascii_lowercase())
}

fn cp1252(b: u8) -> char {
    const HIGH: [char; 32] = [
        '\u{20ac}', '\u{fffd}', '\u{201a}', '\u{0192}', '\u{201e}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02c6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{fffd}',
        '\u{017d}', '\u{fffd}', '\u{fffd}', '\u{2018}', '\u{2019}', '\u{201c}', '\u{201d}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02dc}', '\u{2122}', '\u{0161}', '\u{203a}',
        '\u{0153}', '\u{fffd}', '\u{017e}', '\u{0178}',
    ];
    match b {
        0x80..=0x9f => HIGH[(b - 0x80) as usize],
        _ => b as char,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_encodings_real_books_use() {
        assert_eq!(decode("caf\u{e9}".as_bytes()).unwrap(), "caf\u{e9}");
        assert_eq!(decode(b"\xEF\xBB\xBFhi").unwrap(), "hi");
        assert_eq!(decode(b"\xFF\xFEh\x00i\x00").unwrap(), "hi");
        assert_eq!(decode(b"\xFE\xFF\x00h\x00i").unwrap(), "hi");
        let latin = b"<?xml version=\"1.0\" encoding=\"ISO-8859-1\"?><p>caf\xe9</p>";
        assert!(decode(latin).unwrap().ends_with("caf\u{e9}</p>"));
        let cp = b"<?xml version='1.0' encoding='windows-1252'?><p>\x93q\x94</p>";
        assert!(decode(cp).unwrap().contains("\u{201c}q\u{201d}"));
        let other = b"<?xml version=\"1.0\" encoding=\"Shift_JIS\"?><p/>";
        assert_eq!(decode(other).unwrap_err().kind, WireErrorKind::Encoding);
    }
}
