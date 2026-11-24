# EPUB reader (MVP)

The `ebook-reader` crate opens a DRM-free EPUB 2 or EPUB 3 file on Linux or macOS and
gives the desktop interface what it needs to import, browse and read it: title,
authors, identifiers, series, date and page direction; a word count and a small
cover picture; the chapters in reading order; the table of contents; and each
chapter as blocks of styled text or as sanitised markup with its pictures.
Book files are untrusted, so all parsing and image decoding happens in a
helper process inside a sandbox with no network: bubblewrap on Linux,
`sandbox-exec` on macOS ([S7](#sandbox-on-macos)). The interface
receives text, flags and checked thumbnail pixels, and, from the two content
calls alone, markup the helper wrote and undecoded picture bytes for a web
view. When the sandbox cannot be set up,
nothing is opened. The caller-facing contract is in the
[crate README](../crates/ebook-reader/README.md).

This covers the engine side of `READ-1` (contents and a position to resume
from) and `SEC-1` to `SEC-5` for EPUB text, and the helper's part of
[browsing a large library](../docs/design/library-browsing.md). Not covered
yet: pictures inside chapters, style sheets and fonts,
pagination, search, positions finer than a block, footnote pop-ups, fixed-layout
and media-overlay books, and every platform other than native Linux packages
and an unsandboxed macOS application.

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
- **S5** An opened book reads through one helper process, which ends when
  the book is dropped, whichever thread opened it. A call that finds that
  helper busy for more than 50 ms runs in a helper of its own; at most four
  such helpers run for one book at once, and a call past that waits for
  either until its deadline. `inspect` and `survey` each run one helper that
  ends with the call. A helper that fails, sends a reply the host refuses,
  sends an error no working helper sends, stops reading requests or passes
  the deadline is ended, and the book's next call starts another. A call
  whose helper ends by itself, in time, after answering an earlier call is
  tried once more in a fresh helper; no other call is. Each call has one
  deadline, from when it is made, covering any wait, the helper's start, the
  reply and a fresh try. The host reads only the replies it asked for, so a
  helper that writes more fills its own pipe and not the application's
  memory. The helper stops at a request line longer than any request. The
  sandbox check `Reader::new` makes is made once per helper, `bwrap` and
  limits in a process; every helper repeats its own check as it starts.
- **S6** The helper's reply is checked before the interface sees it. A book
  outline with no chapters, or with more chapters or contents entries than
  the limits allow, is refused. Chapter indices are renumbered, contents
  targets outside the book are removed, nesting depth and heading levels are
  brought into range, a chapter with more blocks than the limit is cut and
  flagged, links other than the kinds in R4 are removed, and control
  characters are stripped from every string.

## Sandbox on macOS

S1, S4, S5 and S6 hold on macOS as written, with `/usr/bin/sandbox-exec` in
the place of `bwrap`. S2 and S3 read as follows.

- **S7** The host starts the helper through `sandbox-exec` with a fixed
  profile that denies everything except running and reading the helper,
  reading the one book, reading the system's shared library cache and
  `/usr/lib`, listing the root directory and reading system settings
  (`sysctl`). The helper and the book are named to the profile as parameters
  by their resolved paths. The helper has no network, no other host files, no
  writable storage, an empty environment, a session of its own and none of
  the application's descriptors beyond its three standard streams, within the bounds S9 gives.
- **S8** The helper refuses to open a book when a lookup of `/Users`,
  `/etc`, `/tmp`, `/var`, `/private`, `/Library`, `/Applications`,
  `/Volumes`, `/dev`, `/bin`, `/opt` or `/System/Library/CoreServices`
  succeeds, when it can create a file under `/private/tmp` or
  `/private/var/tmp`, has a network route, or can start another process.
  It makes that last check before it sets its own process limit, so that
  limit is not what refuses. These checks are a tripwire for a missing
  profile, not a proof of the whole profile; `sandbox_sees_only_the_book`
  in `tests/failures.rs` checks what the profile lets the helper read.
- **S9** macOS has no parent-death signal. The helper ends when the
  application's end closes its input. A call in progress then runs until it
  finishes or its CPU limit (F7) stops it. To mark inherited descriptors close-on-exec, the launcher scans from
  descriptor 3 to the greater of the observed limit and the highest descriptor
  listed before launch plus 256 (exclusive). It also covers the child’s soft
  limit at startup. These bounds are capped at 1,048,576 when the limit query
  succeeds. A descriptor opened beyond the scan after listing can escape if
  the limit is lowered below it before the fork. The application does not
  change that limit.
- **S10** macOS refuses an address-space limit below what a process already
  maps, so F7's memory bound is kept by the helper's allocator: it counts the
  bytes of every allocation in use and refuses one that would pass
  `Limits::memory_bytes`. The helper then stops as F7 describes, or, when
  the refused request was one the unpacker made allowing for failure, refuses
  the book as damaged and carries on. The bound
  covers what the helper's own code allocates through that allocator. The
  allocator's bookkeeping, the stack, which the system caps at 8 MiB, and
  the system libraries mapped into it are outside the count. A helper built
  without that allocator counts as not isolated. An inherited address-space
  limit plays no part.

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
- **M4** A book that declares encrypted content other than obfuscated fonts,
  has a ZIP-encrypted entry, or carries `META-INF/license.lcpl`,
  `META-INF/rights.xml` or `META-INF/sinf.xml`, is reported as protected, with the scheme its
  container declares: Readium LCP for `META-INF/license.lcpl`, Adobe for a
  `META-INF/rights.xml` whose root element declares a namespace on
  `ns.adobe.com`, Apple for `META-INF/sinf.xml`, a password for a ZIP-encrypted
  entry, and unrecognised otherwise, the first that matches in that order. Its
  metadata is still returned so it can be imported and shown as locked. When
  the container file or the package is itself ZIP-encrypted, the metadata is
  empty and the scheme is a password. An encryption list or rights file that
  cannot be read, because it is encrypted, malformed, cut short or over a
  limit, still marks the book as locked. The package is read before these
  files, so a marker that does not fit the unpacking allowance never costs the
  metadata.
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

## Survey

A survey is one helper call that gives the facts a library view needs and
that take more than the package to find: a word count and a cover.

- **V1** The word count and the cover are independent: an unusable cover
  leaves the count, and an unknown count leaves the cover. Neither is ever an
  error. A survey fails only as other calls do (F1 to F7). A protected book
  (M4) gives an unknown count, and its cover only when the cover's file is
  neither listed in the encryption list nor in a ZIP-encrypted entry.
- **V2** The count covers the linear reading order. A word is a run of
  characters containing a letter or digit, ended by whitespace or the end of
  a block, so `don't` and `well-known` are one word each and a dash alone is
  none. Each Han, kana or hangul character counts as half a word; the halves
  of the whole book are added and rounded up once. A picture that stands on
  its own is not counted, nor is its text alternative. A picture in a
  block that also holds text counts as its text alternative, as it reads, because that is how a decorated first letter spells its
  word. A reading-order item that is a picture adds nothing. The count is unknown when a linear item names no
  manifest entry, is missing from the archive, is neither text nor a
  picture, cannot be decoded, passes a limit or is cut at the block limit
  (R9). Non-linear items are not read.
- **V3** The cover is the manifest item with the `cover-image` property,
  else the item an EPUB 2 `meta` named `cover` names by id or by file. It is
  recognised by its content as JPEG, PNG, GIF (first frame) or WebP, drawn
  over white where it is transparent, and scaled with its shape kept to fit
  256 by 384 by averaging areas. A cover smaller than that is enlarged up to
  three times. The result is the thumbnail's RGB pixels, their mean colour,
  and the spine colour: the most common colour, with each channel rounded to
  5 bits, on the tenth of the thumbnail's width at the bound side, given as
  the mean of the pixels with that colour. The bound side is the left, or the
  right for a right-to-left book (M6). A thumbnail under 10 pixels wide uses
  all its columns.
- **V4** There is no cover when none is declared, its file is missing, it is
  over 20 MiB unpacked, its header declares more than 40 million pixels, it
  is another type such as SVG, it fails to decode, it is a JPEG that a strict
  decode refuses, the decoder stops, or it
  would need enlarging more than three times. The pixel count is checked from
  the header before any pixel is decoded. A JPEG's segments and scans
  must lead one after another to its end marker, and it must pass a strict
  decode, before it is used, because the lenient decoder fills in the rest of a file that stops
  early. So a JPEG that stops anywhere before its end gives no cover. Bytes
  after the picture's end are ignored. Damage that this decoder accepts is
  not detected, and the cover is used as it decodes: a file cut and then
  given an end marker, which some stricter decoders refuse, or a changed
  byte inside the picture data.
- **V5** The host accepts a cover only when both sides are at least 1, the
  width at most 256, the height at most 384, and the pixel data is exactly
  width times height times 3 bytes, computed without overflow. Any other
  cover in a reply is refused as a helper failure. The caller cannot build an
  unchecked thumbnail.
- **V6** A 90,000-word book with a 1600 by 2400 JPEG cover is surveyed in
  under 5 seconds in a debug build.

## Content for a web view

A second form of each chapter, for a reader that lays the book out in a web
view. The view that shows it runs no script and loads nothing else. That is
the [desktop reader's](../docs/design/epub-reading.md#the-trust-boundary) job
and not this crate's.

- **C1** A chapter is returned as the children of an XHTML `body`. The helper
  writes every element, attribute and character itself, so the result is
  well-formed XML whatever the source was: unclosed and misnested tags are
  balanced, a repeated attribute keeps its first value, and a prefixed
  element is matched by its local name. Only these elements are written:
  `a abbr address article aside b bdi bdo blockquote br caption cite code col colgroup dd del details dfn div dl dt em figcaption figure footer h1 h2 h3 h4 h5 h6 header hgroup hr i img ins kbd li main mark nav ol p pre q rp rt ruby s samp section small span strong sub summary sup table tbody td tfoot th thead time tr u ul var wbr`.
  Only these attributes are written: `id`, `class`, `lang` (also from
  `xml:lang`), `dir`, `title`, `alt`, `colspan`, `rowspan`, `start`, `value`,
  and `epub:type` as `data-epub-type`, each value cut at 1,024 characters. An
  `id` longer than 256 characters is left out whole. In a value `&`, `<`, `>` and `"` are escaped, so `<` and `>`
  in the markup only ever open and close a tag. Any other element is left out
  and its content kept. With 400 elements open, a further one is left out the
  same way, and a picture there is replaced by its text alternative.
- **C2** Left out with their content: `script`, `style`, `template`, `iframe`,
  `frame`, `frameset`, `object`, `embed`, `applet`, `audio`, `video`,
  `canvas`, `svg`, `math`, `input`, `button`, `select`, `textarea`,
  `datalist`, `noscript`, `head`, `title`, and any element with a `hidden`
  attribute or an inline `display: none`, `html` and `body` included. An
  `svg` that is not hidden and whose only drawn element is one `image` naming
  a picture of the book becomes that picture. Other `svg` and
  `math` add the unsupported-content warning. A link to a place in the book
  is `href="/c/<chapter>#<anchor>"`, or `href="/c/<chapter>"` when it names no
  anchor or names one longer than 256 characters. The anchor is
  percent-encoded once. An
  `http`, `https` or `mailto` link has no `href` and carries its address in
  `data-external`. Any other link has neither. A picture that is a PNG, JPEG,
  GIF or WebP item of the manifest is `src="/r/<id>"`, and any other picture
  is replaced by its text alternative. The markup holds no other address.
  The remote-references warning is raised as in R6, for what is left out as
  much as for what is kept.
- **C3** Text keeps its whitespace. Line ends become a line feed, and
  control characters other than tab and line feed are removed. A chapter's
  length is the number of UTF-16 code units of text in its markup, text
  alternatives written as text included. A place in a chapter is a count of
  those units.
- **C4** A resource id is the position of an item among the manifest items
  that have an id and a path inside the book, counted from 0 in package
  order. When two items name one file, the first says what the file is: a
  reference resolves to it, and the file is a picture only if that item is.
- **C5** A resource is returned only when its manifest item is a PNG, JPEG
  (`image/jpeg` or `image/jpg`), GIF or WebP, it unpacks to at most 32 MiB within the limits of F6, and its
  first bytes are that type's signature. The host checks the signature and
  the size again. The bytes cross from the helper as they are and are not
  decoded. A reference in markup can therefore name a picture that is then
  refused. Its text alternative is in the markup's `alt`.
- **C6** Markup with more than 400,000 elements and text runs is cut there,
  closed so that it is still well-formed, and flagged as truncated.
- **C7** Each chapter of an opened book has the unpacked size the archive
  states for it, capped at the chapter size limit, and 0 when the archive
  lacks the file. The host brings a larger number down to the limit. The
  book controls that number. It is for weighing chapters
  when showing progress and for nothing else.
- **C8** A reading-order item whose own media type is a servable picture's,
  and whose file is one by C4, gives one `img`.
  Any other item that is not a text document gives empty markup and the
  unsupported-content warning. A chapter index past the end and a changed
  file are refused as for R1 to R9 (F4, F8). The host checks
  that the markup it receives is in the helper's own form and that the
  length is the count of its text, and refuses the reply otherwise. The form
  is text with `&amp;`, `&lt;` and `&gt;` as its only entities, and tags
  written as `<name attr="value">`, `<name attr="value"/>` and `</name>`,
  with only the elements and attributes of C1, no attribute twice, `href`
  and `data-external` only on `a`, alone or together, `href` only as
  `/c/<digits>` with an optional `#` and unreserved or `%` characters, `src`
  only on `img` and only as `/r/<digits>`, and double-quoted values that hold no `<`, `>` or
  `"`. A comment, a CDATA section, a processing instruction, a declaration,
  a namespace declaration, a character reference or a carriage return is
  refused. So is any other control character, except a tab or a line feed
  in text. The host does not check that tags are balanced. The
  title is the contents label, else the text of the first heading alone, else
  the document title. The language is the
  chapter's own, else the package's first. Right-to-left is set by
  `dir="rtl"` on `html` or `body`.

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
  entry that unpacks to more than 4 MiB; a 96 MiB reply; a 20 MiB cover file
  and 40 million cover pixels (V4). The entry count is
  read from the archive's end record before its directory is parsed. Passing a
  limit gives a limit error; in particular, contents that are too long or too large
  are an error and never a shortened list. A reading call unpacks at most the
  container file, the encryption list, the rights file, the package, the two
  contents documents and one chapter, so the per-document limits also bound the total. A book's
  helper keeps what it read, but charges each call for the container, the
  encryption list, the rights file, the package and, when the call uses them, the contents
  documents, as if it had read them again, so a call gets the same answer
  whichever helper serves it. A survey
  unpacks the cover first and then the linear chapters, all within the same
  64 MiB, so a book whose cover and text pass it has an unknown count. Nothing is
  unpacked to disk: archive entry names are only looked up, a looked-up name
  never starts with `/` or contains `..`, and an entry marked as a symbolic
  link is read as ordinary bytes. Compression other than stored and deflate is
  refused as unsupported, for a contents document as for any other.
- **F7** Before reading the book the helper limits itself to 768 MiB of
  address space, the call's time allowance plus two in CPU seconds, 32 open
  files, no core dump and no further processes or threads (the image decoders
  run on the helper's one thread), and does not run
  if a limit cannot be set. Where the application already runs under a lower
  limit, soft or hard, that limit is kept. Because the helper cannot start another
  process, the address-space limit bounds the whole sandbox. The helper is
  built so that arithmetic overflow stops it instead of wrapping, and a test
  checks the built helper for that. The host stops the sandbox after 15
  seconds of wall-clock time. The helper never crashes once its first step,
  which allocates nothing, has installed its handlers and unblocked their
  signals, whatever mask the parent left. After that, running out of memory or
  stack, or any other signal that would dump core, ends it with
  `helper: stopped after a fatal error` and exit status 4, so the desktop
  reports no crash. A test raises each such signal and overflows the stack. A
  helper stopped by any of these is reported and the next call works.
- **F8** If the file's size or modification time changes after a book is
  opened, further chapter reads are refused until it is opened again.
