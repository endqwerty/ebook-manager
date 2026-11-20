//! Turn one XHTML chapter into blocks of styled text.
//!
//! Only text, a few style flags, link targets and anchor names leave this
//! module. Scripts, styles, forms, embedded objects and remote references are
//! never interpreted and never passed on.

use super::xml::{attr, tokenize, Tok};
use super::{is_dropped, truncate_chars};
use crate::model::{Block, BlockKind, Link, Span};

pub struct Extracted {
    pub blocks: Vec<Block>,
    pub head_title: Option<String>,
    pub truncated: bool,
    pub malformed: bool,
    /// The document asked for something from a remote address. Nothing was
    /// fetched; the caller can record that the book tried.
    pub remote_references: bool,
}

const MAX_DEPTH: usize = 400;
const MAX_ANCHORS: usize = 100_000;

const BLOCK_TAGS: [&str; 47] = [
    "address",
    "article",
    "aside",
    "blockquote",
    "body",
    "caption",
    "center",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "html",
    "li",
    "main",
    "menu",
    "nav",
    "ol",
    "p",
    "pre",
    "section",
    "summary",
    "table",
    "tbody",
    "tfoot",
    "thead",
    "tr",
    "ul",
    "legend",
    "optgroup",
    "option",
];
/// Elements whose content is never reading text.
const SKIP_TAGS: [&str; 9] = [
    "script", "style", "template", "svg", "iframe", "object", "audio", "video", "canvas",
];

#[derive(Default, Clone, Copy)]
struct Fx {
    block: bool,
    gap: bool,
    row: bool,
    skip: bool,
    head: bool,
    title: bool,
    heading: bool,
    list: bool,
    item: bool,
    quote: bool,
    pre: bool,
    bold: bool,
    italic: bool,
    code: bool,
    sup: bool,
    sub: bool,
    link: bool,
}

struct Frame {
    name: String,
    fx: Fx,
}

#[derive(Default)]
struct Current {
    spans: Vec<Span>,
    kind: Option<BlockKind>,
    has_text: bool,
    has_image: bool,
    pending_space: bool,
    line_start: bool,
    /// The block is a table row: its tabs are column separators.
    row: bool,
}

struct Extractor<'a> {
    resolve: &'a dyn Fn(&str) -> Option<Link>,
    max_blocks: usize,
    frames: Vec<Frame>,
    overflow: usize,
    skip: u32,
    head: u32,
    in_title: bool,
    title: String,
    bold: u32,
    italic: u32,
    code: u32,
    sup: u32,
    sub: u32,
    quote: u32,
    pre: u32,
    links: Vec<Option<Link>>,
    headings: Vec<u8>,
    lists: Vec<Option<u32>>,
    item_fresh: Vec<bool>,
    cur: Current,
    in_row: u32,
    row_cells: u32,
    remote: bool,
    anchors: Vec<String>,
    anchor_count: usize,
    blocks: Vec<Block>,
    truncated: bool,
}

pub fn extract(src: &str, max_blocks: usize, resolve: &dyn Fn(&str) -> Option<Link>) -> Extracted {
    let mut x = Extractor {
        resolve,
        max_blocks,
        frames: Vec::new(),
        overflow: 0,
        skip: 0,
        head: 0,
        in_title: false,
        title: String::new(),
        bold: 0,
        italic: 0,
        code: 0,
        sup: 0,
        sub: 0,
        quote: 0,
        pre: 0,
        links: Vec::new(),
        headings: Vec::new(),
        lists: Vec::new(),
        item_fresh: Vec::new(),
        cur: Current::default(),
        in_row: 0,
        row_cells: 0,
        remote: false,
        anchors: Vec::new(),
        anchor_count: 0,
        blocks: Vec::new(),
        truncated: false,
    };
    let clean = tokenize(src, true, |tok| {
        match tok {
            Tok::Start { name, attrs } => x.start(name, &attrs),
            Tok::End { name } => x.end(&name),
            Tok::Text(text) => x.text(&text),
        }
        !x.truncated
    });
    x.flush();
    if let Some(last) = x.blocks.last_mut() {
        last.anchors.append(&mut x.anchors);
    }
    let title = super::clean_inline(&x.title);
    Extracted {
        blocks: x.blocks,
        head_title: Some(title).filter(|t| !t.is_empty()),
        truncated: x.truncated,
        malformed: !clean,
        remote_references: x.remote,
    }
}

