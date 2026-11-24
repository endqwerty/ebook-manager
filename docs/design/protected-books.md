# Protected books

For [recognising protected files and saying why they cannot be opened](https://github.com/endqwerty/ebook-manager/issues/42).
Requirements [FMT-10 and FMT-11](../requirements/02-content-types.md) and
[LAW-1 and LAW-2](../requirements/10-constraints.md).

## Problem

A reader who buys an EPUB from most stores, or borrows one from a library,
gets a file locked by the seller's protection scheme. Today the Linux app
refuses to import such a file with "This EPUB is protected. Choose a DRM-free
book." The reader loses track of a book they own, and the message does not say
which scheme locked it, so they cannot tell where it can be read.

The requirements ask for the opposite: keep the book in the library, marked as
locked, with whatever metadata and cover are not encrypted; name the scheme;
say that reading, conversion and editing its text will not work; and offer no
way around the lock.

## Goals

- Import a protected EPUB as a book like any other: its managed copy, its
  metadata from the package, and its cover when the cover file is not
  encrypted (FMT-10).
- Name the scheme from what the file declares (FMT-10, LAW-2).
- Show the book as locked wherever books are shown, and say so in its spoken
  name (FMT-10).
- When the reader asks to read a locked book, say which scheme locks it and
  that the app cannot open it, and offer nothing else (LAW-2).
- Contain no code, link or text that removes or explains how to remove
  protection (FMT-11, LAW-1).

## Non-goals

- Reading any protected book, including Readium LCP with a licence and
  passphrase (FMT-12, LAW-3, later).
- Protected MOBI, AZW3 and other formats. The app imports only EPUB. The
  converter already refuses a protected MOBI 7 as `Protected`
  ([conversion spec](../../specs/mobi-to-epub.md)).
- Detecting a scheme that declares nothing in the container. Such a file shows
  up as a book whose chapters cannot be read, as any damaged file does.

## Plan

### Which scheme

The EPUB container format reserves two files for this
([OCF](https://www.w3.org/TR/epub-34/#sec-container-metainf)):
`META-INF/encryption.xml` lists every encrypted resource, and
`META-INF/rights.xml` holds rights-management information in a format the
standard leaves to each vendor. Neither may itself be encrypted. A scheme is
recognised from these and one other published location, checked in this order:

| Scheme | The file declares | Source |
|---|---|---|
| Readium LCP | `META-INF/license.lcpl` | [Readium LCP 1.0](https://readium.org/lcp-specs/releases/lcp/latest), which fixes the licence's place in the container and forbids encrypting it |
| Adobe | `META-INF/rights.xml` whose root element's namespace is on `ns.adobe.com` | OCF reserves the file; Adobe's scheme is the one that writes it with Adobe's namespace |
| Apple | `META-INF/sinf.xml` | Apple Books store files; no published Apple description was found, so this is named only from the file's presence |
| Password-protected archive | A ZIP entry with the ZIP encryption flag set | ZIP format |
| Not recognised | `rights.xml` of another kind, or `encryption.xml` listing a resource encrypted with anything but font obfuscation | OCF |

A book with only obfuscated fonts in `encryption.xml` is not protected, as
today ([reader spec M4](../../specs/epub-reader-mvp.md)).

The reader helper already decides `protected` while it reads the package. It
returns a `Protection` value in its place: `None`, or one of the five kinds
above. Reading `rights.xml` uses the same bounded XML reader as
`encryption.xml`, so a hostile file meets the same limits.

### What is read from a locked book

The package document must not be encrypted, so the title, authors, series,
language and identifier are read as for any book. The survey reads the cover
when the cover's file is not listed in `encryption.xml` and is not in an
encrypted ZIP entry, and reports no word count. Chapters, the table of
contents and pages are not read: calls that need them answer `Protected`, as
today.

### Where it is kept

`ParsedMetadata` gains `protection`, the scheme's name as a short stable string
(`lcp`, `adobe`, `apple`, `zip`, `unknown`), absent for an unprotected book. It
is written to the portable record with the rest of the imported metadata, so
it survives a rebuild and a restore. Older records without it read as
unprotected. A book imported before this change was refused, so none needs
reviewing.

The catalogue gains a `protection` column, and its version goes from 1 to 2.
A catalogue of version 1 is set aside and rebuilt from the portable records, as
the catalogue already does for an older version. `BrowseRow` carries the
scheme so the views can show it without another read.

Replacing a book's file keeps the rule simple: the protection is that of the
file now kept.

### What the reader sees

- **Views.** A locked book shows the platform's closed-lock symbol at the
  trailing end of the author line, in the slot the finished check mark uses
  and in the same secondary colour. A locked book is never finished, so the
  slot holds one mark. The shelf spine has no author line and shows nothing
  extra. The table's Progress column shows the lock. Every view's spoken name
  ends with "Locked by" and the scheme, for example "Locked by Adobe".
- **Read.** Read on a locked book, by menu, key or double-click, shows a
  message instead of starting the reader: "*Title* is locked by Adobe. Ebook
  Manager can't open books locked by this scheme." For a scheme it does not
  recognise: "*Title* is locked by a protection scheme Ebook Manager does not
  recognise." The message has one button, OK. No link, no further help.
- **Import.** A locked book imports like any other and the import report
  counts it as added. It is not an error.
- **Send to device.** Allowed. Copying the managed file is not reading or
  changing it, and some e-readers open these schemes themselves. Nothing is
  promised about whether the device can open it.
- **Edit.** Editing the library's title, authors and series is allowed. It does
  not touch the file.

The scheme names shown are: "Readium LCP", "Adobe", "Apple",
"a password", and "a protection scheme Ebook Manager does not recognise".

### Risks and limits

- A file can claim a scheme it does not use. The name only explains a refusal;
  nothing is unlocked or trusted because of it.
- Rebuilding the catalogue once after the update reads every portable record.
  Its time at 100,000 books is measured with the scale check and reported in
  the pull request.
- `rights.xml` is untrusted input. It goes through the helper's sandbox and
  XML limits like the package does.
- Apple's marker is unsourced beyond secondary descriptions. If it is wrong,
  such a file falls back to "not recognised" through `encryption.xml`, which
  is still a correct refusal.

## Behaviour

1. When a protected EPUB is imported, then the library holds it with its
   package metadata and the import report counts it as added. Test: a desktop
   import test with a generated LCP-marked EPUB.
2. When the file has `META-INF/license.lcpl`, then its protection is Readium
   LCP; with Adobe's `rights.xml`, Adobe; with `META-INF/sinf.xml`, Apple; with
   an encrypted ZIP entry, a password; with only another encrypted resource,
   not recognised. Test: reader crate tests, one generated file per case, in
   the order of the table, including a file with two markers.
3. When `encryption.xml` lists only obfuscated fonts, then the book is not
   protected. Test: the existing reader test for M4 still passes.
4. When the cover file of a locked book is not encrypted, then the survey
   returns the cover and no word count; when it is encrypted, neither. Test:
   reader crate survey tests.
5. When a locked book is shown in any view, then the lock symbol sits in the
   author line's trailing slot (table: Progress column) and the spoken name
   ends "Locked by …". Test: a unit test of the spoken name and the smoke
   driver reading the accessible name in the grid.
6. When the reader asks to read a locked book, then the message names the
   scheme and no reader opens. Test: smoke driver step.
7. When the catalogue on disk is version 1, then it is rebuilt and a locked
   book's protection is shown. Test: a library crate test opening a version 1
   catalogue.
8. When the portable record has no `protection` field, then the book is
   unprotected. Test: a library crate test reading such a record.

## Alternatives

- **Keep refusing at import, with a better message.** Simpler, but loses the
  book from the library, which FMT-10 and LAW-2 reject.
- **A separate "locked" table or flag outside the imported metadata.**
  Protection is a fact of the file, like its language, and changes only when
  the file does, so it belongs with what was read from the file.
- **A badge on the cover.** The interface's Quiet rule allows no badges, and
  the author-line slot already carries one mark for the book's state.

## Implementation

- `crates/ebook-reader`: `Protection` in `model.rs` and the wire; scheme
  detection in `engine/package.rs`; survey cover for locked books in
  `engine/survey.rs`; README and `specs/epub-reader-mvp.md` (M4, F3).
- `crates/ebook-library`: `ParsedMetadata.protection`, catalogue column and
  version, `BrowseRow.protection`; README.
- `apps/desktop`: import keeps protected books (`imports.rs`); lock mark and
  spoken name (`views/`); Read refusal (`window.rs` or `reader.rs`); smoke
  steps; `specs/desktop-mvp.md`.
- `docs/requirements/02-content-types.md` and `10-constraints.md`: mark
  FMT-10, FMT-11, LAW-1 and LAW-2 done.

## Tasks

1. Reader: scheme detection and survey cover, with tests (2, 3, 4). Open.
2. Library: field, column, version, row, with tests (7, 8). Open.
3. Desktop: import, views, Read refusal, smoke steps (1, 5, 6). Open.
4. Specs, requirements and READMEs. Open.

## Open questions

- Should the shelf spine show the lock? Assumed not: a spine has no slot for
  marks, and the spoken name carries it.
