//! The helper's entry point: limits, isolation check, one operation, one reply.

use super::archive::Archive;
use super::package::{self, Package};
use super::{
    encoding, is_text_document, isolation, markup, paths, resource, survey, text, toc,
    truncate_chars,
};
use crate::model::{
    Block, BlockKind, Chapter, ChapterContent, ChapterInfo, ChapterWarning, Limits, Link, Target,
};
use crate::wire::{
    BookOutline, Payload, ProbeReport, Response, WireError, WireErrorKind, BOOK_IN_SANDBOX,
};
use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
use std::fs::File;
use std::io::Write;

/// Run the helper. `args` excludes the program name:
/// `probe` | `metadata <limits>` | `open <limits>` | `survey <limits>` |
/// `chapter <limits> <index>` | `content <limits> <index>` |
/// `resource <limits> <id>`,
/// followed by the CPU-seconds allowance where a limits argument is present.
pub fn main(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("overflow-probe") {
        return overflow_probe();
    }
    let response = match dispatch(args) {
        Ok(payload) => Response::Ok(payload),
        Err(e) => Response::Err(e),
    };
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    match serde_json::to_writer(&mut out, &response)
        .map_err(|e| e.to_string())
        .and_then(|_| out.flush().map_err(|e| e.to_string()))
    {
        Ok(()) => 0,
        Err(_) => 3,
    }
}

fn usage() -> WireError {
    WireError::new(WireErrorKind::Usage, "bad helper arguments")
}

/// Lower this process's own limits. Hard and soft are set together, so they
/// cannot be raised again. A limit that cannot be set is a reason not to run.
fn restrict(limits: &Limits, cpu_seconds: u64) -> Result<(), WireError> {
    let set = |resource, value: u64| {
        // A limit inherited from the application that is already lower,
        // soft or hard, is kept: this only ever tightens.
        let inherited = getrlimit(resource);
        let value = [inherited.current, inherited.maximum]
            .into_iter()
            .flatten()
            .fold(value, u64::min);
        setrlimit(
            resource,
            Rlimit {
                current: Some(value),
                maximum: Some(value),
            },
        )
        .map_err(|e| {
            WireError::new(
                WireErrorKind::NotIsolated,
                format!("cannot set a resource limit: {e}"),
            )
        })
    };
    set(Resource::As, limits.memory_bytes)?;
    set(Resource::Cpu, cpu_seconds.max(1))?;
    set(Resource::Core, 0)?;
    // The helper is one process with one thread. With this, it cannot become
    // more, so the address-space limit above bounds the whole sandbox.
    set(Resource::Nproc, 1)?;
    set(Resource::Nofile, 32)
}

fn dispatch(args: &[String]) -> Result<Payload, WireError> {
    let op = args.first().map(String::as_str).ok_or_else(usage)?;
    let limits: Limits = match args.get(1) {
        Some(json) => serde_json::from_str(json).map_err(|_| usage())?,
        None => Limits::default(),
    };
    let cpu = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    restrict(&limits, cpu)?;
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
        "survey" => Ok(Payload::Survey(survey::survey(
            &mut archive,
            &package,
            &limits,
        ))),
        "chapter" => {
            let index: u32 = args.get(3).and_then(|s| s.parse().ok()).ok_or_else(usage)?;
            chapter(&mut archive, &package, &limits, index).map(Payload::Chapter)
        }
        "content" => {
            let index: u32 = args.get(3).and_then(|s| s.parse().ok()).ok_or_else(usage)?;
            content(&mut archive, &package, &limits, index).map(Payload::Content)
        }
        "resource" => {
            let id: u32 = args.get(3).and_then(|s| s.parse().ok()).ok_or_else(usage)?;
            readable(&package)?;
            resource::read(&mut archive, &package, &limits, id).map(Payload::Resource)
        }
        _ => Err(usage()),
    }
}

fn readable(package: &Package) -> Result<(), WireError> {
    if package.metadata.protected {
        return Err(WireError::new(
            WireErrorKind::Protected,
            "the book declares encrypted content",
        ));
    }
    if package.spine.is_empty() {
        return Err(WireError::invalid("the book lists no chapters"));
    }
    Ok(())
}

fn open(
    archive: &mut Archive,
    package: Package,
    limits: &Limits,
) -> Result<BookOutline, WireError> {
    readable(&package)?;
    let toc = toc::load(archive, &package, limits)?;
    // One pass over the contents: the first label that points into each chapter.
    let mut titles: Vec<Option<&str>> = vec![None; package.spine.len()];
    for entry in &toc {
        let slot = entry
            .target
            .as_ref()
            .and_then(|t| titles.get_mut(t.chapter as usize));
        if let Some(slot) = slot {
            if slot.is_none() && !entry.title.is_empty() {
                *slot = Some(&entry.title);
            }
        }
    }
    let chapters = package
        .spine
        .iter()
        .zip(&titles)
        .enumerate()
        .map(|(index, (item, title))| ChapterInfo {
            index: index as u32,
            title: title.map(str::to_owned),
            linear: item.linear,
            size: archive
                .stated_size(&item.path)
                .unwrap_or(0)
                .min(limits.max_chapter_bytes)
                .min(u64::from(u32::MAX)) as u32,
        })
        .collect();
    Ok(BookOutline {
        metadata: package.metadata,
        chapters,
        toc,
    })
}

