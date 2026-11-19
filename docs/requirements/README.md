# Requirements

What Ebook Manager has to do, collected before the first storage design. The
feature set decides how the data is structured, so the requirements come first.

These pages describe planned behaviour. [`specs/`](../../specs/README.md) describes
behaviour that exists. When a requirement is built, its behaviour moves into a spec
and the requirement is marked done or removed. The backlog of work is in
[GitHub issues](https://github.com/endqwerty/ebook-manager/issues), and each issue
names the requirement identifiers it covers.

## Pages

| Page | Covers | Identifiers |
|---|---|---|
| [Library](01-library.md) | Organising, search, import, storage, backups, several libraries, speed | `LIB` |
| [Formats and kinds of content](02-content-types.md) | File formats, protected files, comics, audiobooks, periodicals, serials | `FMT` |
| [Metadata](03-metadata.md) | Fields, editing, fetching from sources, what each source allows | `META` |
| [Reading](04-reading.md) | The reader for text, PDF, comics and audio, read aloud, access needs | `READ` |
| [Positions, annotations and reading history](05-annotations-history.md) | Where the reader is, what they marked, what they read, statistics, export | `POS`, `ANN`, `HIST` |
| [Conversion, checking and editing](06-conversion-editing.md) | Converting formats, validating and repairing files, editing contents | `CONV`, `CHK`, `EDIT` |
| [Reading devices](07-devices.md) | Sending books to e-readers by cable and wirelessly, importing device data | `DEV` |
| [Libraries on other machines, sync and sharing](08-sync-sharing.md) | One library on several devices, merging, households, OPDS, API, extensions | `SYNC`, `SHARE`, `SERVE`, `EXT` |
| [Where books come from](09-acquisition.md) | Purchases, loans, open catalogues, serials, paper books | `ACQ` |
| [Limits from outside](10-constraints.md) | Law, licences, store rules, privacy, safety | `LAW`, `LIC`, `PLAT`, `A11Y`, `PRIV`, `SEC`, `DATA` |
| [Features built on a book's text](11-derived-features.md) | Search by meaning, summaries, speech, audio alignment | `DERIV` |
| [What the requirements ask of the data model](data-model-inputs.md) | The rules and records the storage design must respect | |
| [Research notes](research/README.md) | The research reports these pages are drawn from, with every source | |

## Reading a requirement

Each requirement has an identifier, a priority and one statement of what a user can
do or what the app guarantees.

- **core**: needed for a first release that someone could move their library into.
- **expected**: what users of comparable tools take for granted. Planned, after
  core.
- **later**: not planned yet. Listed so the data model leaves room for it.

A priority says when the feature is built, not when the model must allow it.
Several expected and later requirements constrain the first schema: editions and
containers, kinds of item, merging duplicates, several users, text and audio
alignment. [The data model page](data-model-inputs.md) lists them.

The priorities are a first proposal by the agent that wrote these pages. The owner
has not reviewed them.

## Scope

The owner's brief: read books in the app; import EPUB, PDF and other formats and
convert between them; transfer books to USB reading devices; manage libraries locally
and on other devices; edit metadata; fetch metadata from trusted sources; and match
the popular features of Apple Books, Google Play Books and Kindle.

The intended shape is a native desktop app for macOS and Linux, probably Windows, and
native apps for iOS and Android, fast and smooth with large libraries. The [platform design](../design/platforms.md) selects a shared Rust core and a native toolkit for each shell.

## What people ask for most

From user discussions, most dated 2022 to 2026 and a few older. The counts behind this
order are rough, and Reddit could not be read, so treat it as a guide.

1. One reading position across every device and app, for the ebook and the audiobook
   together.
2. Sync with Kobo and KOReader devices built in, with no server exposed to the
   internet.
3. Speed that holds at 20,000 books and beyond.
4. Import of an existing library, without changing it.
5. One searchable place for highlights from every device, with export.
6. A clear interface with fewer nested menus and full keyboard control.
7. Running on a home server, with a web view and a phone app.
8. Fast bulk metadata editing that never silently overwrites the user's work.
9. Custom fields.
10. Households: each person has their own progress, shelves and permissions.
11. Sending to a device wirelessly, from a phone or a desktop, without mail server
    setup.
12. Automatic intake of a downloads folder, with a result for each file.
13. Plain files in a layout the user can read, with no forced folder scheme.
14. Catalogue access that also carries progress and highlights back.
15. Duplicate detection with a safe merge.
16. Comics, PDFs and audiobooks in the same library.
17. A command line and an API.
18. Reading statistics.
19. Backups, integrity checks and recovery from corruption.
20. Serials and fan works that update without losing the reader's place.

## What a new manager must promise

People do not move a library into a tool they do not trust. Several projects in this
field were abandoned, deleted by their author, or moved features behind a payment
between 2024 and 2026, and users name each of these as a reason to stay put.

- It opens an existing library read-only first and changes nothing without a report
  and an undo.
- Books stay plain files that open without the app. Everything the user created can
  be exported: metadata, annotations, positions, history.
- Backups and integrity checks are built in. Network storage is safe.
- Syncing your own data is free.
- There is a way out as well as a way in.
- The licence is open, and more than one person can keep the project alive.

## Decisions for the owner

The [implementation design](../../README.md#implementation-design) records decisions on storage, sync and platforms. The other rows give the working assumptions for their areas.

| Question | Why it matters | Assumed |
|---|---|---|
| Which sync arrangement comes first: a hosted library, direct sync between devices, or sync through the user's own storage? | It sets the merge design and what the phone apps need | Design the model so all three work. Build direct sync on the home network first |
| Is a household with several users in the first release? | Personal and shared data must be separate from the first schema either way | The model has users from the start. The first release has one |
| Do comics and audiobooks belong in the first release? | They force the work, edition, file and parts model | The model holds them. Reading them is "expected", after text and PDF |
| Database or files beside the books as the source of truth? | Portability and safety against speed | A local database for speed and pending edits. Recover published changes from the portable log |
| Should Kobo wireless sync be built? | It means answering as Kobo's own service. It is among the most requested server features and has no public specification | Listed as "later". Needs black-box testing on a device |
| Should the project seek Readium LCP certification? | It is the one protection scheme found with a published licensing route for independent readers, and some library loans use it. It needs a contract, a fee and a closed module | Not now. The app catalogues LCP files and says why it cannot open them |
| Are features that send book text to a service ever acceptable? | Authors' groups object to them, and reading data is sensitive | Local or off by default, with consent per book and per service |
| Which app stores will the project ship through? | Store rules remove extensions, cable transfer and background work on iOS, and limit folder access on Android | Direct downloads on desktop. Stores on mobile |
| Is importing an existing calibre library a first-release feature? | It is the first thing most prospective users check | Yes, by reading the files calibre writes, which Rule 1 allows |

## How this was researched

Two rounds of research agents, twenty-two in all, each given one subject. The first
round listed features. The second read primary documents: specifications, vendor help
pages, API terms, store policies and user discussions. Each brief carried
[Rule 1](../../AGENTS.md#rule-1-never-look-at-calibres-source-code). No agent opened
calibre's source, manual or plugins, and none reported seeing calibre code. One agent
requested a made-up address on the website of calibre's author by mistake. It returned
"not found" and nothing was read.

The agents were told to make at most ten requests to any one website, one at a time,
and not to retry or work round a page that refused them. One agent reported two
slips: a few requests to one site sent together, and eleven requests to another.

Limits of the research:

- Many primary pages refused automated readers: Amazon's help site, Apple's
  developer documentation, Reddit, Hardcover, EUR-Lex and others. Facts that rest on
  a secondary source are listed under "Not verified" on each page.
- Nothing was tested on a device or against a running product. Device behaviour in
  particular needs hands-on checks, and those are in the backlog.
- Popularity rankings are tallies from a small sample of discussions.
- Statements about law and licences report what public documents say. They are not
  legal advice.
- Products change. Dated facts carry their date.
