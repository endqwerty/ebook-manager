# Positions, annotations and reading history

Where the reader is in a book, what they marked, and what they have read. In user
discussions the two loudest complaints about every existing product are a reading
position that differs on each device and highlights stuck on the device that made
them. This area exists to fix both.

## Positions

- `POS-1` (core) A position is stored in several forms at once: the file and the
  part of it (spine item, page, audio file), the fraction through that part, the
  fraction through the whole book, a structural path such as an EPUB CFI, and the
  text just before and after. No single form survives every change, so none is
  stored alone.
- `POS-2` (core) A position records which file version it was made against, by
  hash.
- `POS-3` (core) Each device keeps its own last position. The app also keeps the
  furthest position and the most recent one, each with its device and time. Opening
  a book on another device offers the newer place and never jumps without asking.
  Kavita's own documentation warns that automatic sync can undo progress.
- `POS-4` (core) Position history is kept, so a mistaken sync or a slip of the
  finger can be undone.
- `POS-5` (core) When a file is replaced, converted or corrected, positions move to
  the new file. The app tries in this order: same file hash; same content hash for
  that part; structural path whose text checks still match; search for the
  surrounding text in that part, then in the whole book; the fraction, marked as
  approximate.
- `POS-6` (expected) Text and audio positions of one work are separate records.
  The alignment map turns one into the other.
- `POS-7` (expected) A position imported from a device or another service keeps
  its original unit (a Kindle location, a Kobo value, a percentage, a page) and how
  exact the conversion was.

## Annotations

- `ANN-1` (core) Highlights in several colours, underline, notes on a highlight,
  notes on a page, chapter or whole book, and bookmarks. Bookmarks are made
  deliberately and can be undone.
- `ANN-2` (core) An annotation is its own record and is never written into the book
  file by default. Writing annotations into a PDF is an export that produces a copy.
  KOReader's documentation shows why: writing into the file changes its hash and
  orphans everything matched by hash.
- `ANN-3` (core) An annotation carries several anchors, in the same forms as a
  position, plus the quoted text. It moves to a replaced file by the steps in
  `POS-5`. One that cannot be placed is kept as an orphan with its quote and never
  deleted.
- `ANN-4` (core) One screen lists every annotation from every book, device and
  source. It can be searched and filtered by book, colour, tag, date and source.
- `ANN-5` (core) Export annotations for one book or all of them as Markdown, plain
  text, HTML, CSV and JSON. Nothing caps how much of the user's own book they can
  export. Kindle truncates exports beyond a publisher's limit of about 10 to 20
  percent of a book.
- `ANN-6` (expected) Export uses templates the user can edit: variables for the
  quote, note, colour, tags, chapter, page and citation, conditions on colour, front
  matter, and one file per book or per note.
- `ANN-7` (expected) Every exported annotation carries a stable identifier. A
  second export updates the earlier note in place and makes no duplicate. A file the
  user has edited outside the app is never overwritten. Google Play Books overwrites
  such edits.
- `ANN-8` (expected) The user names what each colour means. A colour is stored as a
  value plus an optional name, because other tools use either fixed names or free
  hex values.
- `ANN-9` (expected) Tags on annotations, a favourite flag, and a link from one
  note to several passages.
- `ANN-10` (expected) Highlights that touch or overlap stay separate records. A
  Kindle update merged neighbours into one colour.
- `ANN-11` (expected) Import annotations from devices and services: Kindle's
  clippings file, Kobo's device data, KOReader's sidecar folders, annotations already
  inside a PDF, and Readwise or Apple Books exports. Each import can be repeated
  without making duplicates, records its source, and keeps a copy of the raw input so
  it can be parsed again after a bug fix. See [devices](07-devices.md).
- `ANN-12` (core) Deleting an annotation leaves a marker, so the deletion
  reaches other devices and the annotation does not come back on the next sync.
- `ANN-13` (expected) PDF annotations: highlight, underline, strike-out, squiggly,
  text note, free text, shapes and ink, placed by page and rectangle.
- `ANN-14` (later) Handwriting with a stylus on pages and in notebooks that belong
  to no book. Strokes keep points with time and pressure, and are anchored to text so
  a change of font does not displace them. Recognised text is stored apart from the
  strokes.
