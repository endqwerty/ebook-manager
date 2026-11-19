//! The helper's entry point: limits, isolation check, one operation, one reply.

use super::archive::Archive;
use super::package::{self, Package};
use super::{encoding, isolation, paths, text, toc, truncate_chars};
use crate::model::{Block, BlockKind, Chapter, ChapterInfo, ChapterWarning, Limits, Link, Target};
use crate::wire::{
    BookOutline, Payload, ProbeReport, Response, WireError, WireErrorKind, BOOK_IN_SANDBOX,
};
use rustix::process::{setrlimit, Resource, Rlimit};
use std::fs::File;
use std::io::Write;

/// Run the helper. `args` excludes the program name:
/// `probe` | `metadata <limits>` | `open <limits>` | `chapter <limits> <index>`,
/// followed by the CPU-seconds allowance where a limits argument is present.
pub fn main(args: &[String]) -> i32 {
    let response = match dispatch(args) {
        Ok(payload) => Response::Ok(payload),
        Err(e) => Response::Err(e),
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match serde_json::to_writer(&mut out, &response).map_err(|e| e.to_string()).and_then(|_| out.flush().map_err(|e| e.to_string())) {
        Ok(()) => 0,
        Err(_) => 3,
    }
}

fn usage() -> WireError {
    WireError::new(WireErrorKind::Usage, "bad helper arguments")
}

fn restrict(limits: &Limits, cpu_seconds: u64) {
    let set = |resource, value: u64| {
        let _ = setrlimit(resource, Rlimit { current: Some(value), maximum: Some(value) });
    };
    set(Resource::As, limits.memory_bytes);
    set(Resource::Cpu, cpu_seconds.max(1));
    set(Resource::Core, 0);
    set(Resource::Nofile, 32);
}

fn dispatch(args: &[String]) -> Result<Payload, WireError> {
    let op = args.first().map(String::as_str).ok_or_else(usage)?;
    let limits: Limits = match args.get(1) {
        Some(json) => serde_json::from_str(json).map_err(|_| usage())?,
        None => Limits::default(),
    };
    let cpu = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    restrict(&limits, cpu);
    isolation::verify().map_err(|why| WireError::new(WireErrorKind::NotIsolated, why))?;
    if op == "probe" {
        return Ok(Payload::Probe(ProbeReport { isolated: true }));
    }
    let file = File::open(BOOK_IN_SANDBOX)
        .map_err(|e| WireError::invalid(format!("cannot open the book: {e}")))?;
    let mut archive = Archive::open(file, &limits)?;
    let package = package::load(&mut archive, &limits)?;
    match op {
        "metadata" => Ok(Payload::Metadata(package.metadata)),
        "open" => open(&mut archive, package, &limits).map(Payload::Book),
        "chapter" => {
            let index: u32 = args.get(3).and_then(|s| s.parse().ok()).ok_or_else(usage)?;
            chapter(&mut archive, &package, &limits, index).map(Payload::Chapter)
        }
        _ => Err(usage()),
    }
}

fn readable(package: &Package) -> Result<(), WireError> {
    if package.metadata.protected {
        return Err(WireError::new(WireErrorKind::Protected, "the book declares encrypted content"));
    }
    if package.spine.is_empty() {
        return Err(WireError::invalid("the book lists no chapters"));
    }
    Ok(())
}

fn open(archive: &mut Archive, package: Package, limits: &Limits) -> Result<BookOutline, WireError> {
    readable(&package)?;
    let toc = toc::load(archive, &package, limits);
    let chapters = (0..package.spine.len() as u32)
        .map(|index| ChapterInfo {
            index,
            title: toc_title(&toc, index),
            linear: package.spine[index as usize].linear,
        })
        .collect();
    Ok(BookOutline { metadata: package.metadata, chapters, toc })
}

fn toc_title(toc: &[crate::model::TocEntry], chapter: u32) -> Option<String> {
    toc.iter()
        .find(|e| e.target.as_ref().is_some_and(|t| t.chapter == chapter) && !e.title.is_empty())
        .map(|e| e.title.clone())
}

fn is_text_document(media_type: &str, path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    matches!(media_type, "application/xhtml+xml" | "text/html")
        || [".xhtml", ".html", ".htm", ".xml"].iter().any(|ext| lower.ends_with(ext))
}

fn chapter(
    archive: &mut Archive,
    package: &Package,
    limits: &Limits,
    index: u32,
) -> Result<Chapter, WireError> {
    readable(package)?;
    let item = package.spine.get(index as usize).ok_or_else(|| {
        WireError::new(WireErrorKind::ChapterOutOfRange, package.spine.len().to_string())
    })?;
    let toc = toc::load(archive, package, limits);
    let mut out = Chapter {
        index,
        title: toc_title(&toc, index),
        blocks: Vec::new(),
        truncated: false,
        warnings: Vec::new(),
    };
    if item.media_type.starts_with("image/") && item.media_type != "image/svg+xml" {
        out.blocks.push(Block { kind: BlockKind::Image, spans: Vec::new(), anchors: Vec::new() });
        return Ok(out);
    }
    if !is_text_document(&item.media_type, &item.path) {
        out.warnings.push(ChapterWarning::UnsupportedContent);
        return Ok(out);
    }
    let bytes = archive.read(&item.path, limits.max_chapter_bytes)?;
    let source = encoding::decode(&bytes)?;
    drop(bytes);
    let chapters = package.chapter_index();
    let resolve = |href: &str| -> Option<Link> {
        let href = href.trim();
        if paths::is_external(href) {
            let lower = href.to_ascii_lowercase();
            let allowed = ["http://", "https://", "mailto:"].iter().any(|p| lower.starts_with(p));
            let tidy = !href.chars().any(|c| c.is_control() || c.is_whitespace());
            return (allowed && tidy && href.len() <= 2048).then(|| Link::External(href.to_owned()));
        }
        let local = paths::resolve(&item.path, href)?;
        let chapter = *chapters.get(local.path.as_str())?;
        let anchor = local.fragment.map(|mut f| {
            truncate_chars(&mut f, 256);
            f
        });
        Some(Link::Internal(Target { chapter, anchor }))
    };
    let extracted = text::extract(&source, limits.max_blocks_per_chapter as usize, &resolve);
    out.truncated = extracted.truncated;
    if extracted.malformed {
        out.warnings.push(ChapterWarning::MalformedMarkup);
    }
    if out.title.is_none() {
        out.title = extracted
            .blocks
            .iter()
            .find(|b| matches!(b.kind, BlockKind::Heading { .. }))
            .map(Block::text)
            .map(|t| super::clean_inline(&t))
            .filter(|t| !t.is_empty())
            .or(extracted.head_title);
    }
    out.blocks = extracted.blocks;
    Ok(out)
}