pub(super) fn is_remote(reference: &str) -> bool {
    let r = reference.trim_start().to_ascii_lowercase();
    ["http://", "https://", "ftp://", "//"]
        .iter()
        .any(|p| r.starts_with(p))
}

/// Style text that loads something from a remote address: a `url(...)` or
/// an `@import "..."` whose target is remote. Addresses that only appear in
/// comments or other text do not count.
pub(super) fn css_is_remote(css: &str) -> bool {
    let css = css.to_ascii_lowercase();
    let target = |rest: &str, closers: &[char]| -> bool {
        let rest = rest.trim_start();
        let rest = rest.strip_prefix(['"', '\'']).unwrap_or(rest);
        let end = rest.find(closers).unwrap_or(rest.len());
        is_remote(&rest[..end])
    };
    let mut rest = css.as_str();
    while let Some(at) = rest.find("url(") {
        rest = &rest[at + 4..];
        if target(rest, &[')', '"', '\'']) {
            return true;
        }
    }
    let mut rest = css.as_str();
    while let Some(at) = rest.find("@import") {
        rest = &rest[at + 7..];
        if target(rest, &[';', '"', '\'', ' ']) {
            return true;
        }
    }
    false
}

pub(super) fn hidden(attrs: &[(String, String)]) -> bool {
    if attr(attrs, "hidden").is_some() {
        return true;
    }
    attr(attrs, "style").is_some_and(|s| {
        let compact: String = s.chars().filter(|c| !c.is_whitespace()).collect();
        compact.to_ascii_lowercase().contains("display:none")
    })
}

