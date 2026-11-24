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
    /// Most unpacked bytes one request may take out of the archive.
    pub max_unpacked_bytes: u64,
    /// Largest unpacked-to-packed ratio for an entry over 4 MiB unpacked.
    pub max_compression_ratio: u32,
    /// Largest reply the host accepts from the helper.
    pub max_output_bytes: u64,
    /// Address-space limit the helper puts on itself.
    pub memory_bytes: u64,
    /// Largest cover image file, as unpacked, that a survey decodes.
    pub max_cover_bytes: u64,
    /// Most pixels, width times height, a cover image may declare.
    pub max_cover_pixels: u64,
    /// Largest picture, as unpacked, that a resource request returns.
    pub max_resource_bytes: u64,
    /// Most elements and text runs written for one chapter's markup. A longer
    /// chapter is cut and flagged.
    pub max_markup_nodes: u32,
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
            max_unpacked_bytes: 64 * MIB,
            max_compression_ratio: 200,
            max_output_bytes: 96 * MIB,
            memory_bytes: 768 * MIB,
            max_cover_bytes: 20 * MIB,
            max_cover_pixels: 40_000_000,
            max_resource_bytes: 32 * MIB,
            max_markup_nodes: 400_000,
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
    /// The scheme that locks the book, when it declares encrypted content
    /// other than obfuscated fonts. [`crate::Reader::open`] refuses such a
    /// book.
    pub protection: Option<Protection>,
    /// The series the book declares itself part of, in package order. The
    /// first is the primary series.
    pub series: Vec<SeriesMembership>,
    /// The publication date exactly as the file writes it, for example
    /// `"2019"` or `"2019-04-02"`. Not checked to be a date.
    pub published: Option<String>,
    /// The book's pages turn from right to left.
    pub right_to_left: bool,
}

/// A protection scheme, named from what the container declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Protection {
    /// `META-INF/license.lcpl`.
    ReadiumLcp,
    /// `META-INF/rights.xml` in Adobe's namespace.
    Adobe,
    /// `META-INF/sinf.xml`.
    Apple,
    /// A ZIP entry encrypted with the archive's own password scheme.
    Password,
    /// Another `rights.xml`, or another encrypted resource.
    Unrecognised,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeriesMembership {
    pub name: String,
    /// The book's place in the series as text, such as `"2"`, `"1.5"` or
    /// `"TPB1"`. `None` when the book gives none.
    pub position: Option<String>,
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

/// A colour with 8-bit red, green and blue channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    /// `0xRRGGBB`.
    pub fn to_u32(self) -> u32 {
        u32::from(self.r) << 16 | u32::from(self.g) << 8 | u32::from(self.b)
    }
}

/// What [`crate::Reader::survey`] found. The two facts are independent: an
/// unusable cover leaves the word count, and an unknown count leaves the cover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Survey {
    /// Words in the linear reading order. `None` when any linear item could
    /// not be found, was not text this build reads, or was cut at a limit.
    pub words: Option<u32>,
    /// `None` when the book declares no cover, or its cover could not be used.
    pub cover: Option<SurveyedCover>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SurveyedCover {
    pub thumbnail: Thumbnail,
    /// The mean colour of the thumbnail.
    pub average: Rgb,
    /// The most common colour on the tenth of the cover at the bound side.
    pub spine: Rgb,
}

/// A cover scaled to fit within [`Thumbnail::MAX_WIDTH`] by
/// [`Thumbnail::MAX_HEIGHT`], as rows of RGB pixels, top row first, three
/// bytes a pixel, no padding. Only this crate can make one, after checking
/// that both sides are within range and the pixels fill them exactly.
#[derive(Clone, PartialEq, Eq)]
pub struct Thumbnail {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Thumbnail {
    pub const MAX_WIDTH: u32 = 256;
    pub const MAX_HEIGHT: u32 = 384;

