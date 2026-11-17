# Libraries on other machines, sync and sharing

One person with a desktop, perhaps a home server, a phone and a tablet wants one
library on all of them. A household wants to share it. This area decides more of the
data model than any other, because every record must be able to change on two devices
at once.

## One library, several devices

- `SYNC-1` (core) A library on one machine can be opened from the user's other
  devices. Edits made on any of them reach the rest.
- `SYNC-2` (core) Every device works fully offline with what it holds and catches
  up when it reconnects.
- `SYNC-3` (core) A phone holds the whole catalogue, with covers as thumbnails, and
  only the book files the user wants. A book that is not downloaded still shows its
  details, status and position.
- `SYNC-4` (core) What syncs: metadata, covers, collections, tags, custom fields,
  reading positions, annotations, reading history, device records and settings. Book
  files sync on demand or by rule.
- `SYNC-5` (core) No account with the project is needed, and no server on the
  public internet. Syncing the user's own data is never a paid feature. Readest and
  others lost users by charging for it.
- `SYNC-6` (expected) Rules for which files a device keeps: a collection, a series,
  unread books, recently added, up to a size limit. The user can pin a book.
  Finished books can be removed automatically. Downloads resume, and can be limited
  to Wi-Fi.
- `SYNC-7` (expected) Positions and annotations sync within seconds to minutes
  while reading, and on closing the book.
- `SYNC-8` (expected) Settings that describe a device's screen stay on that device
  unless the user asks for them to sync.
- `SYNC-9` (expected) The user can see what has synced, what is waiting and what
  failed, per book, and can repair the sync state. Kobo had to add a repair step.
- `SYNC-10` (core) Turning sync off on a device leaves its data in place. Apple
  Books empties the library.

## Ways to connect

The owner asked for libraries managed "on other devices or locally". These are the
arrangements the design must allow. Which to build first is an open decision in the
[README](README.md).

- `SYNC-20` (expected) A hosted library: one machine, such as a desktop or a home
  server, holds the files, and the others connect to it.
- `SYNC-21` (expected) Direct sync between the user's devices on the home network
  with no always-on machine.
- `SYNC-22` (expected) Sync through storage the user already has: a folder kept in
  step by Syncthing, iCloud Drive or Dropbox, or a WebDAV server. A live database
  file in such a folder gets corrupted, so the folder holds separate change files,
  which a sync tool can copy safely.
- `SYNC-23` (later) A server mode that runs with no window on a NAS or a small
  computer, set up from a browser.

## Merging edits

- `SYNC-30` (core) Two devices edit the same book offline and neither edit is lost
  silently. Fields merge one by one, and the later edit of a field wins. A real
  clash, the same field changed two ways, is shown to the user where it matters.
- `SYNC-31` (core) Sets merge as additions and removals: tags, collection
  membership, contributors. Adding a tag on one device and another tag on a second
  leaves both.
- `SYNC-32` (core) Deleted records leave a marker so they do not return.
- `SYNC-33` (core) Annotations and read-through records are independent items with
  permanent identifiers, so they merge without conflict. Note text edited in two
  places keeps both versions.
- `SYNC-34` (core) Book files are identified by content hash and never conflict. A
  changed file is a new version.
- `SYNC-35` (core) Clocks on the user's devices disagree. Ordering does not depend
  on wall-clock time alone.
- `SYNC-36` (expected) History is kept for a while, so a bad merge can be undone.

## Several people

- `SHARE-1` (expected) A library can have several users. The catalogue is shared.
  Each user has their own positions, annotations, history, ratings, collections and
  settings.
- `SHARE-2` (expected) A user is given access per library, and can be limited by
  age rating, tag or collection. An exclusion beats an inclusion. The app says how a
  series is treated when one book in it is restricted, and the choice is the
  owner's.
- `SHARE-3` (expected) Roles: owner, editor, reader, guest. Downloading files and
  editing metadata are separate permissions.
- `SHARE-4` (expected) Child profiles need no password and collect no data beyond
  the device.
- `SHARE-5` (expected) Each device or client app gets its own key, which can be
  revoked alone.
- `SHARE-6` (later) Shared collections and reading lists. Shared highlights for a
  reading group.
- `SHARE-7` (later) Sign-in through an identity provider (OIDC) for people who run
  one.
- `SHARE-8` (later) A link that shares one book with someone for a limited time.

## Serving other apps

- `SERVE-1` (expected) An OPDS catalogue, version 1.2 and 2.0, with search, facets
  and paging, per user. It exposes the user's collections, series and "continue
  reading". Project Gutenberg retires its OPDS 1 feeds in 2027, so version 2 comes
  first.
- `SERVE-2` (expected) Page streaming for comics, so a client reads a large archive
  page by page.
- `SERVE-3` (expected) Position sync for outside readers. OPDS is download-only,
  and users name that as a gap, so the app also answers KOReader's sync protocol and,
  later, Kobo's (see [devices](07-devices.md)).
- `SERVE-4` (expected) A documented API with per-client keys, so other tools can
  script the library. A command-line tool covers the same actions.
- `SERVE-5` (later) A web reader for a browser on a machine with no app installed.
- `SERVE-6` (core) Anything the app serves listens only on the local machine until
  the user turns on network access, and then requires a key. See
  [constraints](10-constraints.md).
- `SERVE-7` (later) Browse outside OPDS catalogues inside the app and add books
  from them. See [acquisition](09-acquisition.md).

## Extending the app

- `EXT-1` (later) Data-only extensions: metadata source definitions, device
  profiles, export templates, themes, dictionaries. These are allowed on every
  platform.
- `EXT-2` (later) Code extensions on desktop only. Apple's store rules forbid an app
  from downloading code that changes what it does. Extensions run with declared
  permissions, and a broken one cannot stop the app. Extensions that break on each
  update are a standing complaint about existing tools.
- `EXT-3` (later) Automation rules: when a book is added, tag it, fetch metadata,
  convert it, send it to a device.

## What the app must store

- A permanent identifier for every record, created on the device that made it and
  unique across devices.
- For every record and field: which device changed it and a version that orders
  changes without trusting clocks.
- Deletion markers, with a rule for when they can be dropped.
- A change log per library, and for each device how far it has read that log.
- Per device: which files it holds, its download rules and pins, and its transfer
  state.
- Users, roles and permissions, per-library grants and restrictions. Keys per
  client, with what each may do and when it was last used.
- Which data is per user (positions, annotations, history, ratings, collections,
  settings) and which is shared (the catalogue). The split must be in the model from
  the first version, even if the first release has one user.
- Conflict records waiting for the user.
- Outside catalogues the user added. Extension records: kind, origin, version,
  permissions.

## Not verified

No product's documentation states a library size limit or describes how it finds a
moved file by content. How Kavita, Komga and Audiobookshelf behave under two-device
edits needs testing, not reading. Apple's rules on local network use and background
work could not be read from its developer pages.

## Sources

Zotero's sync documentation, the Kavita, Komga and Audiobookshelf documentation, the
OPDS 1.2 and 2.0 specifications, the KOReader sync server's README, Apple's App Review
Guidelines, Android's foreground service documentation, and the round-two reports on
self-hosted servers and user discussions.