impl Extractor<'_> {
    fn start(&mut self, name: String, attrs: &[(String, String)]) {
        if self.overflow > 0 || self.frames.len() >= MAX_DEPTH {
            self.overflow += 1;
            return;
        }
        let mut fx = Fx::default();
        let n = name.as_str();
        if !self.remote {
            // A hyperlink's own address is a place to go, not something to load.
            let hyperlink = n == "a" || n == "area";
            self.remote = attrs.iter().any(|(key, value)| {
                let key = key.rsplit(':').next().unwrap_or(key);
                match key {
                    "href" if hyperlink => false,
                    "src" | "href" | "poster" | "data" => is_remote(value),
                    "srcset" => value.split([',', ' ', '\t', '\n']).any(is_remote),
                    "style" => css_is_remote(value),
                    _ => false,
                }
            });
        }
        if self.skip > 0 {
            // Inside skipped content: track nesting only.
        } else if self.head > 0 {
            if n == "title" {
                fx.title = true;
                self.in_title = true;
            }
        } else if n == "head" {
            fx.head = true;
            self.head += 1;
        } else if SKIP_TAGS.contains(&n) || hidden(attrs) {
            if n == "svg" {
                self.image("");
            }
            fx.skip = true;
            self.skip += 1;
        } else {
            self.open(n, attrs, &mut fx);
        }
        self.frames.push(Frame { name, fx });
    }

    fn open(&mut self, n: &str, attrs: &[(String, String)], fx: &mut Fx) {
        if BLOCK_TAGS.contains(&n) {
            let table_part = matches!(n, "table" | "thead" | "tbody" | "tfoot" | "tr" | "caption");
            if self.in_row == 0 || table_part {
                self.flush();
                fx.block = true;
            } else {
                // Inside a table cell a paragraph is a gap, not a new block:
                // the row stays on one line, with a space on either side.
                fx.gap = true;
                self.gap();
            }
        }
        if let Some(id) =
            attr(attrs, "id").or_else(|| if n == "a" { attr(attrs, "name") } else { None })
        {
            self.anchor(id);
        }
        match n {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.headings.push(n.as_bytes()[1] - b'0');
                fx.heading = true;
            }
            "ul" | "menu" | "dir" => {
                self.lists.push(None);
                fx.list = true;
            }
            "ol" => {
                let start = attr(attrs, "start")
                    .and_then(|s| s.trim().parse::<u32>().ok())
                    .unwrap_or(1);
                self.lists.push(Some(start.saturating_sub(1)));
                fx.list = true;
            }
            "li" => {
                if let Some(Some(n)) = self.lists.last_mut() {
                    *n = n.saturating_add(1);
                }
                self.item_fresh.push(true);
                fx.item = true;
            }
            "blockquote" => {
                self.quote += 1;
                fx.quote = true;
            }
            "pre" => {
                self.pre += 1;
                fx.pre = true;
            }
            "hr" => {
                self.emit(Block {
                    kind: BlockKind::Separator,
                    spans: Vec::new(),
                    anchors: Vec::new(),
                });
            }
            "tr" => {
                self.row_cells = 0;
                self.in_row += 1;
                fx.row = true;
            }
            "td" | "th" => {
                // One tab before every cell after the first, so an empty
                // cell still holds its column.
                if self.row_cells > 0 {
                    self.cur.pending_space = false;
                    self.push_raw('\t');
                    self.cur.line_start = true;
                }
                self.row_cells += 1;
                self.cur.row = true;
            }
            "br" => {
                if self.pre > 0 {
                    self.push_raw('\n');
                } else if !self.cur.spans.is_empty() {
                    self.cur.pending_space = false;
                    self.push_raw('\n');
                    self.cur.line_start = true;
                }
            }
            "img" => self.image(attr(attrs, "alt").unwrap_or("")),
            "b" | "strong" => {
                self.bold += 1;
                fx.bold = true;
            }
            "i" | "em" | "cite" | "dfn" | "var" => {
                self.italic += 1;
                fx.italic = true;
            }
            "code" | "kbd" | "samp" | "tt" => {
                self.code += 1;
                fx.code = true;
            }
            "sup" => {
                self.sup += 1;
                fx.sup = true;
            }
            "sub" => {
                self.sub += 1;
                fx.sub = true;
            }
            "a" => {
                if let Some(href) = attr(attrs, "href") {
                    self.links.push((self.resolve)(href));
                    fx.link = true;
                }
            }
            _ => {}
        }
    }

    fn end(&mut self, name: &str) {
        if self.overflow > 0 {
            self.overflow -= 1;
            return;
        }
        let Some(at) = self.frames.iter().rposition(|f| f.name == name) else {
            return;
        };
        while self.frames.len() > at {
            let Some(frame) = self.frames.pop() else {
                break;
            };
            self.close(frame.fx);
        }
    }

    fn close(&mut self, fx: Fx) {
        if fx.block {
            self.flush();
        }
        if fx.gap {
            self.gap();
        }
        if fx.row {
            self.in_row -= 1;
        }
        if fx.skip {
            self.skip -= 1;
        }
        if fx.head {
            self.head -= 1;
        }
        if fx.title {
            self.in_title = false;
        }
        if fx.heading {
            self.headings.pop();
        }
        if fx.list {
            self.lists.pop();
        }
        if fx.item {
            self.item_fresh.pop();
        }
        if fx.quote {
            self.quote -= 1;
        }
        if fx.pre {
            self.pre -= 1;
        }
        if fx.bold {
            self.bold -= 1;
        }
        if fx.italic {
            self.italic -= 1;
        }
        if fx.code {
            self.code -= 1;
        }
        if fx.sup {
            self.sup -= 1;
        }
        if fx.sub {
            self.sub -= 1;
        }
        if fx.link {
            self.links.pop();
        }
    }

    fn text(&mut self, text: &str) {
        if self.in_title {
            if self.title.len() < 4000 {
                self.title.push_str(text);
            }
        } else if self.skip > 0 || self.head > 0 {
            if self.frames.last().is_some_and(|f| f.name == "style") && css_is_remote(text) {
                self.remote = true;
            }
        } else if self.pre > 0 {
            for c in text.chars() {
                if c == '\r' || (is_dropped(c) && c != '\n' && c != '\t') {
                    continue;
                }
                self.push_raw(c);
                self.cur.has_text = true;
            }
        } else {
            self.collapsed(text, true);
        }
    }

    /// Add text with whitespace runs folded to one space.
    fn collapsed(&mut self, text: &str, is_text: bool) {
        for c in text.chars() {
            if matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
                if !self.cur.spans.is_empty() && !self.cur.line_start {
                    self.cur.pending_space = true;
                }
                continue;
            }
            if is_dropped(c) {
                continue;
            }
            if self.cur.pending_space {
                self.cur.pending_space = false;
                // The gap goes with the text before it, unless that text is a
                // link that ends here: a link never trails an underlined space.
                let now = self.links.last().cloned().flatten();
                match self.cur.spans.last_mut() {
                    Some(last) if last.link.is_none() || last.link == now => last.text.push(' '),
                    _ => self.push_styled(' ', None),
                }
            }
            self.cur.line_start = false;
            self.push_raw(c);
            self.cur.has_text |= is_text;
        }
    }

    /// A word break where a block boundary was folded into the line.
    fn gap(&mut self) {
        if !self.cur.spans.is_empty() && !self.cur.line_start {
            self.cur.pending_space = true;
        }
    }

    fn image(&mut self, alt: &str) {
        self.cur.has_image = true;
        self.collapsed(alt, false);
    }

    fn anchor(&mut self, id: &str) {
        let id = id.trim();
        if id.is_empty() || self.anchor_count >= MAX_ANCHORS {
            return;
        }
        let mut id = id.to_owned();
        truncate_chars(&mut id, 256);
        self.anchor_count += 1;
        self.anchors.push(id);
    }

    fn kind_now(&mut self) -> BlockKind {
        if self.pre > 0 {
            return BlockKind::Preformatted;
        }
        if let Some(level) = self.headings.last() {
            return BlockKind::Heading {
                level: (*level).clamp(1, 6),
            };
        }
        if let Some(fresh) = self.item_fresh.last_mut() {
            if *fresh {
                *fresh = false;
                return BlockKind::ListItem {
                    depth: self.lists.len().clamp(1, 255) as u8,
                    ordinal: self.lists.last().copied().flatten(),
                };
            }
        }
        if self.quote > 0 {
            BlockKind::Quote
        } else {
            BlockKind::Paragraph
        }
    }

    fn push_raw(&mut self, c: char) {
        let link = self.links.last().cloned().flatten();
        self.push_styled(c, link);
    }

    fn push_styled(&mut self, c: char, link: Option<Link>) {
        if self.cur.kind.is_none() {
            self.cur.kind = Some(self.kind_now());
        }
        let (bold, italic, code) = (self.bold > 0, self.italic > 0, self.code > 0);
        let (superscript, subscript) = (self.sup > 0, self.sub > 0);
        if let Some(last) = self.cur.spans.last_mut() {
            if last.bold == bold
                && last.italic == italic
                && last.code == code
                && last.superscript == superscript
                && last.subscript == subscript
                && last.link == link
            {
                last.text.push(c);
                return;
            }
        }
        let mut text = String::new();
        text.push(c);
        self.cur.spans.push(Span {
            text,
            bold,
            italic,
            code,
            superscript,
            subscript,
            link,
        });
    }

    fn flush(&mut self) {
        let mut cur = std::mem::take(&mut self.cur);
        let pre = cur.kind == Some(BlockKind::Preformatted);
        if pre {
            if let Some(first) = cur.spans.first_mut() {
                if first.text.starts_with('\n') {
                    first.text.remove(0);
                }
            }
        }
        while let Some(last) = cur.spans.last_mut() {
            let kept = if pre {
                last.text.trim_end_matches('\n').len()
            } else if cur.row {
                last.text.trim_end_matches(['\n', ' ']).len()
            } else {
                last.text.trim_end_matches(['\n', ' ', '\t']).len()
            };
            last.text.truncate(kept);
            if last.text.is_empty() {
                cur.spans.pop();
            } else {
                break;
            }
        }
        cur.spans.retain(|s| !s.text.is_empty());
        let blank = cur.spans.iter().all(|s| s.text.trim().is_empty());
        if blank && !cur.has_image {
            return;
        }
        let kind = if cur.has_image && !cur.has_text {
            BlockKind::Image
        } else {
            cur.kind.unwrap_or(BlockKind::Paragraph)
        };
        let spans = if blank { Vec::new() } else { cur.spans };
        self.emit(Block {
            kind,
            spans,
            anchors: Vec::new(),
        });
    }

    fn emit(&mut self, mut block: Block) {
        if self.blocks.len() >= self.max_blocks {
            self.truncated = true;
            return;
        }
        block.anchors = std::mem::take(&mut self.anchors);
        self.blocks.push(block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Target;

    fn run(body: &str) -> Extracted {
        let doc = format!(
            "<html><head><title>Doc</title><style>p{{}}</style></head><body>{body}</body></html>"
        );
        extract(&doc, 1000, &|href| {
            if href.starts_with("http") {
                Some(Link::External(href.to_owned()))
            } else {
                Some(Link::Internal(Target {
                    chapter: 7,
                    anchor: None,
                }))
            }
        })
    }

    fn lines(body: &str) -> Vec<String> {
        run(body).blocks.iter().map(Block::text).collect()
    }

    #[test]
    fn collapses_source_whitespace_but_keeps_word_gaps_across_tags() {
        assert_eq!(
            lines("<p>\n   The  quick\n\tbrown <em>fox</em>\n jumps<b> over</b>.  </p>"),
            vec!["The quick brown fox jumps over."]
        );
        assert_eq!(lines("<p>a<span>b</span>c</p>"), vec!["abc"]);
        assert_eq!(
            lines("<p>non&nbsp;breaking &amp; kept</p>"),
            vec!["non\u{a0}breaking & kept"]
        );
    }

    #[test]
    fn splits_blocks_and_classifies_them() {
        let x = run("<h2 id='c1'>One</h2><p>First.</p><div>Loose text<p>Inner</p>tail</div>\
                     <blockquote><p>Quoted</p></blockquote><hr/>\
                     <ul><li>a</li><li><p>b</p><p>more</p></li></ul><ol start='3'><li>c</li><li>d</li></ol>");
        let kinds: Vec<(BlockKind, String)> = x.blocks.iter().map(|b| (b.kind, b.text())).collect();
        assert_eq!(
            kinds,
            vec![
                (BlockKind::Heading { level: 2 }, "One".into()),
                (BlockKind::Paragraph, "First.".into()),
                (BlockKind::Paragraph, "Loose text".into()),
                (BlockKind::Paragraph, "Inner".into()),
                (BlockKind::Paragraph, "tail".into()),
                (BlockKind::Quote, "Quoted".into()),
                (BlockKind::Separator, "".into()),
                (
                    BlockKind::ListItem {
                        depth: 1,
                        ordinal: None
                    },
                    "a".into()
                ),
                (
                    BlockKind::ListItem {
                        depth: 1,
                        ordinal: None
                    },
                    "b".into()
                ),
                (BlockKind::Paragraph, "more".into()),
                (
                    BlockKind::ListItem {
                        depth: 1,
                        ordinal: Some(3)
                    },
                    "c".into()
                ),
                (
                    BlockKind::ListItem {
                        depth: 1,
                        ordinal: Some(4)
                    },
                    "d".into()
                ),
            ]
        );
        assert_eq!(x.blocks[0].anchors, vec!["c1"]);
        assert_eq!(x.head_title.as_deref(), Some("Doc"));
    }

    #[test]
    fn keeps_line_breaks_and_preformatted_spacing() {
        assert_eq!(
            lines("<p>Roses are red,<br/>\n  Violets are blue<br></p>"),
            vec!["Roses are red,\nViolets are blue"]
        );
        let x = run("<pre>\n  let x = 1;\r\n\tlet y = 2;\n</pre>");
        assert_eq!(x.blocks[0].kind, BlockKind::Preformatted);
        assert_eq!(x.blocks[0].text(), "  let x = 1;\n\tlet y = 2;");
    }

    #[test]
    fn carries_styles_and_links_as_data() {
        let x = run("<p>plain <strong>bold <i>both</i></strong> <a href='http://example.com/'>out</a> <a href='ch2.xhtml'>in</a><sup>1</sup></p>");
        let s = &x.blocks[0].spans;
        assert_eq!(s[0].text, "plain ");
        assert!(s[1].bold && !s[1].italic && s[1].text == "bold ");
        assert!(s[2].bold && s[2].italic && s[2].text == "both ");
        let out = s.iter().find(|s| s.text == "out").unwrap();
        assert_eq!(out.link, Some(Link::External("http://example.com/".into())));
        let inn = s.iter().find(|s| s.text == "in").unwrap();
        assert_eq!(
            inn.link,
            Some(Link::Internal(Target {
                chapter: 7,
                anchor: None
            }))
        );
        assert!(s.last().unwrap().superscript);
    }

    #[test]
    fn drops_active_and_hidden_content() {
        let got = lines(
            "<script>alert('x')</script><p onclick='steal()'>Shown</p><style>p{color:red}</style>\
             <p hidden='hidden'>no</p><div style='display: none'>no</div>\
             <iframe src='http://example.com'>no</iframe><object data='x'>no</object>\
             <form><input value='typed'/><button>Press</button></form>",
        );
        assert_eq!(got, vec!["Shown", "Press"]);
        let x = run("<p onclick='steal()'>Shown</p>");
        let json = serde_json::to_string(&x.blocks).unwrap();
        assert!(!json.contains("steal"));
    }

    #[test]
    fn shows_pictures_as_placeholders_with_their_text_alternative() {
        let x = run("<div><img src='http://tracker.example/p.png' alt='A map of the island'/></div>\
                     <p><img src='t.png' alt='T'/>he start</p><svg><title>x</title><image href='c.jpg'/></svg>");
        assert_eq!(x.blocks[0].kind, BlockKind::Image);
        assert_eq!(x.blocks[0].text(), "A map of the island");
        assert_eq!(x.blocks[1].kind, BlockKind::Paragraph);
        assert_eq!(x.blocks[1].text(), "The start");
        assert_eq!(x.blocks[2].kind, BlockKind::Image);
        assert!(x.blocks[2].spans.is_empty());
        assert!(!serde_json::to_string(&x.blocks)
            .unwrap()
            .contains("tracker"));
    }

    #[test]
    fn survives_markup_that_is_not_well_formed() {
        let x = extract(
            "<html><body><p>One<p>Two <b>bold</i> still</p><br>Three &bogus; <td>cell",
            100,
            &|_| None,
        );
        let got: Vec<String> = x.blocks.iter().map(Block::text).collect();
        assert_eq!(got, vec!["One", "Two bold still", "Three &bogus; cell"]);
    }

    #[test]
    fn stops_at_the_block_limit_and_says_so() {
        let body = "<p>x</p>".repeat(50);
        let x = extract(&format!("<body>{body}</body>"), 10, &|_| None);
        assert_eq!(x.blocks.len(), 10);
        assert!(x.truncated);
    }

    #[test]
    fn removes_control_characters() {
        assert_eq!(
            lines("<p>a&#27;[31mb&#7;c&#x200B;d</p>"),
            vec!["a[31mbc\u{200b}d"]
        );
    }

    #[test]
    fn keeps_trailing_spaces_in_preformatted_text() {
        let x = run("<pre>  line one  \n  line two  \n</pre>");
        assert_eq!(x.blocks[0].text(), "  line one  \n  line two  ");
    }

    #[test]
    fn keeps_an_empty_first_cell_in_its_column() {
        assert_eq!(
            lines("<table><tr><td>East</td><td></td><td>West</td></tr><tr><td></td><td>High</td><td>Low</td></tr><tr><td></td><td> </td></tr></table>"),
            vec!["East\t\tWest", "\tHigh\tLow"]
        );
    }

    #[test]
    fn notes_remote_references_without_returning_them() {
        assert!(
            !run(
                "<p>Plain <a href='http://example.com/'>link</a> <img src='local.png' alt=''/></p>"
            )
            .remote_references
        );
        for body in [
            "<img src='https://tracker.example/p.gif' alt=''/>",
            "<script src='//cdn.example/x.js'></script>",
            "<video poster='http://example.com/p.jpg'></video>",
            "<svg><image xlink:href='http://example.com/c.jpg'/></svg>",
        ] {
            let x = run(body);
            assert!(x.remote_references, "{body}");
            assert!(!serde_json::to_string(&x.blocks)
                .unwrap()
                .contains("example"));
        }
        let head = extract("<html><head><link rel='stylesheet' href='https://fonts.example/css'/></head><body><p>x</p></body></html>", 10, &|_| None);
        assert!(head.remote_references);
    }

    #[test]
    fn keeps_trailing_empty_cells_and_cells_with_paragraphs_on_one_row() {
        assert_eq!(
            lines("<table><tr><td></td><td>B</td><td></td></tr><tr><td>C</td><td></td><td>E</td></tr></table>"),
            vec!["\tB\t", "C\t\tE"]
        );
        assert_eq!(
            lines("<table><tr><td><p>Left</p><p>more</p></td><td><p>Right</p></td></tr></table><p>After</p>"),
            vec!["Left more\tRight", "After"]
        );
    }

    #[test]
    fn notes_remote_references_in_source_sets_and_styles() {
        for body in [
            "<img src='a.png' srcset='a.png 1x, https://cdn.example/a2.png 2x' alt=''/>",
            "<p style=\"background: url('http://example.com/bg.png')\">x</p>",
            "<p style='background:url(//example.com/bg.png)'>x</p>",
            "<style>@import url(https://fonts.example/css);</style><p>x</p>",
        ] {
            assert!(run(body).remote_references, "{body}");
        }
        for body in [
            "<img src='a.png' srcset='a.png 1x, b.png 2x' alt=''/>",
            "<p style='background:url(bg.png); color: red'>x</p>",
            "<style>p { margin: 0 } /* see http://example.com */</style><p>x</p>",
        ] {
            assert!(!run(body).remote_references, "{body}");
        }
    }

    #[test]
    fn separates_cell_paragraphs_from_text_on_either_side() {
        assert_eq!(
            lines("<table><tr><td>lead<p>Left</p>tail</td><td><div>Right</div></td></tr></table>"),
            vec!["lead Left tail\tRight"]
        );
    }

    #[test]
    fn judges_style_targets_not_stray_addresses() {
        for css in [
            "background: url( //example.test/a )",
            "background:url( 'https://example.test/a.png' )",
            "@import \"//example.test/x.css\";",
            "@import url(http://example.test/x.css);",
        ] {
            assert!(css_is_remote(css), "{css}");
        }
        for css in [
            "background: url(local.png) /* see https://example.test/docs */",
            "content: 'http://example.test'",
            "@import 'local.css'; /* //cdn */",
        ] {
            assert!(!css_is_remote(css), "{css}");
        }
        assert!(
            run(
                "<p><a href='x.xhtml' style='background:url(https://example.test/b.png)'>x</a></p>"
            )
            .remote_references
        );
        assert!(!run("<p><a href='https://example.test/'>x</a></p>").remote_references);
    }

    #[test]
    fn joins_table_cells_on_one_line_per_row() {
        assert_eq!(
            lines(
                "<table><tr><th>Name</th><th>Age</th></tr><tr><td>Ann</td><td>9</td></tr></table>"
            ),
            vec!["Name\tAge", "Ann\t9"]
        );
    }
}
