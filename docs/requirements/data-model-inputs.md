# What the requirements ask of the data model

This page gathers, from every area, the facts a storage design must respect. It is
input to the design, not the design. It names the things that must be representable
and the rules they follow, and leaves tables, keys and file layouts to the design
work.

## Twelve rules that shape everything

1. **A book is not a file.** There are three levels: the work, an edition of it
   (a translation, a narration, an abridgement, a revision) and a file. A work has
   many editions, an edition has many files, a file can hold several works, and a
   record can have no file at all. ([Library](01-library.md),
   [content types](02-content-types.md))
2. **A file has an identity that survives its path and its bytes.** Each file needs
   a whole-file hash, the quick partial hash that sync clients compute, and a hash of
   its text content. When a file is rewritten, converted or replaced, the old hashes
   stay as aliases of the new file. Positions, annotations, device copies and outside
   sync all hang on this. ([Annotations](05-annotations-history.md),
   [devices](07-devices.md))
3. **A place in a book is stored in several forms at once**, tied to the file
   version it was made in: part and fraction, fraction of the whole, structural path,
   and surrounding text. Positions, annotations, chunks, summaries and audio
   alignment share this one form. ([Annotations](05-annotations-history.md))
4. **The user's data lives outside the book file**: annotations, positions,
   history, ratings. Writing into a file is an explicit export.
5. **Every record can change on two devices at once.** Each needs a permanent
   identifier made without a central server, a version that orders changes without
   trusting clocks, per-field merging, add and remove semantics for sets, and a
   deletion marker. ([Sync](08-sync-sharing.md))
6. **Shared and personal data are separate from the first version.** The catalogue
   is shared. Positions, annotations, history, ratings, collections and settings
   belong to a user. ([Sync](08-sync-sharing.md))
7. **Every metadata field remembers where it came from**: the file, a sidecar, the
   user, a named source, a conversion. It also records whether the user locked it and
   whether its source allows it to be stored. ([Metadata](03-metadata.md))
8. **Names are records, and lists are rows.** People, series, publishers, tags and
   subjects are records with their own identifiers and variants. Contributors,
   identifiers, titles and languages are ordered rows, never joined strings.
9. **Numbers that order things are text with a sort key.** Series positions, issue
   numbers and volume labels include `1.5`, `2.2.1`, `1-5`, `TPB1`, `2015 Special`.
10. **Reading is an event log.** Sessions, read-throughs and position history are
    kept as records with times. Status, statistics, streaks and time-left are
    computed from them.
11. **Rebuildable data is kept apart from the user's data**: search indexes,
    thumbnails, extracted text, vectors, generated summaries, speech audio. Each
    derived item records the hash of what it was built from.
    ([Derived features](11-derived-features.md))
12. **Imports keep their raw input.** The original row, file or device snapshot is
    stored with the parser version, so a lossy or buggy import can be redone.

## The things that must be representable

Grouped by who owns them. Each line names a thing and the area that explains it.

### Catalogue, shared by everyone who uses a library

- Library, with its settings: storage mode, naming template, source order, default
  reading direction, kind. (01)
- Work, edition, file, and the links between them: same work, translation of,
  abridgement of, aligned audio of, contains, derived from, replaced by. (01, 02)
- File facts: location, storage mode, format and version, size, the three hashes
  and their aliases, layout kind, protection scheme, flags (scripted, remote
  resources, text layer, page list), state, and the metadata it carried at import.
  (01, 02)
- Parts of a file or item: spine items, audio files with offsets, comic pages with
  type and size. Chapters with their source and the edition they belong to. (02)
- Titles, contributors with roles, identifiers, languages, subjects, dates,
  description, rights, age rating, accessibility facts, layout facts. (03)
- Person, with name variants, sort form, other scripts and authority identifiers.
  Publisher and imprint. Series, with kind, nesting and per-source names. Tag tree.
  (01, 03)
- Covers: the chosen one and the candidates. (03)
- Custom field definitions and values. (01)
- Field provenance, locks and storage class. Candidate records awaiting review.
  (03)
