# ebook-reader

Opens a reader's own DRM-free EPUB 2 or EPUB 3 file and returns plain data an
interface can draw: import metadata, a word count and cover thumbnail for
browsing, the reading order, the table of contents and each chapter as blocks
of styled text. The behaviour is specified in
[specs/epub-reader-mvp.md](../../specs/epub-reader-mvp.md). Linux only for now.

## Entry point

```rust
use ebook_reader::{Reader, ReaderConfig};

let reader = Reader::new(ReaderConfig::default())?;   // proves the sandbox works
let meta = reader.inspect(path)?;                     // import: title, authors, identifiers,
                                                      // series, published, right_to_left
let survey = reader.survey(path)?;                    // browsing: words and cover
let book = reader.open(path)?;                        // reading
for info in book.chapters() { /* index, title, linear; zero-based, reading order */ }
for entry in book.toc() { /* title, depth, target: Option<Target> */ }
let chapter = book.read_chapter(0)?;                  // blocks of text
```

`inspect` and `open` are separate on purpose: a book can import and still
refuse to open, for example when `Metadata::protected` is set.

`survey` reads every linear chapter and decodes the cover, so it takes longer
than `inspect`: tens of milliseconds for a novel in a release build. Its two
results are independent and neither is an error:

```rust
let Survey { words, cover } = reader.survey(path)?;
// words: Option<u32>, None when some linear chapter could not be counted.
if let Some(SurveyedCover { thumbnail, average, spine }) = cover {
    // thumbnail.width() <= 256, thumbnail.height() <= 384, both >= 1;
    // thumbnail.pixels() is exactly width * height * 3 bytes of RGB, row by row.
    // average and spine are Rgb; Rgb::to_u32 gives 0xRRGGBB.
}
```

An error from `survey` means the helper gave no answer (it timed out,
crashed, or the file is not a readable EPUB); asking again later may work.
A protected book surveys as `{ words: None, cover: None }`.

Following a link or contents entry, and resuming:

```rust
// A Target is a chapter index and an optional anchor.
let chapter = book.read_chapter(target.chapter)?;
let block = target.anchor.as_deref().and_then(|a| chapter.block_for_anchor(a)).unwrap_or(0);

// Save on quit: the chapter shown and its first visible block.
let saved = Position { chapter: chapter.index, block }.to_string();

// Restore after a restart. Bad saved text means start at the beginning.
let pos = book.clamp(saved.parse().unwrap_or_default());
let chapter = book.read_chapter(pos.chapter)?;
match chapter.clamp_block(pos.block) {
    Some(block) => { /* scroll to this block */ }
    None => { /* the chapter has no blocks */ }
}
```

A chapter read can succeed and still carry `warnings` or `truncated`; those
are notes for the reader, not failures.

Showing a chapter in a web view:

```rust
let content = book.read_content(0)?;                  // markup, length, language, warnings
// content.markup: children of an XHTML <body>. Wrap it in your own document.
//   <a href="/c/3#note1">      a place in the book: chapter 3, anchor note1.
//                              The anchor is percent-encoded once; decode it once
//                              and look for that id. `/c/3` alone is the chapter's start.
//   <a data-external="https://…">  an outside address, inert; ask before opening
//   <img src="/r/12" alt="…"/>  a picture: ResourceId(12)
let picture = book.read_resource(ResourceId(12))?;    // kind and undecoded bytes
```

A picture the markup names can still be refused when it is read: missing,
too large, or not the type it claims. Show its `alt` text then.

A place inside a chapter is an offset into its text in UTF-16 code units,
from 0 to `content.length`. `book.chapters()[i].size` weighs chapters for a
progress figure.

Every call blocks while one helper process runs, usually a few milliseconds.
Call from a worker thread. `Reader` and `Book` are `Send + Sync + Clone`.

## What callers can rely on

- **No book byte is parsed in the calling process.** The library starts
  `ebook-reader-helper` under bubblewrap (`bwrap`) for every call and reads back
  a size-capped JSON reply.
- **Fail closed.** `Reader::new` returns an error when the helper or `bwrap` is
  missing, or when the helper's isolation check fails inside the sandbox. There
  is no unsandboxed fallback. The helper also refuses to read a book when run
  outside a sandbox.
- **Inert output, with two exceptions.** Results hold text, style flags,
  anchor names and link targets. No script, style sheet or file path from the
  book reaches the caller. Markup and encoded pictures reach it only through
  `read_content` and `read_resource`, described next. `Link::External` is text; ask the user before
  opening it. The one picture is the survey's cover, as raw RGB pixels that
  the helper decoded and scaled in the sandbox.
- **Content markup is written by the helper, not copied.** `read_content`
  returns well-formed XML built from an allow-list of elements and
  attributes (C1 and C2 in the spec). It holds no script, no handler
  attribute, no style, no frame, form control or embedded object, and no
  address other than `/c/<chapter>#<anchor>`, `/r/<id>` and the inert
  `data-external`. The host does not parse it again, but it counts the text
  and refuses a reply whose `length` is not that count. It is still the book's
  content, so show it only where no script runs and nothing else loads, and
  serve it as `application/xhtml+xml` so no parser repairs it into something
  else. `styles` is always empty for now.
- **Resources are checked, not decoded.** `read_resource` returns bytes only
  for a PNG, JPEG, GIF or WebP manifest item of at most
  `Limits::max_resource_bytes` (32 MiB) whose first bytes are that type's
  signature. The helper and the host each check. Whoever decodes the bytes
  is parsing untrusted input and needs its own containment. A resource id
  never becomes a path outside the helper.
