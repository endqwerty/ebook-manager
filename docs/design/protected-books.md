# Protected books

For [recognising protected files and saying why they cannot be opened](https://github.com/endqwerty/ebook-manager/issues/42).
Requirements [FMT-10 and FMT-11](../requirements/02-content-types.md) and
[LAW-1 and LAW-2](../requirements/10-constraints.md).

## Problem

A reader who buys an EPUB from most stores, or borrows one from a library,
gets a file locked by the seller's protection scheme. Today the Linux app
refuses to import such a file with "This EPUB is protected. Choose a DRM-free
book." A file whose package is itself encrypted fails even earlier, because
the reader helper cannot read its metadata. Either way the reader loses track
of a book they own, and nothing says which scheme locked it, so they cannot
tell where it can be read.

The requirements ask for the opposite: keep the book in the library, marked as
locked, with whatever metadata and cover are not encrypted; name the scheme;
explain that reading, conversion and editing its text will not work; and offer
no way around the lock.

## Goals

- Import a protected EPUB as a book like any other: its managed copy, its
  metadata from the package when the package is readable, and its cover when
  the cover file is not encrypted (FMT-10).
- Name the scheme from what the file declares (FMT-10, LAW-2).
- Show the book as locked wherever books are shown, with the explanation one
  pointer or screen reader away (FMT-10).
- When the reader asks to read a locked book, say which scheme locks it and
  that the app cannot open it, and offer nothing else (LAW-2).
- Contain no code, link or text that removes or explains how to remove
  protection, and say so as project policy (FMT-11, LAW-1).

## Non-goals

- Reading any protected book, including Readium LCP with a licence and
  passphrase (FMT-12, LAW-3, later).
- Protected MOBI, AZW3 and other formats. The app imports only EPUB. The
  converter already refuses a protected MOBI 7 as `Protected`
  ([conversion spec](../../specs/mobi-to-epub.md)).
- Detecting a scheme that declares nothing in the container. Such a file shows
  up as a book whose chapters cannot be read, as any damaged file does.
- Book details and Needs attention. Neither screen exists in the Linux app
  yet. When they are built, a locked book shows its lock and explanation there
  too, as [the dialogs and messages rules](interface-screens.md#dialogs-and-messages)
  ask for any one-book problem.

## Plan

### Which scheme

The EPUB container format reserves two files for this
([OCF](https://www.w3.org/TR/epub-34/#sec-container-metainf)):
`META-INF/encryption.xml` lists every encrypted resource, and
`META-INF/rights.xml` holds rights-management information in a format the
standard leaves to each vendor. Neither may itself be encrypted. A scheme is
recognised from these and from two other markers, checked in this order. The
first that matches names the scheme.

| Scheme | The file declares | Source |
|---|---|---|
| Readium LCP | `META-INF/license.lcpl` | [Readium LCP 1.0](https://readium.org/lcp-specs/releases/lcp/latest), which fixes the licence's place in the container and forbids encrypting it |
| Adobe | `META-INF/rights.xml` whose root element declares a namespace (`xmlns` or `xmlns:*` attribute) with `ns.adobe.com` in its URI | OCF reserves the file; Adobe's scheme writes it in Adobe's namespace. The helper's XML reader drops prefixes and keeps `xmlns` attributes, so the attribute values are what is checked |
| Apple | `META-INF/sinf.xml` | Apple Books store files. No published Apple description was found, so this is named only from the file's presence |
| A password | Any ZIP entry with the ZIP encryption flag set | ZIP format |
| Not recognised | `rights.xml` of another kind, or `encryption.xml` listing a resource with an algorithm other than font obfuscation | OCF |

A book with only obfuscated fonts in `encryption.xml` is not protected, as
today ([reader spec M4](../../specs/epub-reader-mvp.md)).

The reader helper's `Metadata.protected: bool` becomes
`Metadata.protection: Option<Protection>`, an enum of the five kinds. The
helper decides it while it reads the package, as it decides `protected` today.
Reading `rights.xml` uses the same bounded XML reader and package byte limit as
`encryption.xml`, inside the same sandbox.

### A file whose package cannot be read

A password-protected ZIP often encrypts every entry, the container file and
the package included. Today `inspect` then fails with `Protected` before any
metadata exists. Instead, when the archive has an encrypted entry and the
container file or the package cannot be read because it is encrypted,
`inspect` returns metadata with protection "a password" and nothing else.
Import then names the book after its file name, as it already does for a
package without a title. Every other call on such a file still answers
`Protected`.

### What is read from a locked book

When the package is readable (every scheme but a fully encrypted archive),
the title, authors, series, language and identifier are read as for any book.
The package keeps the set of paths `encryption.xml` lists, resolved from the
container root and percent-decoded, whatever their algorithm. The survey reads
the cover when its path is not in that set and its ZIP entry is not
encrypted, and reports no word count. Chapters, the table of contents and pages
are not read: calls that need them answer `Protected`, as today.

### Where it is kept

`ParsedMetadata` gains `protection`, the scheme as a short stable string
(`lcp`, `adobe`, `apple`, `password`, `unknown`), absent for an unprotected
book. It is written to the portable record with the rest of the imported
metadata, so it survives a rebuild and a restore. Older records without it read
as unprotected. A book imported before this change was refused, so none needs
reviewing.

The catalogue's `book` table gains a `protection` column. The catalogue version
stays 1: setting a version 1 catalogue aside would leave its device-local
source paths and unpublished changes behind
([library README](../../crates/ebook-library/README.md)). Instead, opening a
catalogue whose `book` table has no `protection` column adds it with
`ALTER TABLE` and clears the stored projection profile, so the existing
re-projection on open fills every row from its record. `BrowseRow` carries the
scheme so the views can show it without another read.

Replacing a book's file keeps the rule simple: the protection is that of the
file now kept.

### What the reader sees

One phrase names the lock everywhere: "Locked by" and the scheme's name,
"Readium LCP", "Adobe", "Apple", "a password", or "an unrecognised scheme".
One sentence explains it: "Ebook Manager can't read, convert or change the
text of books locked this way."

- **Grid.** The platform's closed-lock symbol (`changes-prevent-symbolic` on
  Linux), 12 units and dimmed like the finished mark, sits at the trailing end
  of the author line in the finished mark's slot. A locked book has no reading
  progress, so the slot holds one mark.
- **List.** The same symbol sits in the row's trailing slot beside the date
  or percentage, where the finished mark goes, whatever the sort.
- **Table.** The Progress column shows the symbol in place of a percentage.
- **Shelf.** The spine shows nothing extra.
- **Spoken names and tooltips.** The grid's and list's item, the shelf's spine
  and, new, each table row have a spoken name that ends with the lock phrase,
  for example "Moby-Dick, Herman Melville, Locked by Adobe". It takes the place
  of the reading state, which a locked book does not have. The lock symbol's
  tooltip is the lock phrase and the sentence.
- **Read.** Read on a locked book, by menu, key or double-click, starts no
  reader and shows the platform's alert. Title: "Can't open *Title*". Message:
  the lock phrase as a sentence ("It is locked by Adobe.") and the explanation
  sentence. One button, Close, the default for Return and Escape. No link and
  no other action. [Interface screens](interface-screens.md#dialogs-and-messages)
  gains this case, because its rules cover confirmations and background
  problems but not a request that cannot be carried out at all.
- **Import.** A locked book imports like any other and the import report
  counts it as added.
- **Send to device.** Allowed. Copying the managed file is not reading or
  changing it, and some e-readers open these schemes themselves. Nothing is
  promised about whether the device can open it.
- **Edit.** Editing the library's title, authors and series is allowed. It does
  not touch the file.

### No way around the lock

The README's project section states the policy that covers the docs and the
issue tracker: the project does not ship, link to, host or describe a way to
remove protection, and closes issues and pull requests that ask for one. In
the app, the only text about a lock is the phrase, the sentence and the alert
above, and a test checks the alert's text and buttons.

### Risks and limits

- A file can claim a scheme it does not use. The name only explains a refusal;
  nothing is unlocked or trusted because of it.
- Adding the column re-projects every book once, on the first start after the
  update. Its time at 100,000 books is measured with the scale check and
  reported in the pull request.
- `rights.xml` is untrusted input. It goes through the helper's sandbox and
  XML limits like the package does.
- Apple's marker is unsourced beyond secondary descriptions. If it is wrong,
  such a file falls back to "an unrecognised scheme" through `encryption.xml`,
  which is still a correct refusal.

## Behaviour

1. When a protected EPUB with a readable package is imported, then the library
   holds it with its package metadata and protection, and the import report
   counts it as added. Test: desktop import unit test with a generated
   LCP-marked EPUB.
2. When every entry of an EPUB is ZIP-encrypted, then `inspect` returns
   protection "a password" and import keeps the book under its file name.
   Test: reader crate test and desktop import unit test.
3. When the file has `META-INF/license.lcpl`, then its protection is Readium
   LCP; with Adobe's `rights.xml`, Adobe; with `META-INF/sinf.xml`, Apple; with
   an encrypted ZIP entry, a password; with only another encrypted resource or
   another `rights.xml`, an unrecognised scheme. A file with two markers takes
   the first in the table's order. Test: reader crate tests, one generated file
   per case.
4. When `encryption.xml` lists only obfuscated fonts, then the book is not
   protected. Test: the existing reader test for M4 still passes.
5. When the cover file of a locked book is not encrypted, then the survey
   returns the cover and no word count; when it is listed in
   `encryption.xml`, neither. Test: reader crate survey tests.
6. When a locked book is shown, then the grid, list, shelf and table names end
   with "Locked by …" and the lock symbol shows in the grid, list and table.
   Test: unit tests of the shared phrase and of each view's name, and the
   smoke driver reading the grid's and shelf's accessible names.
7. When the reader asks to read a locked book, then the alert names the
   scheme and the explanation, has the one response Close, contains no link,
   and no reader opens. Test: unit test of the alert's text and responses, and
   a smoke driver step.
8. When the catalogue on disk has no `protection` column, then opening adds it
   and a locked book's protection is shown, with source paths kept. Test:
   library crate test that opens a catalogue made by the previous schema.
9. When the portable record has no `protection` field, then the book is
   unprotected. Test: library crate test reading such a record.

## Alternatives

- **Keep refusing at import, with a better message.** Simpler, but loses the
  book from the library, which FMT-10 and LAW-2 reject.
- **Raise the catalogue version.** The rebuild is already written, but it
  leaves source paths and unpublished changes in the old file.
- **Read the scheme from the record's JSON for each browse row.** No schema
  change, but every page of rows would parse every record.
- **A badge on the cover.** The interface's Quiet rule allows no badges, and
  the author-line slot already carries one mark for the book's state.
- **Disable Read for a locked book.** Quiet, but a double-click would do
  nothing, and LAW-2 asks the app to say it cannot open the file when the
  reader tries.

## Implementation

- `crates/ebook-reader`: `Protection` in `model.rs` and the wire; scheme
  detection, encrypted path set and the unreadable-package case in
  `engine/package.rs`; survey cover in `engine/survey.rs`; `readable()` in
  `engine/run.rs`; tests in `tests/failures.rs`; README and
  `specs/epub-reader-mvp.md` (M4, F3, V1 and the per-call unpack list).
- `crates/ebook-library`: `ParsedMetadata.protection`, the column in
  `schema.sql` and `project.sql`, the in-place column in `catalogue.rs`,
  `BrowseRow.protection`; README and `specs/library-mvp.md`.
- `apps/desktop`: import keeps protected books (`imports.rs`); the lock
  phrase, mark and names (`views/mod.rs`, `grid.rs`, `list.rs`, `table.rs`,
  `shelf/spine.rs`); the Read alert (`window.rs` `read()`); smoke steps;
  `specs/desktop-mvp.md`.
- `docs/design/interface-screens.md`: the alert case.
- `README.md`: the policy sentence.
- `docs/requirements/02-content-types.md` and `10-constraints.md`: mark
  FMT-10, FMT-11, LAW-1 and LAW-2 done.

## Tasks

1. Reader: scheme detection, unreadable package, survey cover, with tests
   (2, 3, 4, 5). Open.
2. Library: field, column, row, with tests (8, 9). Open.
3. Desktop: import, views, Read alert, smoke steps (1, 2, 6, 7). Open.
4. Specs, requirements, READMEs and interface page. Open.

## Open questions

- Should the shelf spine show the lock? Assumed not: a spine has no slot for
  marks, and the spoken name carries it.
