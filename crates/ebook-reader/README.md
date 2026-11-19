# ebook-reader

Opens a reader's own DRM-free EPUB 2 or EPUB 3 file and returns plain data an
interface can draw: import metadata, the reading order, the table of contents
and each chapter as blocks of styled text. The behaviour is specified in
[specs/epub-reader-mvp.md](../../specs/epub-reader-mvp.md). Linux only for now.

## Entry point

```rust
use ebook_reader::{Reader, ReaderConfig};

let reader = Reader::new(ReaderConfig::default())?;   // proves the sandbox works
let meta = reader.inspect(path)?;                     // import: title, authors, identifiers
let book = reader.open(path)?;                        // reading
for info in book.chapters() { /* index, title, linear; zero-based, reading order */ }
for entry in book.toc() { /* title, depth, target: Option<Target> */ }
let chapter = book.read_chapter(0)?;                  // blocks of text
```

`inspect` and `open` are separate on purpose: a book can import and still
refuse to open, for example when `Metadata::protected` is set.

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
- **Inert output.** Results hold text, style flags, anchor names and link
  targets. No markup, script, style sheet, image data or file path from the book
  reaches the caller. `Link::External` is text; ask the user before opening it.
- **Stateless reads.** `Book` holds no process and no open file. A helper that
  crashes or times out costs one call; the next call starts a new helper.
- **Stable input.** The file must not change while a `Book` is in use. A changed
  size or modification time gives `ReaderError::SourceChanged`.
- **Errors are sentences.** `ReaderError`'s `Display` text can be shown to a
  reader as it is.

## Helper discovery

`PATH` is never searched. When `ReaderConfig::helper_path` is set, only that
path is used. Otherwise the first existing file wins:

1. The `EBOOK_READER_HELPER` environment variable, if it is an absolute path
2. `<directory of the running program>/ebook-reader-helper`
3. `<directory of the running program>/../libexec/ebook-manager/ebook-reader-helper`

Cargo does not build a dependency's binaries. An application that depends on
this crate builds the helper itself and installs it by rule 2 or 3:

```sh
cargo build --release --manifest-path crates/ebook-reader/Cargo.toml --bin ebook-reader-helper
```

`bwrap` is taken from `ReaderConfig::bwrap_path`, else `/usr/bin/bwrap`, else
`/bin/bwrap`.

## The sandbox

The helper runs with new user, PID, network, IPC, UTS and cgroup namespaces, no
capabilities, an empty environment and a read-only root that contains only:

- `/helper`: the helper program
- `/book.epub`: the one file being read
- `/usr/lib`, `/usr/lib64`, `/lib`, `/lib64`: the host's libraries, for the loader

Before reading anything the helper limits its own address space, CPU time, open
files and core dumps, and stops if any of those cannot be set. It then checks that `/proc`, `/home`, `/etc` and similar
paths are absent, that it cannot write, and that it has no network route. The
host adds a wall-clock deadline and a cap on the reply size, and kills the
sandbox when either is passed. Defaults are in `Limits` and
`ReaderConfig::default()`.

Not yet in place: a seccomp system-call filter, Landlock, a cgroup memory
limit, and any sandbox for Flatpak, where `bwrap` cannot be nested this way.

## Side effects

One short-lived `bwrap` and helper process per call. Nothing is written to
disk, nothing is cached and no network connection is made.

## Checking it

```sh
cargo test                     # unit tests and end-to-end tests through the real sandbox
cargo test --release
cargo run --example epub-dump -- book.epub      # needs EBOOK_READER_HELPER or an installed layout
```

The end-to-end tests need a working `bwrap` with unprivileged user namespaces
and fail without one. On GitHub's `ubuntu-latest` runners the distribution's
`bubblewrap` package is enough.

Fixtures are generated in `tests/common/mod.rs`. Add a case there when a real
book shows a markup habit the tests do not cover; never commit a real book.

## Layout

- `src/model.rs`, `src/error.rs`: public types
- `src/host.rs`: discovery, sandbox launch, limits, reply checks
- `src/engine/`: parsing, run only inside the helper
- `src/bin/helper.rs`: the helper program
- `examples/epub-dump.rs`: prints a book as text
