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

Any of the three marker files makes a book locked even when nothing else is
listed as encrypted: Readium LCP forbids encrypting its licence so that a
protected publication can be recognised by it, and the other two exist only to
carry rights. A book with only obfuscated fonts in `encryption.xml` and none of
the markers is not protected, as today ([reader spec M4](../../specs/epub-reader-mvp.md)).

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
package without a title. Open, chapter, contents and resource calls answer
`Protected`, and a survey answers with no count and no cover.

A marker file that cannot be read counts as a lock, never as a broken book:
an `encryption.xml` or `rights.xml` that is ZIP-encrypted, malformed or over a
limit makes the book locked by a password (when ZIP-encrypted) or by an
unrecognised scheme.

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

The catalogue's `book` table gains a `protection` column, written by
`project.sql` and read by the browse query. The catalogue version stays 1:
setting a version 1 catalogue aside would leave its device-local source paths
and unpublished changes behind
([library README](../../crates/ebook-library/README.md)). Instead, opening an
existing version 1 catalogue whose `book` table has no `protection` column adds
it with `ALTER TABLE`. Every book in such a catalogue was imported before
protected books could be, so an empty column is already right and no row is
projected again. `BrowseRow` carries the scheme so the views can show it
without another read. The record's `protection` is an enum of the five names,
so a record with any other value is refused like other invalid metadata.

Replacing a book's file keeps the rule simple: the protection is that of the
file now kept.

A book imported before this change was imported only when the old check found
no encrypted resource, but a file carrying only a marker passed that check and
is stored unlocked. Its lock is found when it is read: the helper refuses to
open it as protected, the window inspects the file, records the scheme for that
version of the book (`Library::set_protection`) and shows the locked alert.
Rescanning every book at upgrade would cost a helper run per book for a rare
case.

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
  or percentage, where the finished mark goes. Unlike the finished mark, it
  shows whatever the sort, because it says the book cannot be read.
- **Table.** The Progress column shows the symbol in place of a percentage.
- **Shelf.** The spine shows nothing extra.
- **Spoken names and tooltips.** The grid's and list's item, the shelf's spine
  and, new, each table row have a spoken name that ends with the lock phrase,
  for example "Moby-Dick, Herman Melville, Locked by Adobe". It takes the place
  of the reading state, which a locked book does not have. The table's rows get
  it from a row factory that labels each `ColumnViewRow`. The lock symbol's
  tooltip is the lock phrase and the sentence, for a pointer. For the keyboard
  and a screen reader the name carries the phrase, and Read gives the
  sentence.
- **Read.** Read on a locked book, by menu, key or double-click, starts no
  reader and shows the platform's alert. Title: "Can't open *Title*". Message:
  the lock phrase as a sentence ("It is locked by Adobe.") and the explanation
  sentence. One button, Close, the default for Return and Escape. No link and
  no other action. The interface rules allow a message only for a problem that
  needs a decision and keep one-book problems off dialogs. A request the app
  cannot carry out at all is a third case: without an answer the reader's
  action would seem ignored. [The base page](interface.md)'s Quiet rule and
  messages rule, and [interface screens](interface-screens.md#dialogs-and-messages),
  gain that case in this change.
- **Import.** A locked book imports like any other and the import report
  counts it as added.
- **Send to device.** Allowed. Copying the managed file is not reading or
  changing it, and some e-readers open these schemes themselves. Nothing is
  promised about whether the device can open it.
- **Edit.** Editing the library's title, authors and series is allowed. It does
  not touch the file.

### No way around the lock

A new `CONTRIBUTING.md`, which GitHub links from the new-issue and
new-pull-request pages, and the README's project section state the policy for
the docs, the issue tracker and any future extension list: the project does not
ship, link to, host or describe a way to remove protection, and closes issues
and pull requests that ask for one. LAW-1 is marked done on that policy and on
the app's content; the app has no extension list yet. In the app, the only text
about a lock is the phrase, the sentence and the alert above, and a test checks
the alert's text and buttons. FMT-11 rests on the absence of any such code,
link or instruction, which each pull request's reviewers check in the diff.

### Risks and limits

- A file can claim a scheme it does not use. The name only explains a refusal;
  nothing is unlocked or trusted because of it.
- `rights.xml` is untrusted input. It goes through the helper's sandbox and
  XML limits like the package does.
- Apple's marker is unsourced beyond secondary descriptions. If it is wrong,
  such a file falls back to "an unrecognised scheme" through `encryption.xml`,
  which is still a correct refusal.

## Behaviour

1. When a protected EPUB with a readable package is imported, then the library
   holds it with its package metadata and protection, and the import report
   counts it as added. Test: the shelf smoke run's locked-books step imports a
   book with Adobe's rights file (`smoke.py` `locked_books`).
2. When every entry of an EPUB is ZIP-encrypted, then `inspect` returns
   protection "a password" and import keeps the book under its file name.
   Test: `a_wholly_encrypted_archive_is_inspected_as_locked` in the reader
   crate, and the same smoke step's "Vault" book.
3. When the file has `META-INF/license.lcpl`, then its protection is Readium
   LCP; with Adobe's `rights.xml`, Adobe; with `META-INF/sinf.xml`, Apple; with
   an encrypted ZIP entry, a password; with only another encrypted resource or
   another `rights.xml`, an unrecognised scheme. A file with two markers takes
   the first in the table's order. Test:
   `the_protection_scheme_is_named_from_the_container`.
4. When `encryption.xml` lists only obfuscated fonts, then the book is not
   protected. Test: `protected_books_are_refused_with_a_clear_reason`.
5. When the cover file of a locked book is not encrypted, then the survey
   returns the cover and no word count; when it is listed in
   `encryption.xml` or in an encrypted ZIP entry, neither. Test:
   `a_protected_book_surveys_its_cover_only_when_it_is_not_encrypted`.
6. When a locked book is shown, then the grid, list, shelf and table names end
   with "Locked by …" and the lock symbol shows in the grid, list and table.
   Test: `a_locked_book_is_said_with_its_lock` for the phrase, and the smoke
   step reading the shelf's, grid's and table row's accessible names.
7. When the reader asks to read a locked book, then the alert names the
   scheme and the explanation, has the one response Close, contains no link,
   and no reader opens. Test:
   `read_on_a_locked_book_says_why_and_offers_only_close`, and the smoke step
   reading the alert's text and buttons.
8. When the catalogue on disk has no `protection` column, then opening adds it,
   its books stay unprotected and its source paths are kept, and a locked book
   imported afterwards shows its protection. Test: library crate test that
   opens a catalogue made by the previous schema
   (`a_catalogue_without_the_protection_column_gains_it_in_place`).
10. When `rights.xml` or `encryption.xml` is malformed or ZIP-encrypted, then
   the book is locked (unrecognised, or a password) and its metadata is still
   read when the package can be. Test:
   `an_unreadable_marker_file_still_names_a_lock`.
9. When the portable record has no `protection` field, then the book is
   unprotected, and one naming an unknown scheme is refused. Test:
   `protection_in_a_record_is_optional_and_checked`.

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

## Open questions

- Should the shelf spine show the lock? Assumed not: a spine has no slot for
  marks, and the spoken name carries it.