- `ANN-15` (later) Review highlights by spaced repetition. Share a quote as an
  image. Copy a quote with a citation.
- `ANN-16` (later) Push annotations to Readwise, Obsidian, Logseq, Notion, Joplin
  or Anki, each with its own record of what was sent.
- `ANN-17` (later) Export as W3C Web Annotation, the open interchange form.

## Reading history

- `HIST-1` (core) A book has a status: want to read, reading, paused, finished, did
  not finish, or none. "Did not finish" can record where the reader stopped and why.
  The user can add statuses of their own, because imports from trackers carry them.
- `HIST-2` (core) Every read-through is its own record with a start date, a finish
  date, the edition and format read, and the device or app. Reading a book twice
  makes two records. Dates can be edited, and a book read on paper or in another app
  can be logged by hand. Apple Books has no editable reading dates, and a Goodreads
  export carries one finish date however often a book was read.
- `HIST-3` (core) A rating on a five-point scale, stored finely enough for half
  and quarter steps because trackers differ, with "unrated" distinct from zero. A review with a spoiler flag. Private notes separate from the
  review.
- `HIST-4` (core) Reading sessions are logged: book, device, start, end, active
  time, and where the reader started and stopped. Statistics, streaks, pace and
  time-left estimates all come from sessions. Every file counts the same, whatever
  its format or origin. Kobo is reported to leave sideloaded books out of parts of
  its statistics.
- `HIST-5` (expected) Statistics: time and pages per day, a calendar, streaks,
  books and pages per year, breakdown by author, genre and format, time to finish,
  speed per book, and a yearly summary.
- `HIST-6` (expected) Goals: minutes per day, books, pages or hours per year. The
  app states what counts toward each.
- `HIST-7` (expected) The user can edit or delete any session, and clear their
  history.
- `HIST-8` (expected) Import and export reading history as Goodreads and StoryGraph
  CSV and as the app's own documented JSON. Every import keeps the original row, and
  reports what could not be mapped. Goodreads exports carry no start dates and only
  one finish date. StoryGraph's export omits genre, series and page counts.
- `HIST-9` (later) Keep an account at Hardcover, StoryGraph, BookWyrm, AniList or
  similar up to date. The app stores the service's identifier for each book, the
  last state sent and a queue for changes made offline. Kobo's own StoryGraph link
  sends only a percentage and reads nothing back.
- `HIST-10` (later) Moods, pace and content warnings, as the user's own data and as
  imported data.
- `HIST-11` (later) Ownership: copies owned, where a paper copy is, who borrowed
  it, when and where it was bought.

## What the app must store

- Position records per user, book, file and device, each with the forms in
  `POS-1`, the file hash, a time and a source. Position history.
- Hash aliases linking each replaced file to its successor, and a map from old to
  new part names when a conversion renames them.
- Annotation: stable identifier, kind, colour value and name, quote, whether the
  quote was cut short by a copy or export limit, note text and format, tags, flags,
  created and modified times, deleted time, author, visibility (private, shared
  with a group, public), origin (app, device, service, embedded in the file),
  identifiers in outside services.
- Annotation anchors: several per annotation, each with its form, the file version
  it belongs to and whether it currently resolves.
- Ink strokes, their page or text anchor, and recognised text. Notebooks.
- Export templates and, per destination, what was exported, when, and the remote
  identifier.
- Raw import snapshots with their hash and parser version.
- Reading status per user and book. Read-through records. Sessions.
- Ratings, reviews with spoiler flag and visibility, private notes.
- Goals with their history. Review schedule for highlights.
- Tracker accounts, per-book remote identifiers, the pending queue, and how far
  the app has read each service's list of changes.
- The original row of every imported record.

## Not verified

The layout and quirks of Kindle's clippings file, the contents of Kindle's per-book
sidecar folders, the columns of Kobo's device database, and the export columns of
StoryGraph, Hardcover and LibraryThing were not read from a primary source. They need
tests on real devices and real export files.

## Sources

The W3C Web Annotation Data Model, the EPUB CFI specification, the Readium locator
model, the Readwise API documentation, Zotero's support pages on annotations and note
templates, InkML, the KOReader user guide, a Goodreads librarian's list of export
columns, StoryGraph's import template, BookWyrm's migration page, and the round-two
reports on user discussions, Kindle, Kobo and Google Play Books.
