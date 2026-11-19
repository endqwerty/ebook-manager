//! Sandboxed EPUB parsing and reading engine.
//!
//! The host API ([`Reader`], [`Book`]) never parses book bytes itself. Every call
//! starts the `ebook-reader-helper` binary inside a bubblewrap sandbox and reads
//! back plain, presentation-ready data. See the crate README for the contract.
#![deny(unsafe_code)]

mod error;
mod host;
mod model;
mod spawn;
#[doc(hidden)]
pub mod wire;

#[doc(hidden)]
pub mod engine;

pub use error::{ReaderError, Unsupported};
pub use host::{Book, Reader, ReaderConfig, HELPER_ENV, HELPER_NAME};
pub use model::{
    Author, Block, BlockKind, Chapter, ChapterInfo, ChapterWarning, Identifier, Limits, Link,
    Metadata, ParsePositionError, Position, SeriesMembership, Span, Target, TocEntry,
};
