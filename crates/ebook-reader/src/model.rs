//! Public data types. Everything here is inert: text, flags and indices.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Bounds applied to one helper run. The helper enforces them on the book; the
/// host enforces the file size, output size and wall-clock time again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    /// Largest EPUB file accepted.
    pub max_file_bytes: u64,
    /// Most entries the ZIP container may list.
    pub max_archive_entries: u32,
    /// Largest unpacked package, navigation or container document.
    pub max_package_bytes: u64,
    /// Largest unpacked chapter document.
    pub max_chapter_bytes: u64,
    /// Most reading-order items.
    pub max_spine_items: u32,
    /// Most table-of-contents entries.
    pub max_toc_entries: u32,
    /// Most blocks returned for one chapter. A longer chapter is cut and flagged.
    pub max_blocks_per_chapter: u32,
    /// Largest reply the host accepts from the helper.
    pub max_output_bytes: u64,
    /// Address-space limit the helper puts on itself.
    pub memory_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        const MIB: u64 = 1024 * 1024;
        Limits {
            max_file_bytes: 512 * MIB,
            max_archive_entries: 20_000,
            max_package_bytes: 8 * MIB,
            max_chapter_bytes: 16 * MIB,
            max_spine_items: 10_000,
            max_toc_entries: 20_000,
            max_blocks_per_chapter: 50_000,
            max_output_bytes: 96 * MIB,
            memory_bytes: 768 * MIB,
        }
    }
}

/// What the book says about itself. Values are the publisher's, unverified.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub authors: Vec<Author>,
    pub identifiers: Vec<Identifier>,
    pub languages: Vec<String>,
    /// The package `version` attribute, for example `"2.0"` or `"3.0"`.
    pub epub_version: Option<String>,
    /// The book declares encrypted content other than obfuscated fonts.
    /// [`crate::Reader::open`] refuses such a book.
    pub protected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Author {
    pub name: String,
    /// The publisher's sorting form, such as `"Austen, Jane"`.
    pub sort_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identifier {
    /// Lower-case scheme when the book states one: `"isbn"`, `"uuid"`, `"doi"`.
    pub scheme: Option<String>,
    /// The value with any `urn:<scheme>:` prefix removed.
    pub value: String,
    /// This is the identifier the package names as its unique identifier.
    pub is_unique_id: bool,
}

/// One reading-order item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub index: u32,
    /// The first table-of-contents label that points into this chapter.
    pub title: Option<String>,
    /// `false` for items the book marks as outside the main reading line.
    pub linear: bool,
}

/// A place in the book: a chapter and, optionally, a named point inside it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub chapter: u32,
    pub anchor: Option<String>,
}

/// One table-of-contents line. Entries come flattened in reading order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TocEntry {
    pub title: String,
    /// Nesting level, 0 for top-level entries.
    pub depth: u32,
    /// `None` when the entry is a plain heading or points outside the reading order.
    pub target: Option<Target>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chapter {
    pub index: u32,
    /// Contents label, else the first heading, else the document title.
    pub title: Option<String>,
    pub blocks: Vec<Block>,
    /// The chapter had more blocks than the limit; the rest was dropped.
    pub truncated: bool,
    pub warnings: Vec<ChapterWarning>,
}

impl Chapter {
    /// The block a contents entry or internal link with this anchor points at.
    pub fn block_for_anchor(&self, anchor: &str) -> Option<u32> {
        self.blocks
            .iter()
            .position(|b| b.anchors.iter().any(|a| a == anchor))
            .map(|i| i as u32)
    }

    /// The chapter's text with one line per block. Meant for search and tests.
    pub fn plain_text(&self) -> String {
        let mut out = String::new();
        for block in &self.blocks {
            out.push_str(&block.text());
            out.push('\n');
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChapterWarning {
    /// The markup was broken. Text up to and around the damage is returned.
    MalformedMarkup,
    /// The item is not a text document this build can show.
    UnsupportedContent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub kind: BlockKind,
    pub spans: Vec<Span>,
    /// Names of points inside this block that links and contents entries use.
    pub anchors: Vec<String>,
}

impl Block {
    pub fn text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockKind {
    Paragraph,
    Heading {
        /// 1 to 6.
        level: u8,
    },
    ListItem {
        /// 1 for a top-level list.
        depth: u8,
        /// The item number in a numbered list; `None` for a bulleted list.
        ordinal: Option<u32>,
    },
    Quote,
    /// Line breaks and spacing inside the spans are significant.
    Preformatted,
    /// A thematic break. No spans.
    Separator,
    /// A picture stood here. Spans hold its text alternative, if it had one.
    Image,
}

/// A run of text with one style. `text` may contain `\n` for forced line breaks.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub superscript: bool,
    pub subscript: bool,
    pub link: Option<Link>,
}

/// Where a link leads. Nothing is opened or fetched by this crate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Link {
    Internal(Target),
    /// An `http`, `https` or `mailto` address exactly as the book wrote it.
    /// Ask the user before handing it to the system.
    External(String),
}

/// A reading position the host stores and passes back.
///
/// The text form is `epubpos1:<chapter>:<block>`. Both numbers are zero-based.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Position {
    pub chapter: u32,
    pub block: u32,
}

impl fmt::Display for Position {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "epubpos1:{}:{}", self.chapter, self.block)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsePositionError;

impl fmt::Display for ParsePositionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("not a saved reading position")
    }
}

impl std::error::Error for ParsePositionError {}

impl FromStr for Position {
    type Err = ParsePositionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut parts = s.split(':');
        let number = |p: Option<&str>| -> Result<u32, ParsePositionError> {
            let p = p.ok_or(ParsePositionError)?;
            if p.is_empty() || p.len() > 10 || !p.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ParsePositionError);
            }
            p.parse().map_err(|_| ParsePositionError)
        };
        if parts.next() != Some("epubpos1") {
            return Err(ParsePositionError);
        }
        let chapter = number(parts.next())?;
        let block = number(parts.next())?;
        if parts.next().is_some() {
            return Err(ParsePositionError);
        }
        Ok(Position { chapter, block })
    }
}
