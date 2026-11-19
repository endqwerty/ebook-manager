//! Turn one XHTML chapter into a strict subset of XHTML for a web view.
//!
//! Nothing is copied from the source. Every element, attribute and character
//! of the output is written here from tokens, so the output is well-formed
//! XML whatever the source was, and holds no script, no handler, no address
//! outside the book and no link a click can follow out of it.

use super::text::{css_is_remote, hidden, is_remote};
use super::xml::{attr, attr_local, tokenize, Tok};
use super::{clean_inline, truncate_chars};
use crate::wire::{MARKUP_ATTRIBUTES, MARKUP_ELEMENTS};

pub struct Sanitised {
    /// Children of `<body>`.
    pub markup: String,
    /// UTF-16 code units of text written to `markup`.
    pub length: u32,
    pub head_title: Option<String>,
    pub first_heading: Option<String>,
    pub language: Option<String>,
    pub right_to_left: bool,
    pub truncated: bool,
    pub malformed: bool,
    pub remote_references: bool,
    /// Inline SVG or MathML was left out.
    pub unsupported: bool,
}

/// Where a hyperlink leads, as the caller resolved it.
pub enum Href {
    /// A reading-order item and an optional anchor in it.
    Chapter(u32, Option<String>),
    /// An `http`, `https` or `mailto` address.
    External(String),
}

pub struct Resolver<'a> {
    pub link: &'a dyn Fn(&str) -> Option<Href>,
    /// The manifest position of a picture this build can serve.
    pub picture: &'a dyn Fn(&str) -> Option<u32>,
}

const MAX_DEPTH: usize = 400;
const MAX_VALUE_CHARS: usize = 1024;
/// An `id` or a link's anchor longer than this is left out whole. Cutting
/// one would make two long names that share a start into one target.
const MAX_ANCHOR_CHARS: usize = 256;

/// Written as elements: `MARKUP_ELEMENTS`. Everything else is left out and
/// its content kept, unless it is in `DROPPED`.
/// Written with no content and no end tag of their own.
const EMPTY: [&str; 5] = ["br", "hr", "img", "col", "wbr"];
/// Left out with everything inside them.
const DROPPED: [&str; 22] = [
    "script", "style", "template", "iframe", "frame", "frameset", "object", "embed", "applet",
    "audio", "video", "canvas", "svg", "math", "input", "button", "select", "textarea", "datalist",
    "noscript", "head", "title",
];

fn kept(name: &str) -> Option<&'static str> {
    MARKUP_ELEMENTS.iter().find(|k| **k == name).copied()
}

struct Skip {
    name: String,
    depth: u32,
    /// For `svg`: picture references seen and other drawn elements seen.
    images: Vec<String>,
    others: u32,
    /// The element was hidden, so nothing stands in for it.
    hidden: bool,
    /// `style` elements open inside it.
    styles: u32,
}

struct Writer<'a> {
    resolve: &'a Resolver<'a>,
    max_nodes: usize,
    out: String,
    length: u32,
    nodes: usize,
    open: Vec<&'static str>,
    skip: Option<Skip>,
    in_body: bool,
    in_title: bool,
    title: String,
    heading: Option<String>,
    in_heading: u32,
    heading_done: bool,
    language: Option<String>,
    right_to_left: bool,
    remote: bool,
    unsupported: bool,
    truncated: bool,
}

pub fn sanitise(src: &str, max_nodes: usize, resolve: &Resolver<'_>) -> Sanitised {
    let mut w = Writer {
        resolve,
        max_nodes,
        out: String::new(),
        length: 0,
        nodes: 0,
        open: Vec::new(),
        skip: None,
        in_body: false,
        in_title: false,
        title: String::new(),
        heading: None,
        in_heading: 0,
        heading_done: false,
        language: None,
        right_to_left: false,
        remote: false,
        unsupported: false,
        truncated: false,
    };
    let clean = tokenize(src, true, |tok| {
        match tok {
            Tok::Start { name, attrs } => w.start(&name, &attrs),
            Tok::End { name } => w.end(&name),
            Tok::Text(text) => w.text(&text),
        }
        !w.truncated
    });
    while let Some(name) = w.open.pop() {
        close(&mut w.out, name);
    }
    let title = clean_inline(&w.title);
    Sanitised {
        markup: w.out,
        length: w.length,
        head_title: Some(title).filter(|t| !t.is_empty()),
        first_heading: w
            .heading
            .map(|h| clean_inline(&h))
            .filter(|h| !h.is_empty()),
        language: w.language,
        right_to_left: w.right_to_left,
        truncated: w.truncated,
        malformed: !clean,
        remote_references: w.remote,
        unsupported: w.unsupported,
    }
}

