//! A forgiving reader over quick-xml. It never loads a DTD or an external
//! entity, treats names without their namespace prefix and survives the
//! unbalanced markup found in real books.

use super::entities;
use quick_xml::events::{BytesStart, Event};

pub enum Tok {
    Start {
        name: String,
        attrs: Vec<(String, String)>,
    },
    End {
        name: String,
    },
    Text(String),
}

const VOID: [&str; 14] = [
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Feed tokens to `f` until the document ends or `f` returns `false`.
/// `html` gives HTML's empty elements their implied end. Returns `false` when
/// the markup was broken somewhere.
pub fn tokenize(src: &str, html: bool, mut f: impl FnMut(Tok) -> bool) -> bool {
    let mut reader = quick_xml::Reader::from_str(src);
    {
        let config = reader.config_mut();
        config.check_end_names = false;
        config.allow_unmatched_ends = true;
        config.check_comments = false;
    }
    let mut clean = true;
    let mut errors = 0u32;
    let mut last_error_at = u64::MAX;
    loop {
        match reader.read_event() {
            Ok(Event::Eof) => break,
            Ok(Event::Start(e)) => {
                let name = local_name(e.name().as_ref());
                let void = html && VOID.contains(&name.as_str());
                if !f(Tok::Start {
                    name: name.clone(),
                    attrs: attributes(&e),
                }) {
                    break;
                }
                if void && !f(Tok::End { name }) {
                    break;
                }
            }
            Ok(Event::Empty(e)) => {
                let name = local_name(e.name().as_ref());
                if !f(Tok::Start {
                    name: name.clone(),
                    attrs: attributes(&e),
                }) || !f(Tok::End { name })
                {
                    break;
                }
            }
            Ok(Event::End(e)) => {
                let name = local_name(e.name().as_ref());
                if html && VOID.contains(&name.as_str()) {
                    continue;
                }
                if !f(Tok::End { name }) {
                    break;
                }
            }
            Ok(Event::Text(t)) => {
                let raw = String::from_utf8_lossy(&t);
                if !f(Tok::Text(entities::unescape(&raw))) {
                    break;
                }
            }
            Ok(Event::CData(c)) => {
                if !f(Tok::Text(String::from_utf8_lossy(&c).into_owned())) {
                    break;
                }
            }
            Ok(_) => {}
            Err(_) => {
                clean = false;
                errors += 1;
                let at = reader.buffer_position();
                if at == last_error_at || errors > 1000 {
                    break;
                }
                last_error_at = at;
            }
        }
    }
    clean
}

fn local_name(raw: &[u8]) -> String {
    let local = raw.rsplit(|b| *b == b':').next().unwrap_or(raw);
    String::from_utf8_lossy(local).to_ascii_lowercase()
}

fn attributes(e: &BytesStart<'_>) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for attr in e.html_attributes().with_checks(false).flatten() {
        if out.len() >= 64 {
            break;
        }
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_ascii_lowercase();
        let value = entities::unescape(&String::from_utf8_lossy(&attr.value));
        out.push((key, value));
    }
    out
}

/// Find an attribute by its name without namespace prefix.
pub fn attr_local<'a>(attrs: &'a [(String, String)], local: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k.rsplit(':').next() == Some(local))
        .map(|(_, v)| v.as_str())
}

/// Find an attribute by its exact written name.
pub fn attr<'a>(attrs: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attrs
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

#[derive(Debug, Default)]
pub struct Element {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub children: Vec<Node>,
}

#[derive(Debug)]
pub enum Node {
    Element(Element),
    Text(String),
}

const MAX_DEPTH: usize = 200;
const MAX_NODES: usize = 400_000;

/// Build a small tree for package and navigation documents.
/// The returned root is a holder whose children are the top-level nodes.
pub fn parse_tree(src: &str) -> Element {
    let mut stack: Vec<Element> = vec![Element::default()];
    let mut overflow = 0usize;
    let mut nodes = 0usize;
    tokenize(src, false, |tok| {
        nodes += 1;
        if nodes > MAX_NODES {
            return false;
        }
        match tok {
            Tok::Start { name, attrs } => {
                if overflow > 0 || stack.len() >= MAX_DEPTH {
                    overflow += 1;
                } else {
                    stack.push(Element {
                        name,
                        attrs,
                        children: Vec::new(),
                    });
                }
            }
            Tok::End { name } => {
                if overflow > 0 {
                    overflow -= 1;
                } else if let Some(at) = stack
                    .iter()
                    .rposition(|e| e.name == name)
                    .filter(|at| *at > 0)
                {
                    while stack.len() > at {
                        close_top(&mut stack);
                    }
                }
            }
            Tok::Text(text) => {
                if let Some(top) = stack.last_mut() {
                    top.children.push(Node::Text(text));
                }
            }
        }
        true
    });
    while stack.len() > 1 {
        close_top(&mut stack);
    }
    stack.pop().unwrap_or_default()
}

fn close_top(stack: &mut Vec<Element>) {
    if let Some(done) = stack.pop() {
        if let Some(parent) = stack.last_mut() {
            parent.children.push(Node::Element(done));
        }
    }
}

impl Element {
    pub fn attr_local(&self, local: &str) -> Option<&str> {
        attr_local(&self.attrs, local)
    }

    pub fn attr(&self, name: &str) -> Option<&str> {
        attr(&self.attrs, name)
    }

    pub fn elements(&self) -> impl Iterator<Item = &Element> {
        self.children.iter().filter_map(|n| match n {
            Node::Element(e) => Some(e),
            Node::Text(_) => None,
        })
    }

    pub fn child(&self, name: &str) -> Option<&Element> {
        self.elements().find(|e| e.name == name)
    }

    /// The first element with this name, searching children before going deeper
    /// is not needed here: document order is what callers want.
    pub fn find(&self, name: &str) -> Option<&Element> {
        for e in self.elements() {
            if e.name == name {
                return Some(e);
            }
            if let Some(found) = e.find(name) {
                return Some(found);
            }
        }
        None
    }

    /// All text inside this element, cleaned for display on one line.
    pub fn text(&self) -> String {
        let mut raw = String::new();
        self.collect_text(&mut raw);
        super::clean_inline(&raw)
    }

    fn collect_text(&self, out: &mut String) {
        for node in &self.children {
            match node {
                Node::Text(t) => out.push_str(t),
                Node::Element(e) => e.collect_text(out),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_tree_from_unbalanced_markup() {
        let root = parse_tree("<a><b x='1'>one<c>two</b><d>three</d></a>");
        let a = root.child("a").unwrap();
        assert_eq!(a.child("b").unwrap().attr("x"), Some("1"));
        assert_eq!(a.child("b").unwrap().text(), "onetwo");
        assert_eq!(a.child("d").unwrap().text(), "three");
    }

    #[test]
    fn ignores_doctype_entity_definitions() {
        let src = "<!DOCTYPE r [<!ENTITY x SYSTEM \"file:///etc/passwd\"><!ENTITY y \"boom\">]><r>&x;&y;</r>";
        let root = parse_tree(src);
        assert_eq!(root.child("r").unwrap().text(), "&x;&y;");
    }

    #[test]
    fn bounds_nesting_depth() {
        let src = "<a>".repeat(5000);
        let root = parse_tree(&src);
        let mut depth = 0;
        let mut cur = &root;
        while let Some(next) = cur.child("a") {
            depth += 1;
            cur = next;
        }
        assert!(depth < MAX_DEPTH);
    }
}
