//! The helper's reply format. Private to this crate's two halves.

use crate::model::{Chapter, ChapterInfo, Metadata, Rgb, TocEntry};
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
