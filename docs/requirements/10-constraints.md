# Limits from outside

Law, licences, store rules and safety. These remove some features, reshape others,
and add records to the data model. This is a summary of what public documents say,
not legal advice.

## Protected books

- `LAW-1` (core) The app does not ship, link to, host or describe a way to remove
  protection from a book. Its documentation, issue tracker and any list of
  extensions refuse them too. United States law exempts certain acts of
  circumvention for certain people, such as readers with print disabilities, and
  does not exempt distributing tools.
- `LAW-2` (core) When a user opens a file protected by a scheme the app does not
  support, the app names the scheme, says it cannot open it, keeps the book in the library, and offers no workaround.
- `LAW-3` (later) Readium LCP is the only scheme found with a published route for an
  independent reader to support it. Other schemes were not researched. It needs a signed agreement with EDRLab, a yearly fee, a yearly
  conformity check and a closed module that cannot live in a public repository. The
  app must build and run without it. Whether EDRLab would certify an open-source
  app is an open question for the owner.

## Licences of parts

The project is Apache 2.0. Code under the GPL or AGPL cannot be built into it.

| Need | Compatible | Not compatible, or with a catch |
|---|---|---|
| EPUB and reader toolkits | Readium Swift and Kotlin toolkits (BSD) | An e-reader engine under the GPL |
| PDF drawing | PDFium, pdf.js | MuPDF (AGPL or paid), Poppler (GPL) |
| Conversion | Own code, kepub conversion tools under MIT | Pandoc (GPL): only as a separate program the user installs |
| OCR | Tesseract (Apache 2.0). Check each language data file | |
| Archives | libarchive for reading RAR | The unrar source has its own restrictive licence. RAR cannot be created |
| MTP | A Rust crate under MIT or Apache 2.0, still before version 1.0 | libmtp (LGPL): link it dynamically. iOS gives apps no USB access to MTP devices at all |
| Audio | The platform's own decoders on mobile | FFmpeg: LGPL only if built without GPL parts and linked dynamically |
| Speech | The platform's voices | Piper (GPL): only as something the user installs |
| Fonts | Fonts under the SIL Open Font License | Atkinson Hyperlegible: licence terms not confirmed |
| Dictionaries | Dictionaries the user installs | Wiktionary data is CC BY-SA: attribute it and keep it apart from the code |
| Storage | SQLite (public domain) | |

- `LIC-1` (core) No GPL or AGPL code is linked into the app. A tool under such a
  licence may be run as a separate program that the user installed, on desktop only.
- `LIC-2` (core) Every bundled part, font, model and data file is listed with its
  licence, and the list ships with the app.
- `LIC-3` (core) Vendor and format names (Kindle, Kobo, EPUB) are used only to say
  what the app works with, with a trademark notice, and never with their logos.

## Platform rules

| Feature | iOS | Android | macOS | Linux | Windows |
|---|---|---|---|---|---|
| Watched folders | Only while the app is open, in a folder the user picked | A folder the user picked. Access to all files needs Google's approval, which is uncertain | A folder the user picked, remembered by a bookmark that can go stale | Free outside Flatpak. Inside it, a portal grant | Free for a classic desktop app |
| Local server | Needs the local network prompt. Cannot run in the background | Runs as a visible foreground task | Allowed | Allowed. Flatpak needs the network permission | Allowed |
| Long jobs (import, conversion) | Only in the foreground | Foreground task with a notification, time limited for some kinds | Free | Free | Free |
| USB device transfer | Not possible | Possible, with a prompt per device | Possible outside the Mac App Store sandbox | Possible. Flatpak needs the USB portal | Possible |
| Code extensions | Forbidden by store rules | Risky under store rules | Possible | Possible | Possible |
| Linking to a store | Restricted outside the United States | | | | |

- `PLAT-1` (core) The desktop app runs on macOS and Linux, and Windows is expected.
  The mobile apps are native on iOS and Android.
- `PLAT-2` (core) Every folder the app may read is recorded as a grant from the
  user, with the platform's token for it, and the app copes when a grant goes stale.
- `PLAT-3` (core) Features that a platform forbids are absent there, not broken.
  The app's own sync covers the gap on iOS, where there is no cable transfer and no
  background service.
- `PLAT-4` (expected) The Android app works with folder grants alone. A build that
  asks for access to all files is separate and is not the store build.
