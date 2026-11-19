# Ebook conversion on Linux

Start a job with `ebook_convert::start(request, config)`. The [runnable caller](examples/convert-book.rs) converts one managed source path to a fresh EPUB and prints its JSON report. The [design](../../docs/design/mobi-to-epub.md) explains the helper boundary and catalogue integration. The [behaviour spec](../../specs/mobi-to-epub.md) lists acceptance checks.

The caller owns the managed version and chooses a destination in a trusted directory. Put the library book ID, version ID and SHA-256 into any string representation and pass it as `Request::identity`. Only the caller interprets it. Supply optional title, author list, language and a JPEG or PNG cover in memory. Show `LOSS_DESCRIPTION` before starting. Supply a trusted absolute path to the built converter helper and the reader configuration.

Poll `job.stage()`, call `job.cancel()` to request cancellation and call `job.wait()` for the terminal result. On success, compare the identity in the report to the intended version before attaching it. Another job may already own the requested destination. `Error::Exists` means this job published nothing. Retry an unsuccessful request by calling `start` again. Dropping the handle detaches the running thread. The coordinator owns the catalogue transaction, desktop queue and helper installation.

The writer accepts up to 512 sections, 4,096 images and 16 MiB of chapter markup per section. The checked file retains private staging permissions (0600 on Linux). A malformed source language becomes `und`, and XML-forbidden characters become �. Both changes appear in the report. An invalid catalogue language returns an error.

For package-only job tests, first build the independent reader helper in the same Cargo profile, e.g. `cargo build -p ebook-reader --bin ebook-reader-helper && cargo test -p ebook-convert`.

The host copies the input into private scratch space. The read-only converter sandbox returns bytes over stdout. A second sandbox opens the staged EPUB and reads each chapter. Hidden staging files have no EPUB extension. Normal exits clean up staging and scratch. If the host process dies, its caller may remove orphaned staging files in its own output directory.

Build the helpers and run an independent conformance check on a converted book:

```sh
cargo build --release --locked --workspace --bins
cargo run --release --locked -p ebook-convert --example convert-book -- \
  book.mobi new.epub target/release/ebook-convert-helper target/release/ebook-reader-helper
java -jar "$EPUBCHECK_JAR" new.epub
```

EPUBCheck is a standalone acceptance tool. Runtime jobs use the repository's reader. The package and navigation follow [EPUB 3.3](https://www.w3.org/TR/epub-33/).

The new lockfile dependencies chrono, num-traits and language-tags each offer an MIT or Apache-2.0 licence. The format reader is part of this project.

## MOBI decoder

`mobi::decode(bytes) -> Result<ParsedBook, DecodeError>` in [src/mobi.rs](src/mobi.rs)
reads an unprotected older Kindle book (MOBI 7) into the
[`ParsedBook`](src/book.rs) passed to the EPUB writer. It is a pure function: no
files, no network, no global state. Its tests and the generated sample live in
[tests/mobi](tests/mobi/README.md).

### What it reads

- A Palm database whose type and creator are `BOOK` and `MOBI`, with MOBI type 2
  or 232 and a format version below 8.
- Uncompressed or PalmDOC-compressed text in 4096-byte records, in CP1252 or
  UTF-8. Trailing entries named by the extra data flags are removed.
- Title from the full-name field, falling back to the database name; authors
  from every EXTH 100; language from EXTH 524, else `en` when the locale's
  language byte is 0x09; cover from EXTH 201.
- Markup: `mbp:pagebreak` starts a section; `h1`–`h6` become headings and give
  the section its title, otherwise it is titled "Section N"; block tags end a
  paragraph; `<img recindex=N>` places image record N, counted from 1 at the
  first image record. Entities `&amp; &lt; &gt; &quot; &apos; &nbsp;` and
  numeric references are decoded.
- JPEG and PNG images, each record stored once.

### What it refuses or reports

