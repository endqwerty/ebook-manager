# Conversion, checking and editing

Turning a book into a form a device can read, finding what is wrong with a file, and
changing a file's contents. These requirements are written from user needs and from
public specifications and the documentation of other tools. See
[Rule 1](../../AGENTS.md#rule-1-never-look-at-calibres-source-code).

## Conversion

- `CONV-1` (core) Convert between reflowable formats, with EPUB as the form in the
  middle. The directions that matter first are EPUB to kepub for Kobo, EPUB to and
  from AZW3 and MOBI, and text, HTML, DOCX and FB2 into EPUB.
- `CONV-2` (core) Conversion never replaces the source. The result is a new file on
  the same book, linked to the file it came from with the settings used.
- `CONV-3` (core) The library's metadata and cover are written into the result.
- `CONV-4` (core) Conversion runs as a queued background job with progress, cancel
  and retry. Each job ends with a report: warnings, what was lost, and the size of
  the result.
- `CONV-5` (core) The app states what a direction loses before it runs: PDF to
  reflowable text loses layout and may scramble order, EPUB to MOBI loses SVG, maths
  and scripts, reflowable to PDF fixes one page size.
- `CONV-6` (expected) The user picks a target profile: generic EPUB, a Kindle, a
  Kobo, a small e-ink screen, a tablet. A profile sets the format, image size limits,
  greyscale or colour, and device-specific fixes.
- `CONV-7` (expected) Settings the user controls:
  - Fonts: keep, embed a chosen font, subset, or remove.
  - Layout: margins, line height, paragraph spacing and indent, justification, and
    whether to keep or strip the book's own styles.
  - Structure: where chapters split, how they are detected, how deep the table of
    contents goes, whether to add a title page.
  - Cover: keep, replace or generate.
  - Images: maximum size, quality, greyscale, conversion of types the target cannot
    show.
  - Text: hyphenation, smart punctuation, find and replace, character set override
    for broken input.
- `CONV-8` (expected) Named presets, a default preset per device and per pair of
  formats, and overrides per book.
- `CONV-9` (expected) Clean-up for badly formatted input: rejoin lines broken at a
  fixed width, rejoin hyphenated words, remove running headers, footers and page
  numbers, and mark scene breaks.
- `CONV-10` (expected) A side-by-side preview of source and result, with a
  comparison of the two tables of contents.
- `CONV-11` (expected) The result is checked before it is added (`CHK-1`).
- `CONV-12` (expected) When a conversion renames or merges the parts of a book, it
  records the map from old to new, so positions and annotations follow. See
  [annotations](05-annotations-history.md).
- `CONV-13` (expected) Convert on the way to a device without keeping the result in
  the library, if the user prefers. See [devices](07-devices.md).
- `CONV-14` (expected) Comic archives convert between CBZ, CBR and CB7 by repacking,
  with no loss. The app can read RAR archives and cannot create them.
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

- Conversion presets, the default preset per device and per format pair, and
  overrides per book.
- Jobs: kind, target, settings, state, progress, start and end times, warnings and
  the error if any, so a job can resume after a restart.
- For each derived file: the source file's hash, the tool and its version, the
  settings, the losses reported, and the map of renamed parts.
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