- `PLAT-5` (expected) On iOS the app is more than a catalogue browser, and an
  in-app OPDS browser does not act as a shop. Apple's reviewers reject apps that are
  mainly links or that steer buyers to outside stores.

## Access for disabled readers

- `A11Y-1` (core) The reader meets the requirements in [reading](04-reading.md),
  `READ-70` to `READ-76`. European law has applied to e-book reading software since
  28 June 2025. Whether a free project falls under it is unsettled, so the app aims
  to conform anyway.
- `A11Y-2` (expected) The project publishes an accessibility statement and a way to
  report barriers.

## Privacy

- `PRIV-1` (core) The app works with no account and no network. Nothing leaves the
  machine unless the user turned that feature on.
- `PRIV-2` (core) No telemetry by default. Crash or usage reports are opt-in and
  show what would be sent.
- `PRIV-3` (core) A reading history can reveal religion, politics, health and
  sexuality, which European law treats as special categories. The app never sends
  the library list, reading history or annotations to any service as a side effect.
- `PRIV-4` (core) A log shows every time data left the machine: when, to which
  service, and what kind of data.
- `PRIV-5` (core) Secrets live in the operating system's keychain, never in the
  library or in an export.
- `PRIV-6` (expected) A book can be marked private: left out of sync, sharing,
  export and device rules.
- `PRIV-7` (expected) Deleting a book removes its derived data too: thumbnails,
  index entries, extracted text.
- `PRIV-8` (later) If the project ever hosts a sync service, it takes on data
  protection duties that a local app avoids. That is a separate decision.

## Opening untrusted files

- `SEC-1` (core) Scripts in a book are off by default. When allowed, they run in
  their own origin with no access to local files, the network or the app.
- `SEC-2` (core) Remote resources in a book are blocked by default, and loaded only
  over HTTPS when allowed. A book that asks for one is flagged, because a remote
  image can report that the book was opened.
- `SEC-3` (core) Archives are unpacked with limits on entry count, total size and
  compression ratio. Entries that point outside the archive, absolute paths and
  symbolic links are refused.
- `SEC-4` (core) XML is parsed without external entities or outside definitions.
- `SEC-5` (core) Parsing, drawing and conversion of files run in a separate process
  with no network and with limits on time and memory.
- `SEC-6` (core) PDF scripts, launch actions and attachments never run.
- `SEC-7` (core) Links out of a book open in the system browser after the user
  confirms.
- `SEC-8` (core) Anything the app serves is bound to the local machine by default,
  needs a key on the network, and serves book content from a different origin than
  the app's own pages.
- `SEC-9` (expected) An import records what a file tried to do: scripts found,
  remote resources referenced, limits hit.

## Fetched data

- `DATA-1` (core) Covers, descriptions and reviews fetched from sources belong to
  their publishers and writers. The app keeps them for the user's own library and
  household, and does not republish them.
- `DATA-2` (core) Each fetched field keeps its source and that source's terms. See
  [metadata](03-metadata.md).

## What the app must store

- Per file: protection scheme and whether this build can open it.
- Folder grants with the platform token, a stale flag and when each was last
  checked.
- Per book: permission for scripts and for network, and the log from `SEC-9`.
- The log of data that left the machine: when, which book, which service, what
  kind and range of data, and why. The user's consent per service and per book,
  with its scope, when it was given and when it lapses.
- Private flag per book.
- Per profile: whether it is a child's.
- A manifest of bundled parts and their licences.
- Per extension: kind, origin, signature, licence, permissions.

## Not verified

The text of the European directive and its exemption for very small businesses,
Apple's developer pages on sandboxing and local networks, Microsoft's packaging
rules, the licence of several parts in the table, and the EU's law on circumvention.
Each needs a person to read the primary text before the project relies on it.

## Sources

Apple's App Review Guidelines, Google Play's policy on access to all files, Android's
storage and foreground service documentation, the Flatpak sandbox documentation, the
United States rule at 37 CFR 201.40 and the Copyright Office's pages on the current
rulemaking, EDRLab's page for reading system providers, the Apache and GNU licence
compatibility pages, the licences of each part named, the W3C note mapping the
European Accessibility Act to EPUB, EPUB Reading Systems 3.3, and GDPR Article 9.