- **Checked thumbnails.** `Thumbnail` has private fields and no public
  constructor. The host builds one only after checking both sides are 1 or
  more and within 256 by 384 and that the pixels are exactly width times
  height times 3 bytes, with overflow-checked arithmetic. A reply that fails
  is `ReaderError::HelperFailed`. The host never decodes an image.
- **Cover limits.** The helper reads the cover only up to
  `Limits::max_cover_bytes` (20 MiB unpacked) and decodes it only when its
  header declares at most `Limits::max_cover_pixels` (40 million). JPEG, PNG,
  GIF and WebP are decoded; anything else gives no cover. A cover that would
  need enlarging more than three times gives no cover.
- **Stateless reads.** `Book` holds no process and no open file. A helper that
  crashes or times out costs one call; the next call starts a new helper.
  The helper doesn't crash in the system's sense. Its first step installs
  handlers, and after that, running out of memory or stack, or any signal that
  would dump core, makes it exit with status 4 and
  `helper: stopped after a fatal error` on stderr. The call returns that as
  `ReaderError::HelperFailed`.
- **Stable input.** The file must not change while a `Book` is in use. A changed
  size or modification time gives `ReaderError::SourceChanged`.
- **Errors are sentences.** `ReaderError`'s `Display` text can be shown to a
  reader as it is.

## Helper discovery

`PATH` is never searched. When `ReaderConfig::helper_path` is set, only that
path is used. Otherwise the first existing file wins:

1. In debug builds only: the `EBOOK_READER_HELPER` environment variable, if
   it is an absolute path. Release builds ignore it.
2. `<directory of the running program>/ebook-reader-helper`
3. `<directory of the running program>/../libexec/ebook-manager/ebook-reader-helper`

Cargo does not build a dependency's binaries. An application that depends on
this crate builds the helper itself and installs it by rule 2 or 3:

```sh
cargo build --release --manifest-path crates/ebook-reader/Cargo.toml --bin ebook-reader-helper
```

`bwrap` is taken from `ReaderConfig::bwrap_path`, else `/usr/bin/bwrap`, else
`/bin/bwrap`. It must be bubblewrap 0.8.0 or later (for `--disable-userns`),
on Linux 5.11 or later. Older systems get `SandboxUnavailable`.

## The sandbox

Before bubblewrap starts, the child process marks every descriptor above
stderr close-on-exec and asks the kernel to kill it when the application
exits, so no open file, socket or process handle of the application reaches
the sandbox. This is the crate's only `unsafe` block, in `src/spawn.rs`. The
kill-on-exit request follows the calling thread, so do not let the thread that
made a call end before the call returns; every call here blocks, which keeps
that true.

The helper runs with new user, PID, network, IPC, UTS and cgroup namespaces,
no way to create further user namespaces, no capabilities, an empty
environment and a read-only root that contains only:

- `/helper`: the helper program
- `/book.epub`: the one file being read
- `/usr/lib`, `/usr/lib64`, `/lib`, `/lib64`: the host's libraries, for the loader

Before reading anything the helper limits its own address space, CPU time,
open files, core dumps and process count, and stops if any of those cannot be
set. It then checks that `/proc`, `/home`, `/etc` and similar paths are
absent, that it cannot write, that it has no network route and that it cannot
start another process. The last check is what lets the address-space limit
stand for the whole sandbox. The
host adds a wall-clock deadline and a cap on the reply size, and kills the
sandbox when either is passed. Defaults are in `Limits` and
`ReaderConfig::default()`.

Not yet in place, in the order worth closing:

1. A seccomp system-call filter. The helper can still make any system call an
   unprivileged process may.
2. A private library directory or a static helper. The host's library
   directories are readable inside the sandbox.
3. A cgroup. The memory bound is on address space per process, not on
   resident memory, and there is no kernel-enforced group to clean up.
4. Landlock, as a second layer under the mount setup.
5. Flatpak, where `bwrap` cannot be nested this way. The reader fails closed
   there.

## Side effects

One short-lived `bwrap` and helper process per call. Nothing is written to
disk, nothing is cached and no network connection is made.

## Checking it

```sh
cargo test                     # unit tests and end-to-end tests through the real sandbox
cargo test --release
cargo run --example epub-dump -- book.epub      # debug build: set EBOOK_READER_HELPER to the built helper
cargo test --release --test reading a_novel_is_surveyed -- --nocapture   # prints the survey time
```

The survey's statements (V1 to V6 in the spec) are checked in
`tests/reading.rs` (series, dates, each image format, spine colour, word
counts) and `tests/failures.rs` (unusable covers, cover limits, unknown
counts, and a stand-in sandbox whose malformed cover replies the host must
refuse).

The content statements (C1 to C8) are checked in `tests/content.rs`, which
parses every returned chapter as strict XML, and in the unit tests of
`src/engine/markup.rs`.

The end-to-end tests need a working `bwrap` with unprivileged user namespaces
and fail without one. On GitHub's `ubuntu-latest` runners the distribution's
`bubblewrap` package is enough.

Fixtures, cover pictures included, are generated in `tests/common/mod.rs`. Add a case there when a real
book shows a markup habit the tests do not cover; never commit a real book.

## Layout

- `src/model.rs`, `src/error.rs`: public types
- `src/host.rs`: discovery, sandbox launch, limits, reply checks
- `src/engine/`: parsing, run only inside the helper; `survey.rs` counts
  words, `cover.rs` decodes and shrinks the cover, `markup.rs` writes
  chapter markup and `resource.rs` names and reads pictures
- `src/bin/helper.rs`: the helper program
- `examples/epub-dump.rs`: prints a book as text
