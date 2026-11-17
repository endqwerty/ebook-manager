ISSUES

Rule 1: no calibre source, docs or plugins were opened. No calibre code was seen on any page. No training recall of calibre arose on this subject. Some primary pages were blocked or empty (see section 4), so several items are [unverified].

## 1. Findings

### Part 1: annotation models and interchange

**W3C Web Annotation Data Model (verified, https://www.w3.org/TR/annotation-model/; first 100k of 138k characters read)**
- An annotation has: id (IRI), type, body (zero or more), target (one or more), motivation (one recommended), and optional creator, created, generator, generated, modified, audience, rights, canonical, via, stylesheet.
- Motivation/purpose vocabulary: assessing, bookmarking, classifying, commenting, describing, editing, highlighting, identifying, linking, moderating, questioning, replying, tagging. So a highlight, a note and a tag are one record type with different motivations. One annotation can have several bodies (note text plus tags) and several targets (a note linked to several passages).
- Body kinds: TextualBody (value, format, language, purpose), an external resource, a Choice (ordered items), or a SpecificResource.
- Selectors, each with optional `refinedBy`:
  - TextQuoteSelector: exact, prefix, suffix.
  - TextPositionSelector: start, end (character offsets after text normalization; end exclusive).
  - DataPositionSelector: byte offsets.
  - FragmentSelector: value, plus conformsTo.
  - CssSelector and XPathSelector: value.
  - SvgSelector: shape relative to the source's size.
  - RangeSelector: startSelector and endSelector, each an independent selector.
- Several selectors may be given on one target as alternatives; the consumer picks one if they disagree.
- State: TimeState (sourceDate, cached copy) and HttpRequestState pin the version of the source the selectors apply to.
- The spec defines no re-anchoring algorithm. Positional selectors break when the text changes. TextQuote carries its own copy of the text, so it survives edits best. The spec recommends pairing positional selectors with a State.
- Need for a highlight to be exportable and re-attachable: the work/file identity, a location in the file, the exact quoted text with prefix and suffix, and the file version it was made against.

**Readium locator (verified, https://readium.org/architecture/models/locators/)**
- Locator: href (required, no fragment), type (media type, required), title, locations, text.
- locations (alternatives): fragments[] (HTML id, `t=67`, `page=12`), progression (0 to 1 inside the resource), position (integer from 1, index in the publication), totalProgression (0 to 1 over the publication), plus extensions.
- text: before, highlight, after.
- Use cases listed: bookmarks, highlights/annotations, search results.
- The page says nothing about how positions are computed or whether they are stable across devices. The HTML extension page (cssSelector, partialCfi) returned 404, so those field definitions are [unverified].
- Implication: store several location forms for one highlight (href, progression, position, CSS/CFI-style path, text quote with context), because one form alone fails after a re-flow or a file edit.

**Readwise API (verified, https://readwise.io/api_deets and https://readwise.io/reader_api)**
- Highlight create fields and limits: text (required, max 8191), title (511), author (1024), image_url (2047), source_url (2047), source_type (3 to 64 chars, no spaces), category (books, articles, tweets, podcasts), note (8191), location (integer), location_type (page, location, none, order, offset, time_offset; default order), highlighted_at (ISO 8601; UTC by default), highlight_url (unique, 4095).
- Deduplication: by title + author + text + source_url, nulls included. The same highlight_url with new text updates the existing highlight.
- Export result, per book: user_book_id, is_deleted, title, author, readable_title, source, category, external_id (only for Reader), highlights[]. Per highlight: id, is_deleted, text, location, location_type, note, color, highlighted_at, updated_at, tags, is_favorite, is_discard, readwise_url. Incremental pull uses `updatedAfter`; deleted items come back with `includeDeleted`.
- Highlight colours are a fixed set: yellow, blue, pink, orange, green, purple. Highlight tags: name, max 127 chars. Book tags: max 512 chars. Book overrides: title_override, author_override, cover_image.
- Daily Review: a set of highlights per day plus review_completed. Discard (is_discard) and favourite are per-highlight flags.
- Rate limits: 240 per minute, and 20 per minute for highlight and book list.
- Reader API: highlights are documents with parent_id, content, notes, highlight_location and highlight_offset. A document has reading_progress (0 to 1), first_opened_at, last_opened_at, saved_at, category, location (new, later, shortlist, archive, feed), tags and notes. Highlight content must match the parent text or the request returns 400.
- Readwise CSV export is a single file; its columns are not documented on the page I read [unverified]. Markdown export uses Jinja2 templates (page title, page metadata, highlights header, sync notification). Source: https://docs.readwise.io/readwise/docs/exporting-highlights/markdown-csv
- Import routes (verified): Kindle (browser extension or `My Clippings.txt` upload), Apple Books (Mac tool, or share from phone), email import, physical books by OCR/scan, Hypothesis (direct sync), BookFusion (manual, via token), Snipd. Kobo, Play Books, Libby, KOReader, Moon+ Reader and the bulk CSV column list were not on the pages I could open [unverified]. Search snippet says Play Books via Google Docs and Moon+ Reader via an integration (unverified).
- KOReader pushes straight to Readwise (see below).

**Zotero (partly verified)**
- Verified (https://www.zotero.org/support/pdf_reader): highlight and underline, each with a colour picker, in unlocked mode (choose the colour after selecting) or locked mode (tool plus colour). Annotations made in the built-in reader live in Zotero's database and do not appear in external readers until you export a PDF with embedded annotations. Editing pages (add, delete, rotate) in an external reader can misplace annotations. Annotations can be turned into an item note or standalone note; a citation is added by default and can be hidden. Annotation colours can be shown or hidden in notes.
- Verified (https://www.zotero.org/support/kb/annotations_in_database): stored in an open SQLite database, readable by other tools and through the web API. File > Export PDF (library) or Save As (reader) writes a PDF copy with annotations embedded.
- Verified (https://www.zotero.org/support/note_templates): three templates (highlight, note, note title), edited through config key `annotations.noteTemplates`. Variables: highlight (with `quotes`), citation, comment, color (e.g. #ffd400 yellow, #ff6666 red), tags (with `join`). Conditionals `{{if}}`, `{{elseif}}`, `{{else}}`, `{{endif}}` compare colour values. So colour-to-meaning mapping happens in templates.
- From search results only [unverified as primary]: types are highlight, underline, note (sticky), text, image/area, ink; underline works in PDF, EPUB and snapshots, while ink and text are PDF-only; snapshots allow highlight, underline and sticky notes.
- Field-level schema (type, text, comment, colour, pageLabel, sortIndex, position, authorName, tags) was not readable on the pages opened. [unverified]. https://www.zotero.org/support/dev/web_api/v3/types_and_fields points to `GET /itemTypeFields?itemType=annotation` and the schema repository.

**Hypothesis (mostly unverified; docs page returned only a table of contents: https://h.readthedocs.io/en/latest/api-reference/v1/)**
- Verified through search results: the client supports TextQuoteSelector, RangeSelector (container paths and offsets) and TextPositionSelector, while the backend stores arbitrary selectors. The annotation object fields listed in the contents: id, created, updated, user, uri, text, tags, group, target, document, permissions, hidden, flagged, links. Groups, `__world__` public group and permission structure: [unverified]. Readwise syncs Hypothesis directly (verified above).

**PDF annotations (search summaries of ISO 32000-2, not the standard itself)**
- Subtypes listed from the pdf-annot crate docs, which follow ISO 32000-2 Table 170: Text, Link, FreeText, Line, Square, Circle, Polygon, PolyLine, Highlight, Underline, Squiggly, StrikeOut, Stamp, Caret, Ink, Popup, FileAttachment, Sound, Widget, Watermark, Redact, Movie.
- Highlight, Underline, Squiggly, StrikeOut are text markups defined by QuadPoints (one quadrilateral per line of marked text). Ink uses an InkList of strokes. Line uses endpoints, Polygon uses Vertices, Square and Circle use a rectangle. FreeText has a default appearance (font, colour). Stamp is a stock name or image.
- Rendering depends on an appearance stream, which one source says must exist for viewers to draw the annotation. Details [unverified].
- Writing into the PDF changes the file's bytes. KOReader documents the consequence: its hash-based metadata id changes when PDF highlights are written, which orphans stored metadata (verified, https://koreader.rocks/user_guide/). Zotero keeps annotations beside the file and writes them only on export. Both choices are valid; a manager needs both a sidecar store and an "embed on export" action.

**Handwriting and ink**
- InkML (verified, https://www.w3.org/TR/InkML/): `ink` root; `trace` is a list of sample points; `traceFormat` declares the channels per point. Channels: X, Y, Z, F (force), S (tip switch), T (time), OTx/OTy (tilt), OA/OE/OR (azimuth, elevation, rotation), colour, transparency, width, buttons. Values can be absolute, differences or second differences. Trace type: penDown, penUp, indeterminate. `traceGroup` nests; `traceView` selects traces without copying; `brush` and `context` carry style and canvas; `annotation` attaches semantic labels (recognized text is not stored by InkML itself).
- Apple PencilKit (page was JS-only; the description came from the fetch tool's own knowledge, so [unverified]): a drawing is a list of strokes; a stroke has an ink (type and colour), a path of points (location, time offset, size, opacity, force, azimuth, altitude) and a transform. Ink types pen, pencil, marker, plus monoline, fountain pen, watercolor, crayon. Eraser has bitmap and vector modes; lasso selects strokes. Drawings serialize to an opaque data blob and render to an image.
- Android Ink API (verified at overview level, https://developer.android.com/develop/ui/compose/touch-input/stylus-input/about-ink-api): based on Google's Ink library; supports low-latency drawing, stroke erase, save ink data, and server-side rendering so exports to PDF or PNG look identical.
- What users expect (unverified, general): pen colours and widths, highlighter, stroke eraser vs pixel eraser, lasso move/resize/delete, undo, palm rejection, convert handwriting to text, export notebook to PDF with strokes in place, ink drawn on top of book pages.

**Export targets and annotation features beyond basics**
- KOReader (verified): exports highlights and notes as text, HTML, Markdown, Kindle clippings and JSON; can restrict to styles (lighten, underline, strikeout, invert); pushes to Joplin, Readwise, Memos, Flomo, XMNote; export for current book, whole library or chosen files; custom file names; imports through KoHighlights and a Calibre sync plugin. Highlight colours only on colour-screen devices.
- Kindle (search results only, conflicting): email export of notes as PDF and CSV; publisher caps on exportable highlights (figures like 10 to 20 percent are quoted by vendor pages, unreliable); `My Clippings.txt` entries separated by `==========` lines.
- Obsidian/Logseq (Markdown with front matter), Notion, Anki cards, BibTeX-linked notes, copy with citation, quote image: none of these had a primary page read. [unverified]. Readwise's Jinja2 templates and Zotero's note templates are verified examples of user-editable templates with variables; copy the idea.
- Readwise verified features worth matching: favourite flag, discard flag, per-highlight tags, per-book tags, notes on a highlight, colours, daily spaced review with completion state, book-level cover/title/author overrides, update by stable `highlight_url`.

### Part 2: reading history and trackers

**Goodreads export (verified from a librarian forum post, https://www.goodreads.com/topic/show/22566592-what-s-in-an-export-file?page=1; not vendor docs; the official help article 404ed)**
Columns: Book Id, Title, Author, Author l-f, Additional Authors, ISBN, ISBN13, My Rating, Average Rating, Publisher, Binding, Number of Pages, Year Published, Original Publication Year, Date Read, Date Added, Bookshelves, Bookshelves with positions, Exclusive Shelf, My Review, Spoiler, Private Notes, Read Count, Owned Copies. Legacy and removed in 2022: Recommended For, Recommended By, Original Purchase Date, Original Purchase Location, Condition, Condition Description, BCID.
- Exclusive Shelf values: read, currently-reading, to-read (custom exclusive shelves can appear). Shelf order in "with positions" is reported as inconsistent. No start date, no progress, no per-re-read dates (only Read Count and one Date Read). Hardcover users confirm: no start dates in Goodreads exports (search snippet).

**StoryGraph (verified import template, https://thestorygraph.freshdesk.com/support/solutions/articles/79000146300-bulk-import-template)**
- Import template uses the Goodreads shape. Required: Title, Author, My Rating (0 means unrated; zero ratings are not stored), Date Added (yyyy/mm/dd), Exclusive Shelf (read, currently-reading, to-read, did-not-finish, or custom), Read Count (0 for unread or did-not-finish). Optional: Date read, My Review, Owned Copies (1), ISBN13 (needed to pick an edition), Bookshelves (tags; hyphenate words; comma-separated).
- Custom exclusive shelves: put the name in both columns; leave the exclusive option blank during tag mapping so it becomes a tag.
- Export: columns not documented in what I could read. Roadmap requests (verified: https://roadmap.thestorygraph.com/requests-ideas/posts/additional-data-in-export) say the export lacks year published, original publication year, page or minute counts, genres, series, publisher. A search snippet says mood and pace are exported [unverified]. Reading-journal entries (pages, duration, dates) appear not exported (request dated Nov 2025, unverified).

**BookWyrm (verified, https://docs.joinbookwyrm.com/user-migration.html; import list from search snippet)**
- Imports CSV/TSV from Calibre, Goodreads, LibraryThing (TSV), OpenLibrary, StoryGraph and OpenReads.
- Account export (tar.gz) contains profile and avatar, most settings, reading goals, shelves, reading history, reviews, statuses, own and saved lists, follows and blocks. Excludes direct messages, replies to your statuses, groups and favourites. Import lets you untick data types; some data is overwritten. Moving accounts needs an ActivityPub alias or move.
- BookWyrm statuses are review, comment, quotation, plus generated notes [unverified]. The dedicated import/export doc URL 404ed.

**Hardcover (mostly unverified; docs.hardcover.app returned 403 twice)**
- GraphQL API, about 60 requests per minute, 30 s timeout, max query depth 3 (third-party guides). status_id: 1 want to read, 2 currently reading, 3 read, 4 paused, 5 did not finish. Reads carry progress_pages and progress_seconds, started_at, finished_at, edition_id, reading_format_id. Privacy setting exists per book entry. All unverified from third-party sources.
- Imports Goodreads and StoryGraph CSV plus a custom CSV with a sample template; export is JSON (account export page); users ask for CSV to match import. The team says no official plans for e-reader integrations in 2026 (roadmap, search snippet). Third-party Goodreads-to-Hardcover and Audiobookshelf-to-Hardcover sync tools exist.

**Open Library (verified, weak)** https://openlibrary.org/help/faq/reading-log: three shelves (currently reading, already read, want to read) plus themed lists; "use this work" adds a work not an edition. Export columns, check-ins, ratings and goals: not readable in pages I opened. [unverified]. The Lists API page (last edited 2023-10-15) says list update is "not implemented".

**LibraryThing (search snippets only)** 2014 blog says the tab-delimited and JSON exports have 41 fields. JSON keys include datestarted and dateread (YYYY-MM-DD; sometimes one day off versus the catalog), collections. MARC mapping holds tags, review, rating, collections, private comments. The wiki page returned 403. Half-star ratings: [unverified].

**Literal, Bookly, Bookmory**
- Literal page 403. Bookmory (App Store listings, not vendor docs): progress logged as pages, percent, episodes or time; reading timer; notes and quotes; rating and review; daily, monthly and annual statistics; monthly reading calendar; iCloud backup; export only as xlsx per a Hardcover forum user. Bookly: not researched (no source opened).

**KOReader (verified, https://koreader.rocks/user_guide/)**
- Progress sync: partial-MD5 document id (any file change breaks the match) or file-name matching (suits managed libraries); server stores username, document id, furthest position; self-hostable.
- Metadata storage: `.sdr` folder beside the book, a central docsettings folder, or MD5-keyed hashdocsettings. Metadata archive restores settings, highlights and progress when a book is re-copied. Reading Statistics plugin gives progress, time-range, calendar views; Book Map shows time per page. Full statistics fields and any Hardcover/Goodreads integration: not on the part I read (a plugin for Hardcover may exist in the community; unverified).

**What each product models (consolidated, evidence as above)**
- Status: Goodreads 3 exclusive shelves plus custom; StoryGraph adds did-not-finish; Hardcover has 5 including paused and DNF. DNF reason: not found in any source. 
- Re-reads: Goodreads only a count and last date; StoryGraph import creates entries from count; Hardcover has multiple reads with start/finish dates.
- Tags vs shelves: Goodreads shelves can be exclusive or not; StoryGraph turns non-exclusive shelves into tags.
- Reviews: spoiler flag (Goodreads Spoiler column), private notes (Goodreads Private Notes).
- Ownership: Owned Copies (count), purchase date, location, condition (legacy).
- Format read: Hardcover reading_format_id; Binding column in Goodreads is the edition format, not the format read.
- Mood, pace, content warnings: StoryGraph concepts (export coverage unverified).
- Goals: BookWyrm exports reading goals. Stats users value (Bookmory, KOReader verified items): daily/monthly/annual stats, monthly calendar, per-page time. Streaks, heat-map, reading speed per book, format split: [unverified for any product].

## 2. Data the app must store

1. **Work, Edition, File**, with identifiers (ISBN10/13, publisher ids, file hash, tracker ids: Goodreads Book Id, Hardcover id, Open Library work/edition id, LibraryThing id, Readwise user_book_id, KOReader document id). Without a mapping table there is no lossless round trip.
2. **FileVersion**: content hash plus a record of every edit that changes it (writing PDF highlights, metadata edits), so annotations bound to a hash can be re-bound.
3. **Annotation** (one record type): id (stable, never reused), work/edition/file refs, kind (highlight, underline, strikeout, squiggly, note, bookmark, text, area image, ink, free text, shape), motivation set, created, modified, deleted-at (tombstone for sync), author, visibility (private, group, public), origin (which app/file/service), external ids (Readwise highlight id and `highlight_url`, Hypothesis id, Zotero key).
4. **AnnotationAnchor** (several per annotation, kept together): ordered list of locators, each with: kind (text quote, text position, CFI-like path, CSS/XPath, PDF page + QuadPoints, rectangle, byte range, time offset for audio), href/spine item, progression, position, totalProgression, page label, fragments, quoted text with before/after context, file version it was made against. Multiple targets per annotation to link one note to many passages.
5. **Body parts**: quote text, comment text and format (plain, Markdown, HTML), language, attached images, tags (as references), colour.
6. **ColourScheme**: per library, a mapping from colour value to a user-named meaning, plus a "colour as raw hex" fallback (Readwise has 6 named colours, Zotero uses hex).
7. **AnnotationFlags**: favourite, discarded, review state (last reviewed, next due, interval, history), exported-to (destination, time, remote id).
8. **InkStroke / InkPage**: page ref, tool (pen, pencil, marker, highlighter), colour, width, points (x, y, time, pressure, tilt, azimuth), stroke grouping, layer, transform; recognized-text overlay stored separately with confidence; rendered preview image.
9. **Notebook**: standalone pages not tied to a book, with background (blank, lined, PDF), and optional links to works.
10. **ExportTemplate**: name, target (Markdown, HTML, CSV, JSON, text, Anki, BibTeX note, Obsidian, Logseq, Notion), template text with variables (highlight, comment, colour, tags, citation, page label), conditionals, front-matter fields, file naming, one-file-per-book vs one-per-annotation, sync state (last export time, remote ids).
11. **Citation data** per work: CSL/BibTeX-style fields, citation key, page label per annotation.
12. **ReadingEntry** (not just a status): work/edition, status (want, reading, read, paused, DNF, custom exclusive), DNF reason and stopped-at position, started_at, finished_at (each re-read is its own entry), format read (print, ebook, audio, plus device/app), rating (scale 0 to 5 with half steps, unrated distinct from 0), review text with spoiler flag and visibility, private notes, owned flag/copies/purchase info/condition, shelves, moods, pace, content warnings, source of entry (imported from which file and row).
13. **ProgressEvent**: timestamp, unit (page, percent, seconds, location), value, session start/end, device, source. Needed for stats, streaks and heat-maps.
14. **ReadingGoal** (year, target in books/pages/minutes, count rule) and **Challenge**.
15. **Shelf/List/Collection** with exclusivity flag, ordering position (Goodreads exports positions), description, visibility.
16. **ImportRecord**: source service, original row or JSON, import date, mapping decisions, so lossy imports can be redone later (Goodreads has no start dates, StoryGraph no genre).
17. **TrackerAccount / SyncState**: service, token reference, last cursor (Readwise `updatedAfter`/`pageCursor`), rate-limit state, id maps.

Fields with no home in typical designs (the first-round list would miss them):
- Per-re-read dates and a separate entry for each read-through.
- DNF reason and DNF position; paused status.
- Spoiler flag, private notes separate from review, visibility per entry.
- Several anchors per annotation, plus the file version they belong to.
- Colour meaning mapping; hex vs named colours.
- Discard flag and review schedule on highlights.
- Annotation tombstones with deleted timestamps (Readwise `is_deleted`).
- Format actually read vs edition format; read device.
- Mood, pace, content warnings as user data and as imported data.
- Tracker external ids per edition and per work (and work vs edition choice: "use this work").
- Purchase date, location, condition, owned copy count.
- Original row of each import.

## 3. Requirements a new manager should adopt

- Keep annotations outside the book file by default, and make embedding into PDF an export action. Reason: Zotero shows the safe pattern; KOReader shows that rewriting the file breaks id-based metadata.
- Bind an annotation to several anchors (text quote with context, position, path) and re-anchor by text quote when the file changes; record which file version each anchor used. The W3C spec leaves re-anchoring undefined, so that logic is the manager's own.
- Make the document id survive metadata edits: do not key sync on a hash of the whole file if the manager edits that file (KOReader's own docs warn about this).
- Round-trip exports: every export carries a stable id so a later re-export updates an existing note instead of making duplicates (Readwise deduplicates by title+author+text+source_url, which merges legitimate duplicates and splits edited quotes).
- Let users define colour meanings and edit export templates with conditionals (Zotero, Readwise).
- Track every read-through with its own dates, DNF reason, format and device; import Goodreads and StoryGraph into that shape and keep the original row.
- Offer JSON and CSV exports of the whole history, not just highlights, with a documented schema. Hardcover offers only JSON and users ask for CSV; StoryGraph's export omits genre, series, pages and journal entries.
- Document everything that does not survive an import (a loss report at import time).
- Keep annotation limits out of the data (Kindle's publisher caps on export are a complaint source; the manager owns the user's own file annotations without caps).

## 4. Open questions

- Readwise CSV import column names, and Kobo/Libby/Play Books/Moon+ Reader import details (docs pages I could open did not list them).
- Hypothesis groups, permissions and tag rules.
- Zotero annotation schema fields and position format for EPUB and snapshots.
- Readium HTML locator extension fields (404).
- Hardcover API field definitions, privacy values and import columns (docs 403).
- StoryGraph, Open Library and LibraryThing exact export columns; Literal, Bookly details; BookWyrm CSV export columns and quotation/comment status schema.
- Anki, Obsidian, Logseq, Notion export shapes; PDF annotation spec text (ISO 32000-2 is paywalled; only summaries read).
- Whether any product stores DNF reasons.
- Statistics: streak, heat-map, reading speed, format split definitions; only generic tracker features were seen.

## 5. Source URLs opened

Opened and read:
- https://www.w3.org/TR/annotation-model/
- https://readium.org/architecture/models/locators/
- https://readwise.io/api_deets
- https://readwise.io/reader_api
- https://docs.readwise.io/readwise/docs/importing-highlights
- https://docs.readwise.io/readwise/docs/importing-highlights/other-sources
- https://docs.readwise.io/readwise/docs/exporting-highlights/markdown-csv
- https://www.zotero.org/support/pdf_reader
- https://www.zotero.org/support/note_templates
- https://www.zotero.org/support/kb/annotations_in_database
- https://www.zotero.org/support/dev/web_api/v3/types_and_fields
- https://h.readthedocs.io/en/latest/api-reference/v1/ (table of contents only)
- https://www.w3.org/TR/InkML/
- https://developer.apple.com/documentation/pencilkit/pkstroke (JS shell; no content)
- https://developer.android.com/develop/ui/compose/touch-input/stylus-input/about-ink-api
- https://www.goodreads.com/topic/show/22566592-what-s-in-an-export-file?page=1
- https://thestorygraph.freshdesk.com/support/solutions/articles/79000146300-bulk-import-template
- https://roadmap.thestorygraph.com/requests-ideas/posts/additional-data-in-export
- https://docs.joinbookwyrm.com/user-migration.html
- https://openlibrary.org/dev/docs/api/lists
- https://openlibrary.org/help/faq/reading-log
- https://koreader.rocks/user_guide/ (two parts)
- https://roadmap.hardcover.app/feature-requests/posts/files-transfer-option-from-bookmory

Blocked or empty (one try each, not retried): readium.org HTML extension (404), pdfa.org PDF annotations (403), docs.hardcover.app (403, two pages), help.goodreads.com (404), docs.joinbookwyrm.com/import-export.html (404), openlibrary.org reading_log dev page (404), wiki.librarything.com (403), literal.club (403), amazon.com help (503), help.readwise.io article 66 (301 redirect to a page already read).

Searches used (summaries only, counted as unverified unless stated): Readwise import sources, Zotero 7 annotation types, Hypothesis API, PDF annotation types, Goodreads columns, StoryGraph export, Hardcover API, BookWyrm import, Open Library reading log, LibraryThing export, Bookmory, Kindle export.