- Collections that are shared, reading lists with unmatched entries and remap
  rules, smart collections with their query. (01)
- Acquisition records, loans, serial and feed sources, paper copies, wish-list
  entries. (09)
- Conversion and edit history: derived-from links with settings, versions of edited
  files, maps of renamed parts, check results. (06)
- Alignment maps between text and audio. (02, 11)

### Personal, owned by one user

- Reading status. Read-through records. Sessions. Position per device, with
  history. (05)
- Annotations with several anchors each, ink strokes, notebooks. Annotation tags,
  colour meanings, review schedule. (05)
- Rating, review, private notes. Goals. (05)
- Personal collections and their manual order. Saved searches. (01)
- Reader settings at each scope: global, library, series, book, device. Themes,
  fonts, vocabulary, dictionaries, voices. (04)
- Per-book permissions for scripts and network. Private flag. (10)
- Accounts at outside services, the identifier each service uses for each book,
  what was last sent and what is queued. (05)
- Export templates and the record of what was exported where. (05)

### About devices and sync

- This app's installations: an identifier for each, what files it holds, its
  download rules, how far it has read the change log. (08)
- Reading devices: identifiers, transport, firmware, volumes, profile, rules. (07)
- What the app believes is on each reading device, and the journal of transfers.
  (07)
- Records imported from devices, with source labels and raw snapshots. (07)
- The change log, deletion markers and conflicts awaiting the user. (08)
- Users, roles, per-library grants and restrictions, client keys. (08)

### About the app's own conduct

- Per metadata source: on or off, rank, rate-limit counters, last refusal, key
  reference. The log of lookups. (03)
- The log of data that left the machine, and consent per service. (10, 11)
- Folder grants with platform tokens. (10)
- Jobs: kind, target, settings, state, progress, result. (06)
- Import log, review queue, duplicate candidates and "not a duplicate" decisions.
  (01)
- Backups made, schema version, sidecar format version. (01)
- Extensions and the manifest of bundled parts with licences. (08, 10)

### Rebuildable

- Search index and full-text index. Thumbnails. Extracted text and chunks.
  Vectors. Generated summaries, entity lists and translations. Speech audio. (01, 11)

## Questions the design must answer

These are the hard parts. Each has a requirement that constrains the answer.

- **Database or sidecar files as the source of truth.** `LIB-42` says the library
  must be rebuildable from files beside the books. `LIB-46` says no live database in
  a synced folder. `LIB-70` says fast at 100,000 books. A local database as a cache
  over per-book files, or a database with a continuous export, both fit. A database
  alone does not.
- **How changes are ordered and merged.** `SYNC-30` to `SYNC-36`. The design must
  choose a scheme: a change log with logical clocks, last-writer-wins per field, or
  data types that merge by construction.
- **Where the line between work, edition and file sits for ordinary users.** Most
  users have one file per book and should never see three levels. The model needs
  the levels and the interface should hide them until they matter.
- **How a place in a book is encoded** so that EPUB, PDF, comics and audio share
  one record shape (`POS-1`).
- **How an item with thousands of parts is stored** without a row per page costing
  too much: comic pages, audio tracks, web archives.
- **What is per user and what is per library** for collections, tags and custom
  fields. A household may want shared tags and private collections.
- **How typed items are modelled**: one record with a kind and optional field sets,
  or separate kinds for books, comics, audiobooks, periodicals and papers
  (`LIB-12`).
- **How custom fields are stored** so they sort and search as fast as built-in
  ones at 100,000 books (`LIB-7`, `LIB-70`).
- **How long history and deletion markers are kept**, and what compaction does to
  a device that has been offline for a year.
- **How data with a "do not store" term is held** long enough to show and review,
  then dropped (`META-42`).

## Sizing

Design for these and test against them: 100,000 books as the comfortable case and
1,000,000 as the case that must still work; 10 files per book in the worst case;
10,000 annotations in one heavily studied book; 5,000 pages in one comic archive;
reading sessions for 20 years of daily reading; 10 devices per user and 10 users per
library. These figures are the author's proposal, drawn from the library sizes users
report. They are not measurements.
