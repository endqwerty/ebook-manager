# Conversion, checking and editing

Turning a book into a form a device can read, finding what is wrong with a file, and
changing a file's contents. These requirements are written from user needs and from
public specifications and the documentation of other tools. See
[Rule 1](../../AGENTS.md#rule-1-never-look-at-calibres-source-code).

## Conversion

- `CONV-1` (core) Convert between reflowable formats, with EPUB as the form in the
  middle. The directions needed first are EPUB to kepub for Kobo and EPUB to and
  from AZW3 and MOBI. Text, HTML, DOCX and FB2 into EPUB follow. The table in
  [content types](02-content-types.md) is the full list of directions.
- `CONV-2` (core) Conversion never replaces the source. The result is a new file on
  the same book, linked to the file it came from with the settings used.
- `CONV-3` (core) The library's metadata and cover are written into the result.
- `CONV-4` (core) Conversion runs as a queued background job with progress, cancel
  and retry. Each job ends with a report: warnings, what was lost, and the size of
  the result.
- `CONV-5` (core) The app states what a direction loses before it runs: PDF to
  reflowable text loses layout and may scramble order, EPUB to MOBI loses SVG, maths
  and scripts, reflowable to PDF fixes one page size.
- `CONV-6` (expected) The user chooses the reading device or app and how the
  file will reach it, or chooses an output format when the target is unknown.
  The app chooses a compatible format and handles packaging, text encoding and
  required image changes automatically, explaining its choices and any unknown
  target limits. It keeps supported content and image detail unless a target limit
  or a user choice requires a change, and asks before losing content.
- `CONV-7` (expected) The user can keep the book's appearance or make body text
  follow the reader's font and spacing choices, while keeping meaningful differences
  such as headings, verse and emphasis. Where the output supports both, the user
  chooses between keeping page layout and allowing text to fit the screen, and
  between sharper images and a smaller file. The app explains these trade-offs with
  sample pages before applying them, including any loss of selectable text or colour.
- `CONV-8` (expected) The user can save named conversion choices and set a default
  for a target and delivery method or for a pair of formats. Each job shows which
  choices came from a default and allows changes for that book without changing the
  default. Reusing saved choices checks them against the current target and source,
  and asks again about newly found losses or uncertain repairs.
- `CONV-9` (expected) The app identifies input problems such as broken characters,
  missing headings, a contents list in the wrong order, and page layouts whose
  columns or tables may extract in the wrong order. It offers affected passages for
  review and asks the user to confirm text encoding, heading choices or reading
  order when these cannot be determined reliably. It repairs unambiguous packaging
  faults automatically and reports them, but never silently rewrites uncertain text
  or invents missing structure.
- `CONV-10` (expected) A side-by-side preview of source and result, with a
  comparison of the two tables of contents.
- `CONV-11` (expected) The result is checked before it is added (`CHK-1`).
- `CONV-12` (expected) When a conversion renames or merges the parts of a book, it
  records the map from old to new, so positions and annotations follow. See
  [annotations](05-annotations-history.md).
- `CONV-13` (expected) Convert on the way to a device without keeping the result in
  the library, if the user prefers. See [devices](07-devices.md).
- `CONV-14` (expected) CBR and CB7 comic archives convert to CBZ by repacking, with
  no loss. The app reads RAR archives and cannot create them.
- `CONV-15` (later) OCR for scanned PDFs and page images: a choice of languages,
  skip pages that already have text, deskew, and output as a PDF with a text layer.
  The report shows confidence, and warns that columns may be read in the wrong order
  and that no headings are created.
- `CONV-16` (later) PDF to EPUB by text extraction, labelled as best effort.
- `CONV-17` (later) Merge an audiobook's files into one with chapters, and split
  one into chapters.
- `CONV-18` (later) Web article, RSS feed or newsletter to EPUB, on a schedule,
  skipping items already seen.

## Checking

- `CHK-1` (expected) Check an EPUB against the specification: the container, the
  package, the navigation document, links, identifiers, media types and markup.
  Results have a severity and point to the place in the file.
- `CHK-2` (expected) Check accessibility: missing access metadata, a missing or
  broken page list, a table of contents out of reading order, a missing title or
  language.
- `CHK-3` (expected) Checks for a target: cover size, image size, file size and
  text encoding against what a store or device accepts.
- `CHK-4` (expected) Offer safe automatic repairs, each named and reversible: add a
  missing language, fix the container's first entry, remove files nothing refers to,
  repair a cover that is declared but not linked. A missing cover on the device is
  usually this last fault.
- `CHK-5` (expected) Check results are stored per file and feed the "needs
  attention" list.
- `CHK-6` (later) Run the reference checkers (EPUBCheck, Ace by DAISY) if the user
  has installed them, and show their reports.

## Editing a book's contents

- `EDIT-1` (later) Fix a typo in the text. Find and replace across the book, with
  regular expressions.
- `EDIT-2` (later) Edit the table of contents: rename, reorder, nest, add, remove,
  or rebuild it from headings.
- `EDIT-3` (later) Edit styles with a live preview. List and remove unused styles
  and files.
- `EDIT-4` (later) Split a book in two, or merge several into one, with the
  metadata and identifiers of the results set deliberately.
- `EDIT-5` (later) Embed or subset fonts and report which characters they cover.
- `EDIT-6` (core, applies to every write) An edit is saved to a temporary file,
  checked, and swapped in whole. A failed edit never damages the book.
- `EDIT-7` (core, applies to every write) Each edit keeps the earlier version. The
  user can compare and restore versions, see how much space the history takes and
  prune it.
- `EDIT-8` (core, applies to every write) The parts of a book keep their names and
  identifiers through an edit where possible, so annotations stay attached. Changing
  an EPUB's unique identifier also breaks obfuscated fonts, which are keyed to it.
  The app warns before doing that.

## What the app must store

- Named conversion choices, their defaults per target and delivery method or format
  pair, and separate choices saved for each book.
- The target's known capabilities and limits, their source and version, and any
  limits that remain unknown.
- Jobs: kind, target, settings, state, progress, start and end times, warnings and
  the error if any, so a job can resume after a restart.
- Each derived file records the source file's hash, the tool and its version, the
  target and delivery method, and the choices actually used, including automatic
  choices and which default supplied each saved choice.
- Each conversion records detected input problems, the passages reviewed, confirmed
  repairs, automatic repairs, accepted losses and unresolved warnings, together with
  the map of renamed parts.
- Versions of an edited file with time and a note.
- Check results per file: the checker and its version, the date, and each message
  with severity and place.
- Device profiles (see [devices](07-devices.md)).
- Feed subscriptions: address, schedule, bundling rule, time of last fetch, items
  already seen.

## Not verified

Amazon's Kindle Publishing Guidelines refused automated readers, so the exact needs
of its newer typesetting are unread. Kobo publishes no description of kepub; the
facts here come from the documentation of an independent converter. EPUBCheck's
message list was not found.

## Sources

EPUB 3.3 and its container rules, the Pandoc manual's EPUB sections, the kepubify
documentation, Ace by DAISY's rule list, the OCRmyPDF and Tesseract documentation,
KDP's help pages on formats and covers, the Apple Books Asset Guide, and the Library
of Congress format descriptions.

## First conversion engine

The [MOBI-to-EPUB spec](../../specs/mobi-to-epub.md) covers one restricted direction of CONV-1. For CONV-2 and EDIT-6 it creates a checked, separate file and returns the source identity. Catalogue linking remains open. For CONV-3 it accepts catalogue metadata and a JPEG or PNG cover. CONV-4 has background execution, progress, cancellation and a success report. Durable queueing and history remain open. CONV-5 has a preflight loss description for this direction. The input remains available for EDIT-7. Version comparison, restore and cross-format location mapping for EDIT-8 remain open.
