# Library

How a user stores, organises, finds and protects their books. This area sets the
shape of the core records, so read it with
[data-model-inputs.md](data-model-inputs.md).

Priorities: **core** is needed for a first usable release, **expected** is what users
of comparable tools take for granted, **later** means store the data now and build the
feature afterwards.

## Organising

- `LIB-1` (core) A book has a title, sort title, authors and other contributors with
  roles, series, tags, publisher, languages, identifiers, dates, a description, a
  rating and a cover. [Metadata](03-metadata.md) lists every field.
- `LIB-2` (core) One book holds several files: the same text as EPUB and PDF, a
  converted copy, an audiobook. Deleting one file leaves the book.
- `LIB-3` (core) A book belongs to any number of series, each with a position. A
  position is text with a sort key, so `1.5`, `2.2.1`, `1-5` and `TPB1` all sort
  correctly. No tool surveyed sorts non-numeric positions well.
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
  columns. Each view remembers its sort.
- `LIB-21` (core) Sort by any field, with a second and third key. Sorting follows
  the locale: leading articles are skipped by a per-language rule, numbers sort as
  numbers, and the user can edit any sort key by hand.
- `LIB-22` (core) Typing in the search box filters at once across title, author,
  series and tags.
- `LIB-23` (expected) A query language with `field:value`, AND, OR, NOT, ranges,
  "is empty" and comparison of dates and numbers. Custom fields and reading state
  are searchable.
- `LIB-24` (expected) Facets with counts: author, series, tag, language, format,
  rating, reading status, source, device.
- `LIB-25` (expected) Full-text search inside books, with a snippet and a jump to
  the hit. The index is a cache the app can rebuild.
- `LIB-26` (expected) A home screen with Continue reading, Recently added and the
  user's pinned collections.
- `LIB-27` (expected) Bulk edit with a preview of every change and undo.
- `LIB-28` (core) Every action is reachable from the keyboard. This is a common
  complaint about existing tools.

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
- `LIB-42` (core) The user's data survives without the app's database. Metadata,
  reading state and annotations can be written beside the books in a documented open
  format, and a library can be rebuilt from those files. Whether the database or the
  sidecar wins a disagreement is an open decision in the [README](README.md).
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
- `LIB-49` (expected) Deleting moves a book to a trash that can be restored. A
  book can also be hidden from the main views without deleting it.
  Removing the file from disk is a separate, explicit step.
- `LIB-50` (core) Every change is journalled so it can be undone, and so sync
  has a record of what changed.

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
  beyond that. It never reads every file at start. Reports of existing tools slowing
  or failing start at about 20,000 books.
- `LIB-71` (core) The grid scrolls smoothly at any size, with covers drawn from
  cached thumbnails.
- `LIB-72` (core) No long job blocks the window: import, conversion, metadata fetch,
  indexing, device transfer.

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
- Import log per file, the review queue, duplicate candidates and "not a duplicate"
  decisions.
- A change journal, backups made, and the versions of the database and sidecar
  formats.
- Rebuildable caches: search index, full-text index, thumbnails.

## Sources

Round-two reports on self-hosted servers and on user discussions, Zotero's support
pages, the Komga, Kavita and Audiobookshelf documentation. Library sizes and speed
complaints come from forum threads and are anecdotes, not measurements.
