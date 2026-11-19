# EPUB reader (MVP)

The `ebook-reader` crate opens a DRM-free EPUB 2 or EPUB 3 file on Linux and
gives the desktop interface what it needs to import and read it: title, authors
and identifiers; the chapters in reading order; the table of contents; and each
chapter as blocks of styled text. Book files are untrusted, so all parsing
happens in a helper process inside a bubblewrap sandbox with no network, and the
interface receives text and flags only. When the sandbox cannot be set up,
nothing is opened. The caller-facing contract is in the
[crate README](../crates/ebook-reader/README.md).

This covers the engine side of `READ-1` (contents and a position to resume
from) and `SEC-1` to `SEC-5` for EPUB text. Not covered yet: images, style sheets and fonts,
pagination, search, positions finer than a block, footnote pop-ups, fixed-layout
and media-overlay books, and every platform other than native Linux packages.

## Sandbox

- **S1** A reader is created only after the helper program and `bwrap` are
  found and the helper has confirmed, from inside the sandbox, that it is
  isolated. A missing helper, a missing `bwrap`, a sandbox that fails to start
  and a sandbox that does not isolate each give an error and no reader.
- **S2** The helper checks its confinement at the start of every run and
  refuses to open a book when it can see `/proc`, `/home`, `/etc`, `/tmp`,
  `/run`, `/var`, `/root`, `/sys` or `/dev`, can write to its filesystem, or
  has a network route.
- **S3** Inside the sandbox the helper can read the one book file, itself and
  the host's library directories. It has no network, no other host files, no
  writable storage, no capabilities and an empty environment.
- **S4** The helper is found, in order, at the configured path, at the absolute
  path in `EBOOK_READER_HELPER`, beside the running program, or in
  `../libexec/ebook-manager/` relative to it. `PATH` is not searched. `bwrap`
  is used from the configured path, `/usr/bin/bwrap` or `/bin/bwrap`.
- **S5** Each call runs one helper process that ends with the call. A helper
  that fails affects only that call.

## Import metadata

- **M1** Inspecting a file returns its title, authors, identifiers, languages
  and EPUB version from the package document, with whitespace collapsed.
  A book with several titles gives the one marked as the main title, else the
  first.
- **M2** Authors are the creators with the author role or no role, in package
  order, each with the publisher's sort name when given. When no creator
  qualifies, all creators are returned. Each identifier has its value, a
  lower-case scheme taken from the scheme attribute or a `urn:` or `isbn:`
  prefix, and a flag for the package's unique identifier.
- **M3** The result depends only on the file's bytes, not its name or location.
- **M4** A book that declares encrypted content other than obfuscated fonts is
  reported as protected. Its metadata is still returned so it can be imported
  and shown as unreadable.

## Reading

- **R1** A book's chapters are its reading-order items in package order.
  Each has its index, the first contents label that points into it, and whether
  the book marks it as outside the main reading line. Items the package does
  not declare are left out.
- **R2** A chapter is returned as blocks in document order: paragraphs,
  headings with their level, list items with depth and number, quotations,
  preformatted text, separators and picture placeholders. Text inside a block
  is split into runs flagged bold, italic, code, superscript or subscript.
- **R3** Whitespace in the source is folded: runs of spaces, tabs and line
  breaks become one space, and blocks do not start or end with one.
  Non-breaking spaces and forced line breaks are kept. Preformatted text keeps
  its spacing. Named and numeric character references are replaced. Control
  characters are removed. Documents in UTF-8, UTF-16, ISO-8859-1 and
  Windows-1252 are decoded; HTML-style markup with unclosed or upper-case tags
  is read.
- **R4** A link is returned as data on its run of text: a chapter index and
  optional anchor for a link inside the book, or the address for an `http`,
  `https` or `mailto` link. Other links are dropped and their text kept.
  Nothing is fetched or opened.
- **R5** The table of contents comes from the EPUB 3 navigation document, else
  the EPUB 2 NCX, as a flat list in order with a nesting depth. An entry that
  points into the reading order has a chapter index and optional anchor; a
  heading-only entry or one that points elsewhere has no target. A chapter
  reports which block an anchor falls in. A book with no usable contents is
  still read by chapter.
- **R6** Scripts, style sheets, frames, embedded objects, audio, video, event
  handlers, hidden elements and remote references are not interpreted and are
  not returned. A picture becomes a placeholder block with its text
  alternative; picture data and addresses are not returned.
- **R7** A reading position is a chapter index and block index with the text
  form `epubpos1:<chapter>:<block>`. The interface stores it and passes it back
  after a restart. A position past the last chapter becomes the start of the
  last chapter. Malformed position text is rejected.
- **R8** A chapter whose markup is damaged returns the text that could be read
  and a malformed-markup warning. A reading-order item that is not a text
  document returns no text and an unsupported-content warning, or a picture
  placeholder when it is an image.
- **R9** A chapter with more blocks than the limit (50,000 by default) is cut
  there and flagged as truncated.
- **R10** A sixty-chapter, 600,000-word book opens and every chapter is read in
  well under thirty seconds in a debug build.

## Refusals and limits

Each failure reaches the interface as a distinct error whose text can be shown
to the reader.

- **F1** A file that is not a ZIP archive, or has no `META-INF/container.xml`,
  is refused as not an EPUB.
- **F2** A missing file, a directory or an unreadable file is refused before
  any sandbox starts.
- **F3** Opening a protected book (M4), or one with encrypted ZIP entries, is
  refused as protected. Obfuscated fonts alone do not prevent reading.
- **F4** A book with no package document, no readable reading order or an
  unparseable package is refused as damaged. A chapter file missing from the
  archive fails only that chapter. A chapter index past the end is refused.
- **F5** A reference that climbs out of the container or names a host file or
  remote address is never followed. XML document type declarations are not
  processed, so entities defined by the book are left as written.
- **F6** Limits, with defaults: file size 512 MiB; 20,000 archive entries;
  8 MiB for a package or navigation document and 16 MiB for a chapter, counted
  on unpacked output rather than the size the archive declares; 10,000
  reading-order items; 20,000 contents entries; a 96 MiB reply. Passing one
  gives a limit error. Compression other than stored and deflate is refused as
  unsupported.
- **F7** The helper limits itself to 768 MiB of address space and to the
  call's time allowance in CPU seconds. The host stops the sandbox after 15
  seconds of wall-clock time. A helper stopped by any of these is reported and
  the next call works.
- **F8** If the file's size or modification time changes after a book is
  opened, further chapter reads are refused until it is opened again.
