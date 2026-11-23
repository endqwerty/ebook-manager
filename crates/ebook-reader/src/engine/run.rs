//! The helper's entry point: limits, isolation check, then one reply for each
//! request until the host closes its input.

use super::archive::Archive;
use super::package::{self, Package};
use super::{
    encoding, is_text_document, isolation, markup, paths, resource, survey, text, toc,
    truncate_chars,
};
use crate::model::{
    Block, BlockKind, Chapter, ChapterContent, ChapterInfo, ChapterWarning, Limits, Link, Target,
    TocEntry,
};
use crate::wire::{
    write_frame, BookOutline, Payload, ProbeReport, Request, Response, WireError, WireErrorKind,
    BOOK_IN_SANDBOX, MAX_REQUEST_BYTES,
};
use rustix::process::{getrlimit, setrlimit, Resource, Rlimit};
use std::fs::File;
use std::io::{BufRead, Read};

/// Run the helper. `args` excludes the program name: `session <limits>
/// <cpu-seconds>`. The first reply says the sandbox holds; then each line on
/// standard input is one [`Request`] and gets one reply, in order.
pub fn main(args: &[String]) -> i32 {
    if args.first().map(String::as_str) == Some("overflow-probe") {
        return overflow_probe();
    }
    let stdout = std::io::stdout();
    let mut out = stdout.lock();
    let limits = match start(args) {
        Ok(limits) => limits,
        Err(e) => return reply(&mut out, Response::Err(e), &[]),
    };
    let ready = Response::Ok(Payload::Probe(ProbeReport { isolated: true }));
    if reply(&mut out, ready, &[]) != 0 {
        return 3;
    }
    let mut session = Session { limits, book: None };
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    loop {
        let mut line = Vec::new();
        let read = (&mut input)
            .take(MAX_REQUEST_BYTES as u64 + 1)
            .read_until(b'\n', &mut line);
        match read {
            Ok(0) => return 0,
            Ok(_) => {}
            Err(_) => return 3,
        }
        let (response, blob) = match Request::parse(&line).ok_or_else(usage) {
            Ok(request) => match session.serve(request) {
                Ok((payload, blob)) => (Response::Ok(payload), blob),
                Err(e) => (Response::Err(e), Vec::new()),
            },
            Err(e) => (Response::Err(e), Vec::new()),
        };
        if reply(&mut out, response, &blob) != 0 {
            return 3;
        }
    }
}

fn reply(out: &mut impl std::io::Write, response: Response, blob: &[u8]) -> i32 {
    match write_frame(out, &response, blob) {
        Ok(()) => 0,
        Err(_) => 3,
    }
}

fn usage() -> WireError {
    WireError::new(WireErrorKind::Usage, "bad helper arguments")
}

