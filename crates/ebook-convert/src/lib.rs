//! Background conversion of an immutable managed book into a new EPUB.
mod host;
mod spawn;
pub use host::{start, Config, Job, Request, Stage};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const LOSS_DESCRIPTION: &str = "The EPUB keeps supported text and pictures in reading order. Original typography, links, layout, navigation targets and reading positions may be lost. Only JPEG and PNG pictures are supported.";
pub const INPUT_LIMIT: u64 = 64 << 20;
pub const OUTPUT_LIMIT: u64 = 96 << 20;
pub const STATUS_LIMIT: usize = 64 << 10;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub authors: Option<Vec<String>>,
    pub language: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Cover {
    Jpeg(Vec<u8>),
    Png(Vec<u8>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Exists,
    Protected,
    Unsupported(String),
    Invalid(String),
    Limit,
    Sandbox(String),
    Deadline,
    Validation(String),
    Io(String),
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error.to_string())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Report {
    pub identity: String,
    pub direction: String,
    pub version: String,
    pub size: u64,
    pub losses: Vec<String>,
}

#[derive(Debug)]
pub struct Converted {
    pub path: PathBuf,
    pub report: Report,
}