fn close(out: &mut String, name: &str) {
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

/// A character XML 1.0 allows that is not a control other than tab and line
/// feed, and not the byte-order mark.
fn allowed(c: char) -> bool {
    match c {
        '\t' | '\n' => true,
        '\u{feff}' | '\u{fffe}' | '\u{ffff}' => false,
        c => !c.is_control(),
    }
}

/// Write text content. Returns the UTF-16 code units written.
fn write_text(out: &mut String, text: &str) -> u32 {
    let mut units = 0u32;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let c = match c {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    continue;
                }
                '\n'
            }
            c => c,
        };
        if !allowed(c) {
            continue;
        }
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            c => out.push(c),
        }
        units += c.len_utf16() as u32;
    }
    units
}

/// Write an attribute with at most `limit` characters of its value. `>` is
/// escaped too, so a `>` in the markup always ends a tag.
fn write_attr(out: &mut String, name: &str, value: &str, limit: usize) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    for c in value.chars().filter(|c| allowed(*c)).take(limit) {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' => out.push(' '),
            c => out.push(c),
        }
    }
    out.push('"');
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

impl Writer<'_> {
    fn note_remote(&mut self, name: &str, attrs: &[(String, String)]) {
        if self.remote {
            return;
        }
        // A hyperlink's own address is a place to go, not something to load.
        let hyperlink = name == "a" || name == "area";
        self.remote =
            attrs
                .iter()
                .any(|(key, value)| match key.rsplit(':').next().unwrap_or(key) {
                    "href" if hyperlink => false,
                    "src" | "href" | "poster" | "data" => is_remote(value),
                    "srcset" => value.split([',', ' ', '\t', '\n']).any(is_remote),
                    "style" => css_is_remote(value),
                    _ => false,
                });
    }

    fn count(&mut self) -> bool {
        if self.nodes >= self.max_nodes {
            self.truncated = true;
            return false;
        }
        self.nodes += 1;
        true
    }

    fn start(&mut self, name: &str, attrs: &[(String, String)]) {
        self.note_remote(name, attrs);
        if let Some(skip) = &mut self.skip {
            if name == skip.name {
                skip.depth += 1;
            }
            if skip.name == "svg" {
                match name {
                    "image" => skip
                        .images
                        .extend(attr_local(attrs, "href").map(str::to_owned)),
                    "svg" | "g" | "title" | "desc" | "defs" => {}
                    _ => skip.others += 1,
                }
            }
            if skip.name == "head" && name == "title" {
                self.in_title = true;
            }
            if name == "style" {
                skip.styles += 1;
            }
            return;
        }
        let concealed = hidden(attrs);
        match name {
            "html" | "body" if !concealed => {
                if self.language.is_none() {
                    self.language = attr(attrs, "xml:lang")
                        .or_else(|| attr(attrs, "lang"))
                        .map(clean_inline)
                        .filter(|l| !l.is_empty())
                        .map(|mut l| {
                            truncate_chars(&mut l, 64);
                            l
                        });
                }
                if attr(attrs, "dir").is_some_and(|d| d.trim().eq_ignore_ascii_case("rtl")) {
                    self.right_to_left = true;
                }
                if name == "body" {
                    self.in_body = true;
                }
                return;
            }
            _ => {}
        }
        if DROPPED.contains(&name) || concealed {
            self.skip = Some(Skip {
                name: name.to_owned(),
                depth: 1,
                images: Vec::new(),
                others: 0,
                hidden: concealed,
                styles: u32::from(name == "style"),
            });
            return;
        }
        let Some(name) = kept(name) else {
            return;
        };
        self.in_body = true;
        if name == "img" {
            self.picture(attr(attrs, "src"), attr(attrs, "alt"), attrs);
            return;
        }
        if self.open.len() >= MAX_DEPTH || !self.count() {
            return;
        }
        self.out.push('<');
        self.out.push_str(name);
        self.attributes(attrs);
        if name == "a" {
            match attr(attrs, "href").and_then(|h| (self.resolve.link)(h)) {
                Some(Href::Chapter(chapter, anchor)) => {
                    let mut href = format!("/c/{chapter}");
                    let anchor = anchor.filter(|a| a.chars().count() <= MAX_ANCHOR_CHARS);
                    if let Some(anchor) = anchor {
                        href.push('#');
                        href.push_str(&percent_encode(&anchor));
                    }
                    write_attr(&mut self.out, "href", &href, usize::MAX);
                }
                Some(Href::External(address)) => {
                    write_attr(&mut self.out, "data-external", &address, usize::MAX)
                }
                None => {}
            }
        }
        if EMPTY.contains(&name) {
            self.out.push_str("/>");
            return;
        }
        self.out.push('>');
        self.open.push(name);
        if matches!(name, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") && !self.heading_done {
            self.in_heading += 1;
            if self.heading.is_none() {
                self.heading = Some(String::new());
            }
        }
    }

    fn attributes(&mut self, attrs: &[(String, String)]) {
        let mut seen: Vec<&str> = Vec::new();
        for (key, value) in attrs {
            let name = match key.as_str() {
                "xml:lang" => "lang",
                "epub:type" => "data-epub-type",
                // Only `epub:type` becomes `data-epub-type`; a book's own
                // attribute of that name is not copied.
                "data-epub-type" => continue,
                k => match MARKUP_ATTRIBUTES.iter().find(|a| **a == k) {
                    Some(a) => a,
                    None => continue,
                },
            };
            if seen.contains(&name) {
                continue;
            }
            seen.push(name);
            if name == "id" && value.chars().count() > MAX_ANCHOR_CHARS {
                continue;
            }
            write_attr(&mut self.out, name, value, MAX_VALUE_CHARS);
        }
    }

    /// An `img`, or its text alternative when the picture cannot be served.
    fn picture(&mut self, src: Option<&str>, alt: Option<&str>, attrs: &[(String, String)]) {
        // With the nesting limit reached a picture is left out like any other
        // element, and its words stand in for it.
        let room = self.open.len() < MAX_DEPTH;
        match src.and_then(|s| (self.resolve.picture)(s)).filter(|_| room) {
            Some(id) => {
                if !self.count() {
                    return;
                }
                self.out.push_str("<img");
                write_attr(&mut self.out, "src", &format!("/r/{id}"), usize::MAX);
                // `alt` is always written, so a missing picture has a place
                // for its words and a decorative one is skipped when read out.
                if attr(attrs, "alt").is_none() {
                    write_attr(&mut self.out, "alt", "", 0);
                }
                self.attributes(attrs);
                self.out.push_str("/>");
            }
            None => {
                if let Some(alt) = alt {
                    self.text(alt);
                }
            }
        }
    }

    fn end(&mut self, name: &str) {
        if let Some(skip) = &mut self.skip {
            if skip.name == "head" && name == "title" {
                self.in_title = false;
            }
            if name == "style" {
                skip.styles = skip.styles.saturating_sub(1);
            }
            if name != skip.name {
                return;
            }
            skip.depth -= 1;
            if skip.depth > 0 {
                return;
            }
            let skip = self.skip.take().expect("checked above");
            match skip.name.as_str() {
                _ if skip.hidden => {}
                "svg" => {
                    let wrapped = (skip.others == 0 && skip.images.len() == 1)
                        .then(|| (self.resolve.picture)(&skip.images[0]))
                        .flatten();
                    match wrapped {
                        Some(_) => {
                            self.in_body = true;
                            self.picture(Some(&skip.images[0]), None, &[]);
                        }
                        None => self.unsupported = true,
                    }
                }
                "math" => self.unsupported = true,
                _ => {}
            }
            return;
        }
        let Some(name) = kept(name) else {
            return;
        };
        let Some(at) = self.open.iter().rposition(|o| *o == name) else {
            return;
        };
        while self.open.len() > at {
            let closing = self.open.pop().expect("length checked");
            if matches!(closing, "h1" | "h2" | "h3" | "h4" | "h5" | "h6") && self.in_heading > 0 {
                self.in_heading -= 1;
                // The title is the first heading alone.
                self.heading_done = self.in_heading == 0;
            }
            close(&mut self.out, closing);
        }
    }

    fn text(&mut self, text: &str) {
        if let Some(skip) = &self.skip {
            if self.in_title && self.title.len() < 4096 {
                self.title.push_str(text);
            }
            if skip.styles > 0 && !self.remote {
                self.remote = css_is_remote(text);
            }
            return;
        }
        if !self.in_body {
            if text.chars().all(char::is_whitespace) {
                return;
            }
            self.in_body = true;
        }
        if !self.count() {
            return;
        }
        self.length += write_text(&mut self.out, text);
        if self.in_heading > 0 {
            if let Some(h) = self.heading.as_mut().filter(|h| h.len() < 4096) {
                h.push_str(text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quick_xml::events::Event;

    fn run(src: &str) -> Sanitised {
        run_limited(src, usize::MAX)
    }

    fn run_limited(src: &str, max: usize) -> Sanitised {
        let link = |href: &str| match href {
            h if h.starts_with("https://") || h.starts_with("mailto:") => {
                Some(Href::External(h.to_owned()))
            }
            h if h.starts_with("ch2.xhtml") => Some(Href::Chapter(
                1,
                h.split_once('#').map(|(_, f)| f.to_owned()),
            )),
            _ => None,
        };
        let picture = |src: &str| (src == "cover.png").then_some(7);
        sanitise(
            src,
            max,
            &Resolver {
                link: &link,
                picture: &picture,
            },
        )
    }

    /// Parse strictly and return every element name, attribute name and
    /// attribute value, plus the UTF-16 length of the text.
    fn parse(markup: &str) -> (Vec<String>, Vec<(String, String)>, u32) {
        let doc = format!("<body>{markup}</body>");
        let mut reader = quick_xml::Reader::from_str(&doc);
        let (mut names, mut attrs, mut length) = (Vec::new(), Vec::new(), 0u32);
        loop {
            match reader.read_event().expect("the markup is well-formed XML") {
                Event::Eof => break,
                Event::Start(e) | Event::Empty(e) => {
                    names.push(String::from_utf8(e.name().as_ref().to_vec()).unwrap());
                    for a in e.attributes() {
                        let a = a.expect("attributes are well-formed and not repeated");
                        attrs.push((
                            String::from_utf8(a.key.as_ref().to_vec()).unwrap(),
                            a.unescape_value().unwrap().into_owned(),
                        ));
                    }
                }
                Event::Text(t) => {
                    length += t.unescape().unwrap().encode_utf16().count() as u32;
                }
                _ => {}
            }
        }
        names.retain(|n| n != "body");
        (names, attrs, length)
    }

    #[test]
    fn the_design_example() {
        let s = run(
            r#"<html><body><p onclick="x()">See <a href="ch2.xhtml#n1">note</a> and <img src="http://t.example/p.png" alt="pixel"/><script>fetch('http://t.example')</script></p></body></html>"#,
        );
        assert_eq!(
            s.markup,
            r#"<p>See <a href="/c/1#n1">note</a> and pixel</p>"#
        );
        assert_eq!(s.length, 18);
        assert!(s.remote_references);
    }

    #[test]
    fn misnested_repeated_and_prefixed_markup_comes_out_well_formed() {
        let s = run(
            r#"<body><div><P CLASS="a" class="b">One<br>two <svg:script>x</svg:script><b><i>three</b> four</i><a href="https://example.org/x">site</a></div><img src="cover.svg" alt="Cover"></body>"#,
        );
        assert_eq!(
            s.markup,
            r#"<div><p class="a">One<br/>two <b><i>three</i></b> four<a data-external="https://example.org/x">site</a></p></div>Cover"#
        );
        let (_, _, length) = parse(&s.markup);
        assert_eq!(s.length, length);
        assert_eq!(
            s.length,
            "Onetwo three foursiteCover".encode_utf16().count() as u32
        );
    }

    #[test]
    fn only_the_allow_list_is_written() {
        let s = run(
            r#"<html xml:lang="fr" dir="RTL"><head><title>Head  title</title><style>p{background:url(http://x.example/a.png)}</style><link rel="stylesheet" href="s.css"/></head>
            <body onload="x()"><section epub:type="chapter" id="s1" style="color:red" data-x="1" onclick="y()">
            <h1>Heading <em>one</em></h1>
            <form action="http://x.example/"><fieldset><label>Kept words</label><input value="gone"/><button>gone</button></fieldset></form>
            <iframe src="a.html">gone</iframe><object data="x">gone</object><video src="v.mp4">gone</video>
            <p hidden="hidden">gone</p><p style="display : none">gone</p>
            <custom-element>kept <unknown>too</unknown></custom-element>
            <a href="javascript:alert(1)">js</a><a href="file:///etc/passwd">file</a><a href="mailto:a@example.org">mail</a>
            <img src="cover.png" alt="A cover" srcset="http://x.example/2x.png 2x" onerror="z()"/>
            <table><tr><td colspan="2" bgcolor="red">cell</td></tr></table>
            <math><mi>x</mi></math>
            </section></body></html>"#,
        );
        let (names, attrs, length) = parse(&s.markup);
        for name in &names {
            assert!(kept(name).is_some(), "{name} is not on the allow-list");
        }
        for (key, value) in &attrs {
            assert!(
                MARKUP_ATTRIBUTES.contains(&key.as_str())
                    || ["href", "src", "data-external"].contains(&key.as_str()),
                "{key} is not an allowed attribute"
            );
            match key.as_str() {
                "href" => assert!(value.starts_with("/c/"), "{value}"),
                "src" => assert!(value.starts_with("/r/"), "{value}"),
                _ => {}
            }
        }
        assert!(!s.markup.contains("gone"));
        assert!(!s.markup.contains("javascript"));
        assert!(!s.markup.contains("file:"));
        assert!(s.markup.contains("Kept words"));
        assert!(s.markup.contains("kept too"));
        assert!(s
            .markup
            .contains(r#"<a data-external="mailto:a@example.org">mail</a>"#));
        assert!(s.markup.contains(r#"<img src="/r/7" alt="A cover"/>"#));
        assert!(s
            .markup
            .contains(r#"<section data-epub-type="chapter" id="s1">"#));
        assert!(s.markup.contains(r#"<td colspan="2">cell</td>"#));
        assert_eq!(s.length, length);
        assert_eq!(s.head_title.as_deref(), Some("Head title"));
        assert_eq!(s.first_heading.as_deref(), Some("Heading one"));
        assert_eq!(s.language.as_deref(), Some("fr"));
        assert!(s.right_to_left);
        assert!(s.remote_references);
        assert!(s.unsupported);
    }

    #[test]
    fn length_counts_utf16_units_of_the_text_written() {
        let s = run("<body><p>a\u{1F600}b &amp; &lt;c&gt;\r\nd\re\u{0}\u{feff}\u{b}</p></body>");
        assert_eq!(s.markup, "<p>a\u{1F600}b &amp; &lt;c&gt;\nd\ne</p>");
        assert_eq!(
            s.length,
            "a\u{1F600}b & <c>\nd\ne".encode_utf16().count() as u32
        );
        assert_eq!(parse(&s.markup).2, s.length);
    }

    #[test]
    fn a_cover_wrapped_in_svg_becomes_its_picture() {
        let s = run(
            r#"<body><svg viewBox="0 0 1 1"><image width="1" height="1" xlink:href="cover.png"/></svg></body>"#,
        );
        assert_eq!(s.markup, r#"<img src="/r/7" alt=""/>"#);
        assert!(!s.unsupported);
        let s = run(r#"<body><svg><image href="cover.png"/><text>Title</text></svg>after</body>"#);
        assert_eq!(s.markup, "after");
        assert!(s.unsupported);
    }

    #[test]
    fn attribute_values_cannot_break_out() {
        let s =
            run(r#"<body><p title="a&quot; onclick=&quot;x()" id="&lt;b&gt;&amp;">t</p></body>"#);
        let (names, attrs, _) = parse(&s.markup);
        assert_eq!(names, ["p"]);
        assert_eq!(
            attrs,
            [
                ("title".to_owned(), "a\" onclick=\"x()".to_owned()),
                ("id".to_owned(), "<b>&".to_owned())
            ]
        );
    }

    #[test]
    fn a_cut_chapter_is_still_well_formed() {
        let src = format!(
            "<body><div><ul>{}</ul></div></body>",
            "<li><b>x</b></li>".repeat(100)
        );
        let s = run_limited(&src, 50);
        assert!(s.truncated);
        let (names, _, length) = parse(&s.markup);
        assert!(names.len() <= 50);
        assert_eq!(s.length, length);
        assert!(s.markup.ends_with("</ul></div>"));
    }

    #[test]
    fn deep_nesting_is_flattened_and_unclosed_dropped_content_ends_the_text() {
        let s = run(&format!("<body>{}deep</body>", "<div>".repeat(1000)));
        let (names, _, _) = parse(&s.markup);
        assert_eq!(names.len(), MAX_DEPTH);
        assert!(s.markup.contains("deep"));
        let s = run("<body><p>before</p><script>never closed<p>after</p></body>");
        assert_eq!(s.markup, "<p>before</p>");
    }

    #[test]
    fn anchors_are_percent_encoded() {
        let s = run(r#"<body><a href="ch2.xhtml#a b&quot;c">x</a></body>"#);
        assert_eq!(s.markup, r#"<a href="/c/1#a%20b%22c">x</a>"#);
    }

    #[test]
    fn a_picture_past_the_nesting_limit_is_its_words() {
        let s = run(&format!(
            "<body>{}<img src=\"cover.png\" alt=\"Deep\"/><svg><image href=\"cover.png\"/></svg></body>",
            "<div>".repeat(MAX_DEPTH)
        ));
        let (names, _, _) = parse(&s.markup);
        assert_eq!(names.len(), MAX_DEPTH);
        assert!(!s.markup.contains("<img"));
        assert!(s.markup.contains("Deep"));
    }

    #[test]
    fn a_hidden_body_and_a_hidden_cover_are_left_out() {
        let s = run(r#"<html><body hidden=""><p>concealed</p></body></html>"#);
        assert_eq!(s.markup, "");
        let s = run(r#"<html hidden="hidden"><body><p>concealed</p></body></html>"#);
        assert_eq!(s.markup, "");
        let s =
            run(r#"<body><svg style="display:none"><image href="cover.png"/></svg>shown</body>"#);
        assert_eq!(s.markup, "shown");
        assert!(!s.unsupported);
    }

    #[test]
    fn remote_styles_in_the_head_are_noticed() {
        let s = run("<html><head><style>p {background:url(https://example.invalid/a)}</style></head><body><p>x</p></body></html>");
        assert!(s.remote_references);
        let s = run("<html><head><style>p {background:url(a.png)}</style></head><body><p>x</p></body></html>");
        assert!(!s.remote_references);
    }

    #[test]
    fn a_long_anchor_is_whole_or_absent() {
        let attrs_for = |name: &str| {
            let s = run(&format!(
                r#"<body><p id="{name}">t</p><a href="ch2.xhtml#{name}">x</a></body>"#
            ));
            parse(&s.markup).1
        };
        // At the limit the id and the link are whole and match.
        let name = "\u{1F600}".repeat(MAX_ANCHOR_CHARS);
        let attrs = attrs_for(&name);
        let encoded: String = name.bytes().map(|b| format!("%{b:02X}")).collect();
        assert_eq!(
            attrs,
            [
                ("id".to_owned(), name.clone()),
                ("href".to_owned(), format!("/c/1#{encoded}"))
            ]
        );
        // Past it there is no id, and the link leads to the chapter. Cutting
        // would make two long names that share a start into one target.
        let attrs = attrs_for(&format!("{name}x"));
        assert_eq!(attrs, [("href".to_owned(), "/c/1".to_owned())]);
    }

    #[test]
    fn the_title_is_the_first_heading_only() {
        let s = run("<body><h1>First</h1><p>para</p><h2>Second</h2></body>");
        assert_eq!(s.first_heading.as_deref(), Some("First"));
        let s = run("<body><title>body title</title><p>x</p></body>");
        assert_eq!(s.markup, "<p>x</p>");
    }

    #[test]
    fn a_greater_than_sign_in_a_value_is_escaped() {
        let s = run(r#"<body><p title="a>b">t</p></body>"#);
        assert_eq!(s.markup, r#"<p title="a&gt;b">t</p>"#);
    }
}