fn start(args: &[String]) -> Result<Limits, WireError> {
    if args.first().map(String::as_str) != Some("session") {
        return Err(usage());
    }
    let limits: Limits = match args.get(1) {
        Some(json) => serde_json::from_str(json).map_err(|_| usage())?,
        None => Limits::default(),
    };
    let cpu = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(30);
    restrict(&limits, cpu)?;
    isolation::verify().map_err(|why| WireError::new(WireErrorKind::NotIsolated, why))?;
    Ok(limits)
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

/// What one helper keeps between requests: the book's archive and package,
/// read on the first request that needs them, and its contents once read.
struct Session {
    limits: Limits,
    book: Option<Result<Loaded, WireError>>,
}

struct Loaded {
    archive: Archive,
    package: Package,
    toc: Option<Vec<TocEntry>>,
}

impl Loaded {
    fn toc(&mut self, limits: &Limits) -> Result<&[TocEntry], WireError> {
        if self.toc.is_none() {
            self.toc = Some(toc::load(&mut self.archive, &self.package, limits)?);
        }
        Ok(self.toc.as_deref().unwrap_or_default())
    }
}

impl Session {
    fn serve(&mut self, request: Request) -> Result<(Payload, Vec<u8>), WireError> {
        let limits = &self.limits;
        let book = self.book.get_or_insert_with(|| {
            let file = File::open(BOOK_IN_SANDBOX)
                .map_err(|e| WireError::invalid(format!("cannot open the book: {e}")))?;
            let mut archive = Archive::open(file, limits)?;
            let package = package::load(&mut archive, limits)?;
            Ok(Loaded {
                archive,
                package,
                toc: None,
            })
        });
        let book = book.as_mut().map_err(|e| e.clone())?;
        // Every request has the whole unpacking allowance, as if it were the
        // helper's only one.
        book.archive.begin_request();
        let none = Vec::new;
        match request {
            Request::Metadata => Ok((Payload::Metadata(book.package.metadata.clone()), none())),
            Request::Open => open(book, limits).map(|b| (Payload::Book(b), none())),
            Request::Survey => {
                let mut found = survey::survey(&mut book.archive, &book.package, limits);
                let pixels = found
                    .cover
                    .as_mut()
                    .map(|c| std::mem::take(&mut c.pixels))
                    .unwrap_or_default();
                Ok((Payload::Survey(found), pixels))
            }
            Request::Chapter(index) => {
                chapter(book, limits, index).map(|c| (Payload::Chapter(c), none()))
            }
            Request::Content(index) => {
                content(book, limits, index).map(|c| (Payload::Content(c), none()))
            }
            Request::Resource(id) => {
                readable(&book.package)?;
                let mut wire = resource::read(&mut book.archive, &book.package, limits, id)?;
                let bytes = std::mem::take(&mut wire.bytes);
                Ok((Payload::Resource(wire), bytes))
            }
        }
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

fn open(book: &mut Loaded, limits: &Limits) -> Result<BookOutline, WireError> {
    readable(&book.package)?;
    let toc = book.toc(limits)?.to_vec();
    let (archive, package) = (&mut book.archive, &book.package);
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
        metadata: package.metadata.clone(),
        chapters,
        toc,
    })
}

fn toc_title(toc: &[crate::model::TocEntry], chapter: u32) -> Option<String> {
    toc.iter()
        .find(|e| e.target.as_ref().is_some_and(|t| t.chapter == chapter) && !e.title.is_empty())
        .map(|e| e.title.clone())
}

fn chapter(book: &mut Loaded, limits: &Limits, index: u32) -> Result<Chapter, WireError> {
    readable(&book.package)?;
    let title = book.toc(limits).map(|toc| toc_title(toc, index))?;
    let (archive, package) = (&mut book.archive, &book.package);
    let item = package.spine.get(index as usize).ok_or_else(|| {
        WireError::new(
            WireErrorKind::ChapterOutOfRange,
            package.spine.len().to_string(),
        )
    })?;
    let mut out = Chapter {
        index,
        title,
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

fn content(book: &mut Loaded, limits: &Limits, index: u32) -> Result<ChapterContent, WireError> {
    readable(&book.package)?;
    let title = book.toc(limits).map(|toc| toc_title(toc, index))?;
    let (archive, package) = (&mut book.archive, &book.package);
    let item = package.spine.get(index as usize).ok_or_else(|| {
        WireError::new(
            WireErrorKind::ChapterOutOfRange,
            package.spine.len().to_string(),
        )
    })?;
    let mut out = ChapterContent {
        index,
        title,
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
        // The item's own type decides, then the file's first manifest item.
        let id = pictures
            .get(item.path.as_str())
            .filter(|_| resource::is_picture(&item.media_type));
        match id {
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
        Link::Internal(t) => {
            // `resolve_link` cuts a long anchor for the block reader. Markup
            // gets the whole anchor, and leaves out one that is too long.
            let whole = paths::resolve(&item.path, href.trim()).and_then(|l| l.fragment);
            Some(markup::Href::Chapter(t.chapter, whole))
        }
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
