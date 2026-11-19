use std::fmt;
use std::path::PathBuf;

/// Why a book this build cannot read was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unsupported {
    /// The file is not an EPUB container.
    NotEpub,
    /// The book is encrypted or carries DRM.
    Protected,
    /// The container uses a compression method other than stored or deflate.
    Compression,
    /// A document uses a text encoding this build does not decode.
    Encoding,
}

/// Every way a call can fail. `Display` is a sentence fit to show a reader.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReaderError {
    /// The helper program was not at any of the documented places.
    HelperNotFound {
        searched: Vec<PathBuf>,
    },
    /// The sandbox could not be started or did not isolate. Nothing was parsed.
    SandboxUnavailable {
        reason: String,
    },
    /// The file is missing, not a regular file or cannot be read.
    SourceUnreadable {
        detail: String,
    },
    /// The file changed after the book was opened.
    SourceChanged,
    Unsupported(Unsupported),
    /// The file is an EPUB but is damaged or incomplete.
    InvalidBook {
        detail: String,
    },
    /// The book is larger or more complex than the configured limits allow.
    LimitExceeded {
        what: String,
    },
    /// The helper did not finish in time and was stopped.
    Timeout,
    /// The helper stopped without a usable answer.
    HelperFailed {
        detail: String,
    },
    ChapterOutOfRange {
        index: u32,
        count: u32,
    },
}

impl fmt::Display for ReaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ReaderError::HelperNotFound { .. } => {
                f.write_str("The book reader component is missing from this installation.")
            }
            ReaderError::SandboxUnavailable { reason } => write!(
                f,
                "Books cannot be opened safely on this system, so nothing was opened ({reason})."
            ),
            ReaderError::SourceUnreadable { detail } => {
                write!(f, "The book file could not be read ({detail}).")
            }
            ReaderError::SourceChanged => {
                f.write_str("The book file changed while it was open. Open it again.")
            }
            ReaderError::Unsupported(Unsupported::NotEpub) => {
                f.write_str("This file is not an EPUB book.")
            }
            ReaderError::Unsupported(Unsupported::Protected) => f.write_str(
                "This book is copy-protected or encrypted. Only DRM-free books can be opened.",
            ),
            ReaderError::Unsupported(Unsupported::Compression) => {
                f.write_str("This book is packed in a way this version cannot unpack.")
            }
            ReaderError::Unsupported(Unsupported::Encoding) => {
                f.write_str("This book uses a text encoding this version cannot read.")
            }
            ReaderError::InvalidBook { detail } => {
                write!(f, "This book is damaged or incomplete ({detail}).")
            }
            ReaderError::LimitExceeded { what } => {
                write!(f, "This book is too large or complex to open ({what}).")
            }
            ReaderError::Timeout => f.write_str("Opening this book took too long and was stopped."),
            ReaderError::HelperFailed { detail } => {
                write!(f, "The book reader stopped unexpectedly ({detail}).")
            }
            ReaderError::ChapterOutOfRange { index, count } => {
                write!(
                    f,
                    "Chapter {} does not exist; this book has {count}.",
                    u64::from(*index) + 1
                )
            }
        }
    }
}

impl std::error::Error for ReaderError {}
