# Library

How a user stores, organises, finds and protects their books. This area sets the
shape of the core records, so read it with
[data-model-inputs.md](data-model-inputs.md).

Priorities: **core** is needed for a first usable release, **expected** is what users
of comparable tools take for granted, **later** means store the data now and build the
feature afterwards.

The [local library](../../specs/library-mvp.md) implements title and author search,
managed EPUB copies, duplicate detection, replacement review and recovery from
a portable backup. These cover parts of LIB-1, LIB-22, LIB-30 through LIB-32,
LIB-40 and LIB-42. Source EPUBs remain unchanged (LIB-43).

## Organising

- `LIB-1` (core) A book has a title, sort title, authors and other contributors with
  roles, series, tags, publisher, languages, identifiers, dates, a description, a
  rating and a cover. [Metadata](03-metadata.md) lists every field.
- `LIB-2` (core) One book holds several files: the same text as EPUB and PDF, a
  converted copy, an audiobook. Deleting one file leaves the book.
- `LIB-3` (core) A book belongs to any number of series, each with a position. A
  position is text with a sort key, so `1.5`, `2.2.1`, `1-5` and `TPB1` all sort
  correctly. No tool surveyed sorts non-numeric positions well. The catalogue sorts text positions this way, import reads the series a file declares, and the [Linux Edit window](../../specs/desktop-mvp.md#edit) sets a book's first series and its position. Editing a second series is open.
- `LIB-4` (core) The user puts books in collections by hand. A book can be in many
  collections. A collection has a name, a description, a manual order and its own
  view and sort settings.
- `LIB-5` (expected) The user saves a search as a smart collection. It updates as
  books change and can be pinned to the home screen or sidebar.
- `LIB-6` (expected) Tags nest (`Fiction > Fantasy`). Tags are the user's own words
  and stay separate from subject headings that come from a source.
- `LIB-7` (expected) The user defines custom fields: text, long text, number, date,
  yes/no, rating, one choice from a list, several choices, and a link to another
  book. Custom fields work in search, sort, bulk edit, device rules and export.
  Missing custom fields is a reason people abandon a new manager.
- `LIB-8` (expected) Reading lists are ordered and cross series and formats: a story
  arc, a recommended reading order, an imported `.cbl` list. An imported entry that
  matches no book stays in the list, and the user's fix is remembered as a rule.
- `LIB-9` (expected) The app tells editions of one work apart: a translation, an
  abridged audiobook, a second narrator, a revised edition. It also models containers:
  an omnibus or box set contains works, and a single file can hold several books.
  No server surveyed has an edition record, and users ask for it.
- `LIB-10` (expected) A book can exist with no file: a purchase locked in a store
  app, a paper copy, a loan that ended, a wish-list entry. The record says where the
  copy is. A file added later merges into it.
- `LIB-11` (expected) People are records, not strings. One author has a display
  name, a sort name, names in other scripts, pen names and authority identifiers.
  `J.R.R. Tolkien` and `J. R. R. Tolkien` are one person.
- `LIB-12` (later) Items that are not books have their own fields: a periodical with
  issues, a comic series with volumes and issues, a paper with a DOI, a saved web
  article. See [content types](02-content-types.md).

## Finding and viewing

- `LIB-20` (core) Cover grid, list and table views. The table has user-chosen
  columns. Each view remembers its sort. Done on Linux: the [grid](../../specs/desktop-mvp.md#the-library-window), the [list and the table](../../specs/desktop-mvp.md#list-and-table) with chosen columns, and a remembered sort for each.
- `LIB-21` (core) Sort by any field, with a second and third key. Sorting follows
  the locale: leading articles are skipped by a per-language rule, numbers sort as
  numbers, and the user can edit any sort key by hand. On Linux the [Sort menu, its editor and the table's headers](../../specs/desktop-mvp.md#sorting) sort by seven fields with up to three keys. The Edit window lets the reader change sort title, author sort names, series and series position. Date added, date published, last read and progress cannot yet be edited by hand as sort keys.
- `LIB-22` (core) Typing in the search box filters at once across title, author,
  series and tags. The Linux window [filters as the reader types](../../specs/desktop-mvp.md#search) across title, author and series. Tags wait until the catalogue holds them.
- `LIB-23` (expected) A query language with `field:value`, AND, OR, NOT, ranges,
  "is empty" and comparison of dates and numbers. Custom fields and reading state
  are searchable.
- `LIB-24` (expected) Facets with counts: author, series, tag, language, format,
  rating, reading status, source, device.
- `LIB-25` (expected) Full-text search inside books, with a snippet and a jump to
  the hit. The index is a cache the app can rebuild. Search shows how much of the
  library is indexed, covers only books whose text the device holds, and never
  shows a count or snippet from a book or note the user may not see.
- `LIB-26` (expected) A home screen with Continue reading, Recently added and the
  user's pinned collections.
- `LIB-29` (expected) A shelf view that shows books by their spines, standing in
  rows. Every spine is one height, its thickness follows the book's length, its
  colour comes from the cover, and a selected book tips out from the top. It is
  drawn as [the shelf design](../design/interface-views.md#shelf) says and stays
  smooth at 100,000 books. The Linux shelf draws the spines and groups,
  keeps the first full book’s row at the top when narrowing or widening as far as focus visibility allows,
  and shares the library selection and commands, including Select All.
  At 100,000 books the desktop driver checks the on-screen widget count,
  scrolls, resizes and extends a selection. The frame-time target remains open.
- `LIB-27` (core) Bulk edit with a preview of every change and undo.
- `LIB-28` (core) Every action is reachable from the keyboard. This is a common
  complaint about existing tools. Done for the Linux library window, including column widths. Every command [has a key](../../specs/desktop-mvp.md#commands-and-keys), and the smoke check exercises keyboard commands. The desktop checks also resize the window and scroll with the wheel. The import report's buttons are reached with Tab.

## Importing

- `LIB-30` (core) Add books by drag and drop, file picker or folder. Import reads
  the metadata inside each file first, then sidecar files, then the file and folder
  names.
- `LIB-31` (core) Import runs in the background, can be paused, and never blocks the
  window. Each file gets a result: added, added as another format of an existing
  book, skipped as a duplicate, or failed with the reason. The whole import can be
  undone.
- `LIB-32` (core) Duplicate handling on import: skip, add as a format, replace, or
  ask. Detection uses the file hash, then identifiers, then title and author.
- `LIB-33` (expected) Watched folders. New files wait in a review queue with the
  proposed metadata until the user accepts them, or go straight in if the user chose
  that. A file still being copied is left alone until it stops changing.
- `LIB-34` (expected) Index a folder tree in place without moving or renaming
  anything.
- `LIB-35` (core) Import an existing calibre library by reading the files it
  writes (its database and folder layout), without changing or locking that library.
  Custom fields, ratings, tags, series and covers carry over. Import from another
  manager's export follows the same rule. This is the first thing people check.
- `LIB-36` (expected) A dry run for organising a messy folder: the app shows which
  files it would move where, and changes nothing until confirmed.
- `LIB-37` (expected) A "needs attention" list holds books with failed parsing,
  missing metadata, low-confidence matches or validation errors.
- `LIB-38` (later) Find duplicates across the library and merge them. The merge
  keeps every format, collection, tag, annotation and reading record. The user can
  mark a pair as "not duplicates" so it is never proposed again. Zotero cannot do
  that.

## Storage

- `LIB-40` (core) Two storage modes per library. Managed: the app owns the folder
  layout and names files from a template. Referenced: files stay where the user put
  them. A book can move from one mode to the other.
- `LIB-41` (core) Files stay plain files that open without the app. The folder
  layout is documented and stable.
- `LIB-42` (core) The user's data survives without the app's database. Metadata
  with its sources and locks, collections, reading state, annotations and deletion
  markers can be written beside the books in a documented open format, and a library
  can be rebuilt from those files. A rebuilt library does not bring deleted records
  back when it next syncs. The portable change log records published edits. The local database
  also holds edits awaiting publication. A hand-edited sidecar is reconciled
  against its base. [Storage design](../design/library-storage.md)
- `LIB-43` (core) The app never edits a book file unless the user asks. Writing
  metadata into files is a separate action. Sync clients identify a book by its hash,
  so a silent edit breaks their match.
- `LIB-44` (core) Moving or renaming a file outside the app does not lose the book.
  The app finds it again by content hash. Kavita misses renames because it compares
  folder times, and Audiobookshelf loses progress when a book is added again.
- `LIB-45` (expected) File name and folder templates, with safe characters and
  length limits for each target file system.
- `LIB-46` (expected) A library on a network share or in a cloud-synced folder is
  safe. The app does not keep a live database file where a sync tool can corrupt it,
  and a "network storage" setting turns off actions that are risky there. Zotero
  tells users not to do this, and corruption reports are common.
- `LIB-47` (expected) The library is portable: paths are relative, and it opens on
  another operating system.
- `LIB-48` (expected) Scheduled backups and a restore. An integrity check lists
  missing files, files with no record, changed hashes and a damaged index.
- `LIB-49` (expected) Three actions are kept apart: removing a download from one
  device, deleting a book from the catalogue, and erasing every copy for good. The
  app says which devices and which referenced files each one touches. Deleting moves
  a book to a trash that can be restored. A
  book can also be hidden from the main views without deleting it.
  Removing the file from disk is a separate, explicit step.
- `LIB-50` (core) Every change is journalled so it can be undone, and so sync
  has a record of what changed. One user action, such as a bulk edit, undoes as one
  step. Undoing a change that another device or user has since built on reverts only
  what is still as the user left it, and asks about the rest.
- `LIB-51` (core) Upgrading the app never loses a library. The app backs up before
  it changes the format of its database or sidecar files, recovers from an upgrade
  that was interrupted, refuses a library made by a newer version with a clear
  message, and keeps syncing safely with a device that runs an older version.

## Several libraries

- `LIB-60` (expected) The user keeps several libraries, switches between them and
  searches across all of them. Each has its own storage mode, naming template,
  metadata source order and default reading direction.
- `LIB-61` (expected) Move or copy books between libraries with their metadata,
  covers, annotations and reading state.
- `LIB-62` (later) A library has a kind (books, comics, audiobooks, papers) that
  sets its default views and fields. A mixed library is allowed.

## Speed

- `LIB-70` (core) The app starts and searches fast at 100,000 books and stays usable
  at 1,000,000. Proposed targets on an ordinary laptop: the window shows the library
  within 2 seconds of launch, and a quick search answers within 100 milliseconds. It never reads every file at start. Reports of existing tools slowing
  or failing start at about 20,000 books. The catalogue meets its open and search targets at 100,000 books and stays within the benchmark's looser limits at 1,000,000, where a search takes about 150 ms. The Linux window shows the first titles of 100,000 books about 0.26 seconds after launch and answers a search as the reader types. [Desktop behaviour](../../specs/desktop-mvp.md#large-libraries) [Catalogue benchmark](../design/library-browsing.md#the-benchmark)
- `LIB-71` (core) The grid scrolls smoothly at any size, with covers drawn from
  cached thumbnails. The Linux grid creates rows only for what is on screen and draws covers from cached thumbnails. Its frame rate has not been measured. For the shelf, the scale driver measures each sampled frame from before update through after paint, including layout, while the scroll position moves in the private X11 session. Whole GTK frames exceeded 16.7 ms on the Cairo software renderer. Run maxima on two CPUs under Ubuntu 26.04 have ranged from 18.6 to 28.6 ms. The software frame budget and hardware presentation-time checks both remain open.
- `LIB-72` (core) No long job blocks the window: import, conversion, metadata fetch,
  indexing, device transfer. On Linux, import, the survey of covers and sending to a device run off the window's thread, and during an import the list refreshes at most once a second and keeps its place. Conversion, metadata fetch and indexing do not exist yet.

## What the app must store

- Book, edition and file as separate records, with links for "same work",
  "translation of", "contains" and "derived from".
- Per file: path or location, storage mode, format and version, size, whole-file
  hash, a quick partial hash, a hash of the text content, the dates it was added and
  last seen, and its state (present, missing, in trash).
- Person records with roles per book, name variants and identifiers.
- Series records and book-to-series links with a text position and a sort key.
- Collections with ordered membership; smart collections with their query and where
  they are pinned; reading lists with unmatched entries and remap rules.
- Tag tree. Custom field definitions and values.
- Per-library settings. View settings per collection.
- Per book, for the shelf view: a spine colour taken from the cover at import, and
  the book's length in words where it is known.
- Import log per file, the review queue, duplicate candidates and "not a duplicate"
  decisions.
- A change journal, backups made, and the versions of the database and sidecar
  formats.
- Rebuildable caches: search index, full-text index, thumbnails.

## Sources

Round-two reports on self-hosted servers and on user discussions, Zotero's support
pages, the Komga, Kavita and Audiobookshelf documentation. Library sizes and speed
complaints come from forum threads and are anecdotes, not measurements.