    /// `None` unless `1 <= width <= MAX_WIDTH`, `1 <= height <= MAX_HEIGHT`
    /// and `pixels` holds exactly `width * height * 3` bytes.
    pub(crate) fn new(width: u32, height: u32, pixels: Vec<u8>) -> Option<Thumbnail> {
        let sides_fit =
            (1..=Self::MAX_WIDTH).contains(&width) && (1..=Self::MAX_HEIGHT).contains(&height);
        let expected = usize::try_from(width)
            .ok()?
            .checked_mul(usize::try_from(height).ok()?)?
            .checked_mul(3)?;
        (sides_fit && pixels.len() == expected).then_some(Thumbnail {
            width,
            height,
            pixels,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// `width * height * 3` bytes: red, green, blue, row by row.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The pixel at column `x`, row `y`, or `None` outside the image.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Rgb> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let at = (y as usize * self.width as usize + x as usize) * 3;
        Some(Rgb {
            r: self.pixels[at],
            g: self.pixels[at + 1],
            b: self.pixels[at + 2],
        })
    }
}

impl fmt::Debug for Thumbnail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Thumbnail({}x{})", self.width, self.height)
    }
}

/// One reading-order item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterInfo {
    pub index: u32,
    /// The first table-of-contents label that points into this chapter.
    pub title: Option<String>,
    /// `false` for items the book marks as outside the main reading line.
    pub linear: bool,
    /// The chapter document's unpacked size in bytes as the archive states
    /// it, capped at the chapter limit, and 0 when the archive lacks it. The
    /// book controls this number. Use it only to weigh chapters for progress.
    #[serde(default)]
    pub size: u32,
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

    /// Bring a saved block index into this chapter. `None` when the chapter
    /// has no blocks, so there is nothing to scroll to.
    pub fn clamp_block(&self, block: u32) -> Option<u32> {
        let last = u32::try_from(self.blocks.len().checked_sub(1)?).unwrap_or(u32::MAX);
        Some(block.min(last))
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

/// One chapter as markup for a web view. See [`crate::Book::read_content`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChapterContent {
    pub index: u32,
    /// Contents label, else the first heading, else the document title.
    pub title: Option<String>,
    /// The children of an XHTML `body`: well-formed XML written by the helper
    /// from an allow-list of elements and attributes. A place in the book is
    /// `href="/c/<chapter>#<anchor>"`, a picture is `src="/r/<id>"` with a
    /// [`ResourceId`], and an address outside the book is the inert attribute
    /// `data-external`. It is still the book's content: show it only in a
    /// view that runs no script and loads nothing else.
    pub markup: String,
    /// Length of the text in `markup` in UTF-16 code units, the unit a place
    /// in a chapter is counted in.
    pub length: u32,
    /// The book's style sheets for this chapter, in cascade order.
    pub styles: Vec<ResourceId>,
    pub language: Option<String>,
    pub right_to_left: bool,
    /// The chapter was longer than the limit; the rest was dropped.
    pub truncated: bool,
    pub warnings: Vec<ChapterWarning>,
}

/// Names one file of the book without its path: the position of its item in
/// the package manifest. It means the same file for as long as the book file
/// is unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    Png,
    Jpeg,
    Gif,
    Webp,
}

impl ResourceKind {
    pub fn media_type(self) -> &'static str {
        match self {
            ResourceKind::Png => "image/png",
            ResourceKind::Jpeg => "image/jpeg",
            ResourceKind::Gif => "image/gif",
            ResourceKind::Webp => "image/webp",
        }
    }

    /// The kind these bytes begin as, by signature.
    pub fn of(bytes: &[u8]) -> Option<ResourceKind> {
        match bytes {
            [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, ..] => Some(ResourceKind::Png),
            [0xff, 0xd8, 0xff, ..] => Some(ResourceKind::Jpeg),
            [b'G', b'I', b'F', b'8', b'7' | b'9', b'a', ..] => Some(ResourceKind::Gif),
            [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => {
                Some(ResourceKind::Webp)
            }
            _ => None,
        }
    }
}

/// One picture from the book: untrusted bytes whose first bytes match `kind`.
/// Nothing here has decoded them.
#[derive(Clone, PartialEq, Eq)]
pub struct Resource {
    pub kind: ResourceKind,
    pub bytes: Vec<u8>,
}

impl fmt::Debug for Resource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Resource({:?}, {} bytes)", self.kind, self.bytes.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChapterWarning {
    /// The markup was broken. Text up to and around the damage is returned.
    MalformedMarkup,
    /// The item is not a text document this build can show.
    UnsupportedContent,
    /// The chapter refers to pictures, styles or media at a remote address.
    /// Nothing was fetched. Opening the book elsewhere could tell that
    /// address the book was opened.
    RemoteReferences,
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
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
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
