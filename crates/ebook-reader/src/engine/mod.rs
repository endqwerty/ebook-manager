//! Helper-side parsing. Runs only inside the sandbox, on untrusted bytes.
//! Designed from the EPUB OCF, Packages and Navigation specifications.

pub mod archive;
pub mod cover;
pub mod encoding;
pub mod entities;
pub mod isolation;
pub mod markup;
#[cfg(target_os = "macos")]
pub mod memory;
pub mod package;
pub mod paths;
pub mod resource;
pub mod run;
pub mod survey;
pub mod text;
pub mod toc;
pub mod xml;

/// Collapse whitespace runs to one space, drop control characters and trim.
pub fn clean_inline(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut pending = false;
    for c in s.chars() {
        if c.is_whitespace() && c != '\u{a0}' {
            pending = !out.is_empty();
        } else if !is_dropped(c) {
            if pending {
                out.push(' ');
                pending = false;
            }
            out.push(c);
        }
    }
    out
}

/// Characters that never reach the host: controls and the byte-order mark.
pub fn is_dropped(c: char) -> bool {
    c.is_control() || c == '\u{feff}'
}

pub fn truncate_chars(s: &mut String, max: usize) {
    if let Some((at, _)) = s.char_indices().nth(max) {
        s.truncate(at);
    }
}

/// A reading-order item this build reads as text.
pub fn is_text_document(media_type: &str, path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(media_type, "application/xhtml+xml" | "text/html")
        || [".xhtml", ".html", ".htm", ".xml"]
            .iter()
            .any(|ext| lower.ends_with(ext))
}
