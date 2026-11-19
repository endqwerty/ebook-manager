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
  UTF-8, starting at the first content record named at record 0 offset 192
  (record 1 when the MOBI header is too short to hold it). Every record in the
  declared range is text, whatever its bytes look like. Trailing entries named
  by the extra data flags are removed.
- Records after the declared text are not read. When the first non-book record
  (offset 80) lies further on, the records between the two are left out,
  because no allowed source says whether they belong to the book. The report
  says so when one of them holds a non-zero byte. Records of only zero bytes,
  like the 2-zero-byte record the MOBI page describes, have no text to lose.
  The four real books each have one such record holding a single zero byte.
- An EXTH 121 ("KF8 BOUNDARY Offset") record of any size or value: the older
  text is converted and the report says the file may also hold a newer-format
  copy that was not read. Nothing else is taken as a sign of a second copy.
- Title from the full-name field, falling back to the database name; authors
  from every EXTH 100; language from EXTH 524, else `en` when the locale's
  language byte is 0x09; cover from EXTH 201.
- Markup: `script` and `style` contents are skipped up to their end tag in any
  letter case, without reading tags inside them; text inside `head` is not book
  text; `mbp:pagebreak` starts a section; `h1`–`h6` become headings and give
  the section its title, otherwise it is titled "Section N"; block tags end a
  paragraph; `<img recindex=N>` places image record N, counted from 1 at the
  first image record. Entities `&amp; &lt; &gt; &quot; &apos; &nbsp;` and
  numeric references are decoded.
- JPEG and PNG images, each record stored once.

Named character references outside the supported set stay visible as written and are reported once per book. A numeric reference outside Unicode becomes � and is reported. A quoted attribute may contain `>`. The decoder looks ahead at most 1,024 bytes for its closing quote. A quote with no partner in that window, or whose value would contain the start of a tag (`<` then `/`, `!` or a letter), is treated as stray: the first `>` ends the tag, the text after it is kept and the report says so.

### What it refuses or reports

| Input | Result |
|---|---|
| PalmDOC encryption field not 0, or a DRM offset with a non-zero DRM count | `Protected`, before any text is decompressed |
| HUFF/CDIC compression, other compression types, other encodings, other MOBI types | `Unsupported` with the reason |
| Record 0 describes a KF8 book (MOBI type 248 or file version 8 or more) | `Unsupported` |
| Extra data flags above `0x8000`, or a trailing entry size longer than four bytes (see [product limits](#product-limits)) | `Unsupported` with the reason |
| Plain PalmDOC (`TEXt`/`REAd`) | `Unsupported` |
| Truncation, offsets out of range or out of order, a first content record of 0 or text records past the end of the database, bad EXTH, bad compressed data, text longer than declared, no readable text | `Invalid` with the reason |
| Input over 64 MiB, or declared text over 64 MiB | `Limit` |

Each record expands to at most 4096 bytes, and the markup is read in one pass
with no recursion, so work and memory grow linearly with the input.

`losses` names each kind of loss once: inline formatting, links (internal
`filepos` and external), tables, lists, GIF images, other image formats, missing
or name-only image references, invalid characters in the text, text shorter than
declared and markup cut off at the end. It also reports:

- A possible newer-format copy (EXTH 121) and unread records between the text
  and the first non-book record, as above.
- Metadata: replaced bytes in the title, an author or the language, and
  descriptive EXTH records the EPUB does not carry, named in one entry (for
  example "Metadata not carried over: publisher, description."). Technical EXTH
  records such as creator software are not listed.
- Structure: a `guide` or `reference` tag, any `h1`–`h6` other than `h2` (the
  writer emits every heading as `h2`), and sections titled "Section N" because
  they had no heading.
- Layout: `pre` text, `center`, `blockquote`, `hr`, and `style`, `align`, `width`
  or `height` on any tag but `img`.
- Scanning: removed scripts and style sheets, an unclosed `script`, `style` or
  `head` that hides the rest of the text, and tags ended early at a stray quote.

### Source gaps

Facts the allowed pages leave open, and what the decoder does about each:

- HUFF/CDIC compression is not described, so these books are refused.
- MOBI 7 heading conventions are not described. The four real books tried mark
  chapter titles with font sizes and bold text, not `h1`–`h6`, so their sections
  are titled "Section N" and the report says so.
- How `filepos` links resolve is not described, so they are dropped.
- Combined files. The MOBI and KF8 pages say KindleGen files hold both a MOBI 7
  and a KF8 edition, name EXTH 121 "KF8 BOUNDARY Offset" with a usual length of
  4, and say KindleGen 2.3 and later puts compilation records "just before the
  BOUNDARY record". They do not say what the 121 value counts, which value means
  none, what a BOUNDARY record holds or how to find it, or which "flags near the
  start of the database" identify the file type. So the decoder reads no
  meaning into the 121 value or into any record's bytes: EXTH 121 adds a report
  line and the older edition is converted.
- Records between the declared text and the first non-book record: the page
  marks the first non-book field with a question mark and describes only the
  2-zero-byte record there. Records there are left out, and reported unless
  they hold only zero bytes.
- Trailing entries above bit 16 and backward-encoded integers longer than four
  bytes are not described. See product limits.
- Locale codes other than English: no mapping was taken from a source.

### Product limits

Choices of this decoder, not format facts:

- Extra data flags may use only bits 1–16 (`0x0001`–`0xFFFF`), the range the
  MOBI page documents. A file that sets a higher flag is refused as
  `Unsupported`, not decoded with a guessed entry layout.
- A trailing entry size may be at most four bytes (28 bits, more than any
  record). A longer one is refused as `Unsupported`; one that runs out of
  record bytes is `Invalid`.
- Input and declared text are capped at 64 MiB, and each text record expands
  to at most 4096 bytes.

### Samples not found

No combined MOBI 7 and KF8 file and no protected file was available, so those
paths are checked only against generated files. Project Gutenberg's cache for
book 84 offers `pg84-images-kf8.mobi` (downloaded 2026-11-19, not committed).
Its bytes show a KF8-only file: file version 8, first content record 0, no
EXTH 121 and no record named `BOUNDARY`. The decoder refuses it as a KF8 book.
The same directory has no combined file.

### MOBI decoder sources

Format facts came only from these pages and from inspecting sample files.

| Source | Read | Used for |
|---|---|---|
| <https://wiki.mobileread.com/wiki/MOBI> | 2026-11-19 | Record 0 layout, MOBI header offsets, encryption and DRM fields, EXTH layout and types 100, 121, 201, 524, trailing entries and backward integers, mixed KF8 files. Read again the same day (page source) for the first content record (offset 192), first non-book record (offset 80), descriptive EXTH types 101–113, 503 and 504, flag bit numbering, and the BOUNDARY and compilation records |
| <https://wiki.mobileread.com/wiki/KF8> | 2026-11-19 | KF8 files hold a MOBI database first and the KF8 one later; no layout or markers given |
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
| 84, Frankenstein (KF8 edition) | `aleph.pglaf.org/cache/epub/84/pg84-images-kf8.mobi` | KF8 only, refused as `Unsupported("KF8 (AZW3) book")` |
