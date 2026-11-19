//! Resolve a reference written inside one container file to another.

/// A reference that stays inside the book.
#[derive(Debug, PartialEq, Eq)]
pub struct Local {
    /// Container path with `/` separators and no leading slash.
    pub path: String,
    pub fragment: Option<String>,
}

/// True when `href` names something outside the container (has a scheme or host).
pub fn is_external(href: &str) -> bool {
    if href.starts_with("//") {
        return true;
    }
    match href.find(':') {
        Some(colon) => {
            let scheme = &href[..colon];
            let mut bytes = scheme.bytes();
            bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
                && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'.' | b'-'))
        }
        None => false,
    }
}

/// Resolve `href`, found in the container file `base`, to a container path.
/// Returns `None` for external references and for paths that climb out.
pub fn resolve(base: &str, href: &str) -> Option<Local> {
    let href = href.trim();
    if is_external(href) {
        return None;
    }
    let (path_part, fragment) = match href.split_once('#') {
        Some((p, f)) => (p, Some(percent_decode(f)).filter(|f| !f.is_empty())),
        None => (href, None),
    };
    let path_part = path_part.split('?').next().unwrap_or("");
    let decoded = percent_decode(path_part);
    let mut parts: Vec<&str> = Vec::new();
    if decoded.is_empty() {
        return Some(Local { path: base.to_owned(), fragment });
    }
    if !decoded.starts_with('/') {
        let mut dirs: Vec<&str> = base.split('/').collect();
        dirs.pop();
        parts = dirs;
    }
    for seg in decoded.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            s => parts.push(s),
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(Local { path: parts.join("/"), fragment })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = |b: u8| (b as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(base: &str, href: &str) -> Option<(String, Option<String>)> {
        resolve(base, href).map(|l| (l.path, l.fragment))
    }

    #[test]
    fn resolves_relative_references_inside_the_container() {
        assert_eq!(p("OEBPS/content.opf", "text/ch1.xhtml"), Some(("OEBPS/text/ch1.xhtml".into(), None)));
        assert_eq!(p("OEBPS/text/ch1.xhtml", "../nav.xhtml#toc"), Some(("OEBPS/nav.xhtml".into(), Some("toc".into()))));
        assert_eq!(p("OEBPS/a.xhtml", "#note%201"), Some(("OEBPS/a.xhtml".into(), Some("note 1".into()))));
        assert_eq!(p("content.opf", "My%20Chapter.xhtml"), Some(("My Chapter.xhtml".into(), None)));
        assert_eq!(p("a/b.opf", "/c/d.xhtml"), Some(("c/d.xhtml".into(), None)));
        assert_eq!(p("a/b.opf", "./c.xhtml?x=1"), Some(("a/c.xhtml".into(), None)));
        assert_eq!(p("a/b.opf", "100%.xhtml"), Some(("a/100%.xhtml".into(), None)));
    }

    #[test]
    fn refuses_references_that_leave_the_container() {
        assert_eq!(p("content.opf", "../outside.xhtml"), None);
        assert_eq!(p("a/b.opf", "../../etc/passwd"), None);
        assert_eq!(p("a/b.opf", "http://example.com/x"), None);
        assert_eq!(p("a/b.opf", "//example.com/x"), None);
        assert_eq!(p("a/b.opf", "file:///etc/passwd"), None);
        assert_eq!(p("a/b.opf", "mailto:someone@example.com"), None);
    }
}