fn toc_title(toc: &[crate::model::TocEntry], chapter: u32) -> Option<String> {
    toc.iter()
        .find(|e| e.target.as_ref().is_some_and(|t| t.chapter == chapter) && !e.title.is_empty())
        .map(|e| e.title.clone())
}

fn chapter(
    archive: &mut Archive,
    package: &Package,
    limits: &Limits,
    index: u32,
) -> Result<Chapter, WireError> {
    readable(package)?;
    let item = package.spine.get(index as usize).ok_or_else(|| {
        WireError::new(
            WireErrorKind::ChapterOutOfRange,
            package.spine.len().to_string(),
        )
    })?;
    let toc = toc::load(archive, package, limits)?;
    let mut out = Chapter {
        index,
        title: toc_title(&toc, index),
        blocks: Vec::new(),
        truncated: false,
        warnings: Vec::new(),
    };
    if item.media_type.starts_with("image/") {
        if !archive.contains(&item.path) {
            return Err(WireError::invalid(
                "a picture in the reading order is missing",
            ));
        }
        out.blocks.push(Block {
            kind: BlockKind::Image,
            spans: Vec::new(),
            anchors: Vec::new(),
        });
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
    let resolve = |href: &str| resolve_link(&chapters, &item.path, href);
    let extracted = text::extract(&source, limits.max_blocks_per_chapter as usize, &resolve);
    out.truncated = extracted.truncated;
    if extracted.remote_references {
        out.warnings.push(ChapterWarning::RemoteReferences);
    }
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

/// A hyperlink's address as the reader may use it: a place in the book, or
/// an `http`, `https` or `mailto` address.
fn resolve_link(
    chapters: &std::collections::HashMap<&str, u32>,
    base: &str,
    href: &str,
) -> Option<Link> {
    let href = href.trim();
    if paths::is_external(href) {
        let lower = href.to_ascii_lowercase();
        let allowed = ["http://", "https://", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p));
        let tidy = !href.chars().any(|c| c.is_control() || c.is_whitespace());
        return (allowed && tidy && href.len() <= 2048).then(|| Link::External(href.to_owned()));
    }
    let local = paths::resolve(base, href)?;
    let chapter = *chapters.get(local.path.as_str())?;
    let anchor = local.fragment.map(|mut f| {
        truncate_chars(&mut f, 256);
        f
    });
    Some(Link::Internal(Target { chapter, anchor }))
}

fn content(
    archive: &mut Archive,
    package: &Package,
    limits: &Limits,
    index: u32,
) -> Result<ChapterContent, WireError> {
    readable(package)?;
    let item = package.spine.get(index as usize).ok_or_else(|| {
        WireError::new(
            WireErrorKind::ChapterOutOfRange,
            package.spine.len().to_string(),
        )
    })?;
    let toc = toc::load(archive, package, limits)?;
    let mut out = ChapterContent {
        index,
        title: toc_title(&toc, index),
        markup: String::new(),
        length: 0,
        styles: Vec::new(),
        language: package.metadata.languages.first().cloned(),
        right_to_left: false,
        truncated: false,
        warnings: Vec::new(),
    };
    let pictures = resource::pictures(package);
    if item.media_type.starts_with("image/") {
        match pictures.get(item.path.as_str()) {
            Some(id) if archive.contains(&item.path) => {
                out.markup = format!(r#"<img src="/r/{id}" alt=""/>"#);
            }
            Some(_) => {
                return Err(WireError::invalid(
                    "a picture in the reading order is missing",
                ))
            }
            None => out.warnings.push(ChapterWarning::UnsupportedContent),
        }
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
    let link = |href: &str| match resolve_link(&chapters, &item.path, href)? {
        Link::Internal(t) => Some(markup::Href::Chapter(t.chapter, t.anchor)),
        Link::External(address) => Some(markup::Href::External(address)),
    };
    let picture = |reference: &str| resource::resolve(&pictures, &item.path, reference);
    let done = markup::sanitise(
        &source,
        limits.max_markup_nodes as usize,
        &markup::Resolver {
            link: &link,
            picture: &picture,
        },
    );
    out.truncated = done.truncated;
    if done.remote_references {
        out.warnings.push(ChapterWarning::RemoteReferences);
    }
    if done.malformed {
        out.warnings.push(ChapterWarning::MalformedMarkup);
    }
    if done.unsupported {
        out.warnings.push(ChapterWarning::UnsupportedContent);
    }
    if out.title.is_none() {
        out.title = done.first_heading.or(done.head_title);
    }
    if done.language.is_some() {
        out.language = done.language;
    }
    out.right_to_left = done.right_to_left;
    out.markup = done.markup;
    out.length = done.length;
    Ok(out)
}

/// Sizes and counts here come from untrusted files, so arithmetic that
/// overflows must stop the helper rather than wrap. This lets a test confirm
/// that the helper it was handed was built that way: with overflow checks the
/// addition panics and the process fails; without them it prints and succeeds.
fn overflow_probe() -> i32 {
    let sum = std::hint::black_box(u8::MAX) + std::hint::black_box(1u8);
    println!("wrapped to {sum}");
    0
}
