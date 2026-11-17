# Reading devices

Getting books onto e-readers, knowing what is on them, and bringing back what the
reader did there.

## Connecting

Devices no longer all appear as a USB drive. This decides the design.

| Family | Over USB | Notes |
|---|---|---|
| Kindle up to 2023 | Drive | Books go in the `documents` folder |
| Kindle from 2024, Scribe | MTP, by third-party reports | Amazon tells Mac users to install a helper app and gives Linux users nothing |
| Kobo | Drive, after tapping Connect on the device | Books anywhere outside hidden folders. The device imports on eject |
| PocketBook | Drive | |
| Nook | Drive, after tapping Connect | Books go in `My Files` |
| Tolino | Drive on older models, MTP on Vision 6, Shine 4, Epos 3 | Books go in `Books` |
| Onyx Boox, Supernote | MTP | Android based. Mac users need a helper today |
| reMarkable | A network interface with a web page | PDF and EPUB, 100 MB limit |
| Android phone or tablet | MTP | |
| iPhone, iPad | No USB file access for other apps | Wi-Fi routes only |

- `DEV-1` (core) Send books to a device that mounts as a drive.
- `DEV-2` (core) Send books to a device that speaks MTP, on macOS, Linux and
  Windows, with no helper app. MTP has its own rules: listing is slow, a file's size
  must be known before sending, a file cannot be changed in place, and only one
  program can hold the device. On macOS a system service grabs the device first. The
  app shows progress, allows cancel, and explains a busy device in plain words.
- `DEV-3` (core) The transport is chosen per device when it connects, not per
  brand. One brand ships both kinds.
- `DEV-4` (core) The app recognises a device it has seen before and tells two units
  of one model apart. It stores several identifiers per device, including a small
  marker file it writes itself, because many units report no usable serial number.
- `DEV-5` (expected) A device the app does not know can be set up by hand: where
  books go, which formats it reads, and its file name rules.

## Sending

- `DEV-10` (core) Send a selection to a device. Each book is sent in the format the
  device prefers and is converted first if needed.
- `DEV-11` (core) Before a batch starts, the app checks free space and the device's
  limits on name length, path length and file size, and says which books will not
  fit.
- `DEV-12` (core) A dry run lists the files, names and folders that would be
  written.
- `DEV-13` (core) A transfer survives an unplugged cable: files are written under a
  temporary name and renamed, and an interrupted batch can resume. The app reminds
  the user to eject drives safely.
- `DEV-14` (core) Each device has a profile: preferred formats in order, conversion
  settings, folder and file name template, cover handling, and how collections are
  expressed.
- `DEV-15` (core) The cover and metadata inside the sent file are correct and
  complete, because the device builds its own library from the file. The app writes them into
  the copy it sends and leaves the library's file alone. Device-side thumbnail
  folders are an extra per family.
- `DEV-16` (expected) The app shows which library books are on the device, which
  device books are not in the library, and which copies are out of date because the
  library's metadata or file changed since they were sent.
- `DEV-17` (expected) After a transfer and on each later connection, the app looks
  at the device again and marks each sent book as confirmed, missing, changed or
  renamed. Kindles have deleted USB-loaded books after a period offline, so the app
  warns when a book it sent has vanished.
- `DEV-18` (expected) Update the metadata or cover of books already on the device.
  Remove books from the device.
- `DEV-19` (expected) Automatic rules per device: keep this collection, this tag,
  the "want to read" list, or the next unread books of each series on the device,
  and remove finished ones.
- `DEV-20` (expected) Series and collections appear on the device where the family
  allows it: through folders, through metadata in the file, or through the device's
  own data.
- `DEV-21` (expected) Free and used space per storage volume. An SD card is a
  second volume of the same device.
- `DEV-22` (expected) Two copies of one book on a device, in two formats or under
  two names, are shown and not sent a third time.
- `DEV-23` (core) The app does not write to a vendor's database on the device by
  default. Vendors change those databases with firmware updates and do not document
  them.

## Bringing data back

