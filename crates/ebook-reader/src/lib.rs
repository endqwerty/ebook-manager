//! Sandboxed EPUB parsing and reading engine.
//!
//! The host API ([`Reader`], [`Book`]) never parses book bytes itself. It runs the
//! `ebook-reader-helper` binary inside a bubblewrap sandbox, one for each open
//! book and one for each other call, and reads back plain, presentation-ready
//! data. See the crate README for the contract.
#![deny(unsafe_code)]

mod error;
mod host;
mod model;
mod session;
mod spawn;
#[doc(hidden)]
pub mod wire;

#[doc(hidden)]
pub mod engine;

pub use error::{ReaderError, Unsupported};
pub use host::{Book, Reader, ReaderConfig, HELPER_ENV, HELPER_NAME};
pub use model::{
    Author, Block, BlockKind, Chapter, ChapterContent, ChapterInfo, ChapterWarning, Identifier,
    Limits, Link, Metadata, ParsePositionError, Position, Resource, ResourceId, ResourceKind, Rgb,
    SeriesMembership, Span, Survey, SurveyedCover, Target, Thumbnail, TocEntry,
};