| Input | Result |
|---|---|
| PalmDOC encryption field not 0, or a DRM offset with a non-zero DRM count | `Protected`, before any text is decompressed |
| HUFF/CDIC compression, other compression types, other encodings, other MOBI types | `Unsupported` with the reason |
| KF8 only (type 248 or version 8+), or a combined MOBI 7 and KF8 file (EXTH 121 or a `BOUNDARY` record) | `Unsupported` |
| Plain PalmDOC (`TEXt`/`REAd`) | `Unsupported` |
| Truncation, offsets out of range or out of order, bad EXTH, bad compressed data, text longer than declared, no readable text | `Invalid` with the reason |
| Input over 64 MiB, or declared text over 64 MiB | `Limit` |

Each record expands to at most 4096 bytes, and the markup is read in one pass
with no recursion, so work and memory grow linearly with the input.

`losses` names each kind of loss once: inline formatting, links (internal
`filepos` and external), tables, lists, GIF images, other image formats, missing
or name-only image references, invalid characters, text shorter than declared
and markup cut off at the end.

### Known gaps

- HUFF/CDIC compression: no allowed source describes it, so these books are
  refused.
- Headings: the four real books tried mark chapter titles with font sizes and
  bold text, not `h1`–`h6`, so their sections are titled "Section N". No allowed
  source describes MOBI 7 heading conventions.
- `filepos` links are dropped: no allowed source describes how they resolve.
- No protected or combined MOBI 7 and KF8 file was available; those refusals are
  checked only against generated files.
- Locale codes other than English are not mapped to languages.

### MOBI decoder sources

Format facts came only from these pages and from inspecting sample files.

| Source | Read | Used for |
|---|---|---|
| <https://wiki.mobileread.com/wiki/MOBI> | 2026-11-19 | Record 0 layout, MOBI header offsets, encryption and DRM fields, EXTH layout and types 100, 121, 201, 524, trailing entries and backward integers, mixed KF8 files |
| <https://wiki.mobileread.com/wiki/PDB> | 2026-11-19 | Palm database header and record list |
| <https://wiki.mobileread.com/wiki/PalmDOC> | 2026-11-19 | PalmDOC compression byte ranges, distance and length |
| <https://wiki.mobileread.com/wiki/EBook_HTML> | 2026-11-19 | Tag list (no `recindex`, `filepos` or `mbp:` facts found) |
| <https://wiki.mobileread.com/wiki/MobiPocket_Creator> | 2026-11-19 | Pointer to the tag list only |
| <https://en.wikipedia.org/wiki/Windows-1252> | 2026-11-19 | CP1252 bytes 0x80–0x9F, the five undefined bytes, Latin-1 for 0xA0–0xFF |

The MOBI wiki page links to source code for HUFF/CDIC in an excluded project.
That link was not opened.

Facts the pages leave open were settled by inspecting sample files:

- Multibyte trailing entry: the record loses its last byte's low two bits plus
  one bytes, which includes the count byte. With this reading, all four books
  below decode to exactly their declared text length.
- `recindex` counts from 1 at the first image record: book 28885 has 38 image
  records and references `00001` to `00038`.
- EXTH 524 holds the language as text, for example `en`.

Sample files (public-domain Project Gutenberg books, downloaded 2026-11-19, not
committed). Their metadata shows a third-party converter produced them; only
their bytes were inspected.

| Book | Source | Trial result |
|---|---|---|
| 11, Alice's Adventures in Wonderland | `aleph.pglaf.org/cache/epub/11/pg11-images.mobi` | Title, author, `en`, cover, 19 sections, opening sentence found |
| 28885, Alice, illustrated by Rackham | `aleph.pglaf.org/cache/epub/28885/pg28885-images.mobi` | 37 JPEGs, 41 image blocks, cover, opening sentence after a drop-cap image |
| 84, Frankenstein | `gutenberg.pglaf.org/cache/epub/84/pg84-images.mobi` | Title, author, cover, 34 sections, first letter's opening found |
| 1661, The Adventures of Sherlock Holmes | `aleph.pglaf.org/cache/epub/1661/pg1661-images.mobi` | Title, author, cover, 19 sections, opening found |