- `DEV-30` (expected) Import reading positions, read status, reading time,
  highlights, notes and bookmarks from devices. Sources seen in practice: Kindle's
  clippings text file, Kobo's device database and its exported notebook files,
  KOReader's per-book sidecar folders and its statistics database, and the exports
  PocketBook, Boox, reMarkable and Supernote offer.
- `DEV-31` (expected) Device data is read from a copy, never from the live file.
  The app records the firmware version and refuses or warns when the layout is one
  it does not know.
- `DEV-32` (expected) Imported records stay labelled with their device and source,
  and an import can be repeated without duplicates. See
  [annotations](05-annotations-history.md).
- `DEV-33` (expected) A device record is matched to a library book by the hash of
  the file the app sent, then by identifiers, then by title and author, with the
  user confirming uncertain matches.
- `DEV-34` (later) Handwritten notebooks from stylus devices are imported as
  documents.

## Without a cable

- `DEV-40` (expected) Send to Kindle by email, to the device's address, from an
  address the user has approved with Amazon. Limits the app enforces: 50 MB a
  message, 25 attachments, and the file types Amazon accepts. Amazon converts EPUB on
  arrival. Setting up mail must be simple. Users describe it as clunky in existing
  tools.
- `DEV-41` (expected) An OPDS catalogue of the library that reader apps and
  KOReader can browse and download from. See [sync and sharing](08-sync-sharing.md).
- `DEV-42` (expected) A web page on the local network that a device's browser can
  download from.
- `DEV-43` (expected) Position sync with KOReader through its open sync protocol.
  The protocol identifies a book by a hash of the file, or by file name, and keeps
  only the furthest position. The app computes the same hash for every file it sends
  and keeps the old hash when a file changes.
- `DEV-44` (later) Sync with Kobo e-readers over Wi-Fi by answering as the device's
  sync service: books, covers, collections, positions and reading state, with kepub
  made on request. This is among the most requested features of self-hosted book servers.
  It must work on the home network without exposing a server to the internet.
- `DEV-45` (later) Cloud folders a device already watches, such as the Dropbox
  folder some Kobo models read, as a delivery route.
- `DEV-46` (later) WebDAV and the wireless routes PocketBook, Boox and Supernote
  provide.

## From a phone

- `DEV-50` (later) An Android phone sends books to a device over a USB-C cable.
  Android lets an ordinary app talk to USB drives and MTP devices after a permission
  prompt, and a long transfer runs as a visible foreground task.
- `DEV-51` (later) An iPhone sends books over Wi-Fi only: Send to Kindle, OPDS, the
  local web page, a cloud folder, or the share sheet into another app. iOS gives
  apps no cable access to e-readers.

## What the app must store

- Device: label, which users may send to it, family, model, firmware string when seen, USB vendor and product
  numbers, USB and MTP serial numbers, the app's marker identifier, transport, last
  seen, volumes with capacity, and its profile.
- What the app believes is on each device: book, format, hash of the exact file
  sent, path or object reference, size, modified time, date sent, the version of the
  library metadata it carries, and its state (pending, sent, confirmed, missing,
  changed, renamed).
- The file name actually written beside the title it came from.
- Per-device rules: what to keep on it, what to remove, conversion preset.
- Transfer journal, so an interrupted batch can resume.
- Imported device records with source, firmware, raw identifier and an import
  identifier. Raw snapshots of device files.
- Wireless endpoints and accounts: Kindle addresses and the sender address, sync
  server address and matching mode, with secrets in the keychain.
- Per book and file: the hash a sync client would compute.

## Not verified

These need real devices, not more reading:

- Which 2024 to 2026 models use MTP and which mount as a drive.
- What a current Kindle does with each file type copied over a cable, and when
  covers, series and collections show for such books.
- The layout of Kindle's clippings and sidecar files, and of Kobo's database.
- USB identifiers and serial files per family.
- Whether a phone can drive each e-reader over a cable.
- What iOS allows, which was not readable from Apple's developer pages.

## Sources

Amazon, Kobo, Barnes & Noble, Tolino, reMarkable, Supernote and PocketBook support
pages, the KOReader user guide and its sync server's README, the libmtp README, the
documentation of the `mtp-rs` Rust crate, Android's USB host and MTP references, and
the round-two reports on devices, Kindle and self-hosted servers.
