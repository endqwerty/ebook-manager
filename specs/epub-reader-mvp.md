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
  A helper that cannot set its own resource limits (F7) counts as not isolated.
- **S2** The helper checks its confinement at the start of every run and
  refuses to open a book when it can see `/proc`, `/home`, `/etc`, `/tmp`,
  `/run`, `/var`, `/root`, `/sys` or `/dev`, can write to its filesystem, has
  a network route, or can start another process.
- **S3** Inside the sandbox the helper can read the one book file, itself and
  the host's library directories. It has no network, no other host files, no
  writable storage, no capabilities and an empty environment. It inherits
  none of the application's open files, sockets or other descriptors beyond
  its three standard streams, cannot create further user namespaces, and is
  killed when the application exits, including while the sandbox is starting.
- **S4** A configured helper path is used alone: if nothing is there, no other
  place is tried. Without one, the helper is found, in order, beside the
  running program or in `../libexec/ebook-manager/` relative to it. A debug
  build first tries the absolute path in `EBOOK_READER_HELPER`; a release
  build ignores that variable. `PATH` is not searched. `bwrap`
  is used from the configured path, `/usr/bin/bwrap` or `/bin/bwrap`.
- **S5** Each call runs one helper process that ends with the call. A helper
  that fails affects only that call.
- **S6** The helper's reply is checked before the interface sees it. A book
  outline with no chapters, or with more chapters or contents entries than
  the limits allow, is refused. Chapter indices are renumbered, contents
  targets outside the book are removed, nesting depth and heading levels are
  brought into range, a chapter with more blocks than the limit is cut and
  flagged, links other than the kinds in R4 are removed, and control
  characters are stripped from every string.

## Import metadata

- **M1** Inspecting a file returns its title, authors, identifiers, languages
  and EPUB version from the package document, with whitespace collapsed.
  A book with several titles gives the one marked as the main title, else the
  first. Every text value, the version and identifier schemes included, is
  cut at 2,000 characters and each list at 64 entries.
- **M2** Authors are the creators with the author role or no role, in package
  order, each with the publisher's sort name when given. When no creator
  qualifies, all creators are returned. Each identifier has its value, a
  lower-case scheme taken from the scheme attribute or a `urn:` or `isbn:`
  prefix, and a flag for the package's unique identifier.
- **M3** The result depends only on the file's bytes, not its name or location.
- **M4** A book that declares encrypted content other than obfuscated fonts is
  reported as protected. Its metadata is still returned so it can be imported
  and shown as unreadable.

- **M5** Series come from EPUB 3 `belongs-to-collection` entries whose
  `collection-type` is `series`, in package order, each with its
  `group-position` as text when given. Collections of another type are left
  out. An EPUB 2 book's `calibre:series` and `calibre:series_index` `meta`
  names add a series when its name is not already listed; an index written
  as a whole number with a zero fraction, such as `3.0`, is given as `3`.
  The first series is the primary one.
- **M6** The publication date is the `dc:date` text as written, with
  whitespace collapsed: the one marked with the publication event when an
  EPUB 2 book marks several, else the first with no event. A date marked with
  another event is never used. A book whose spine says
  `page-progression-direction="rtl"` is flagged as right to left.

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
  Non-breaking spaces and forced line breaks inside a block are kept; a forced
  break at the very end of a block is dropped. Preformatted text keeps its
  spacing, including spaces at the ends of lines, and loses only the line
  breaks before its first and after its last line. Each table row is one block
  with a tab before every cell after the first, so an empty cell keeps its
  column wherever it stands. Paragraphs and other blocks inside a cell are
  folded into the row's line with a space on either side. A row with nothing
  in it is left out. A table inside a cell is not kept in that row: it ends
  the outer row's block, gives its own rows as blocks, and what follows in the
  outer row starts a new block. Named and numeric character references are replaced. Control
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
  heading-only entry or one that points elsewhere has no target. Entries
  nested more than 32 levels deep are left out. A chapter reports which block
  an anchor falls in, or that it has no such anchor. A book with no usable contents is
  still read by chapter.
- **R6** Scripts, style sheets, frames, embedded objects, audio, video, event
  handlers, hidden elements and remote references are not interpreted and are
  not returned. A picture that stands alone in its block becomes a placeholder
  block with its text alternative. A picture inside a line of text adds its
  text alternative to that line, so a decorative initial still spells its word.
  Picture data and addresses are not returned. A chapter carries a
  remote-references warning when one of its elements names a remote address as
  a source, linked resource, poster, object data or source-set candidate, or
  when a style attribute or style element has a `url()` or `@import` whose
  target is one. Nothing is fetched.
  Style sheet files are not inspected, and ordinary hyperlinks (R4) do not
  count.
- **R7** A reading position is a chapter index and block index with the text
  form `epubpos1:<chapter>:<block>`. The interface stores it and passes it back
  after a restart. A position past the last chapter becomes the start of the
  last chapter. A block index past the end of its chapter becomes the last
  block, and a chapter with no blocks has no position to scroll to. Malformed
  position text is rejected.
- **R8** Unclosed and mismatched tags are repaired without comment, as in
  HTML. Markup that cannot be read as tags at all returns the text around it
  and a malformed-markup warning. A reading-order item that is a picture,
  raster or SVG, returns one picture placeholder. Any other item that is not a
  text document returns no text and an unsupported-content warning.
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
- **F3** Opening a protected book (M4) is refused as protected. A book with a
  ZIP-encrypted entry anywhere in its archive counts as protected from the
  moment it is opened or inspected. Obfuscated fonts alone do not prevent
  reading.
- **F4** A book with no package document, a package document in which no
  package element can be found, or no readable reading order is refused as
  damaged. A chapter or picture file missing from the archive fails only that
  chapter. A chapter index past the end is refused.
- **F5** A reference that climbs out of the container or names a host file or
  remote address is never followed. XML document type declarations are not
  processed, so entities defined by the book are left as written.
- **F6** Limits, with defaults: file size 512 MiB; 20,000 archive entries;
  8 MiB for a package or navigation document and 16 MiB for a chapter, counted
  on unpacked output rather than the size the archive declares; 10,000
  reading-order items; 20,000 contents entries; 400,000 elements and text runs
  in a package or contents document; 64 MiB unpacked in one call, counting
  entries that failed part-way; a 200-to-1 unpacked-to-packed ratio for any
  entry that unpacks to more than 4 MiB; a 96 MiB reply. The entry count is
  read from the archive's end record before its directory is parsed. Passing a
  limit gives a limit error; in particular, contents that are too long or too large
  are an error and never a shortened list. One call unpacks at most the
  container file, the encryption list, the package, the two contents documents
  and one chapter, so the per-document limits also bound the total. Nothing is
  unpacked to disk: archive entry names are only looked up, a looked-up name
  never starts with `/` or contains `..`, and an entry marked as a symbolic
  link is read as ordinary bytes. Compression other than stored and deflate is
  refused as unsupported, for a contents document as for any other.
- **F7** Before reading the book the helper limits itself to 768 MiB of
  address space, the call's time allowance plus two in CPU seconds, 32 open
  files, no core dump and no further processes or threads, and does not run
  if a limit cannot be set. Where the application already runs under a lower
  limit, soft or hard, that limit is kept. Because the helper cannot start another
  process, the address-space limit bounds the whole sandbox. The helper is
  built so that arithmetic overflow stops it instead of wrapping, and a test
  checks the built helper for that. The host stops the sandbox after 15
  seconds of wall-clock time. A helper stopped by any of these is reported and
  the next call works.
- **F8** If the file's size or modification time changes after a book is
  opened, further chapter reads are refused until it is opened again.
