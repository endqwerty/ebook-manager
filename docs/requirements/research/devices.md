ISSUES

Why ISSUES: about 24 pages were opened and read, but several important vendor pages failed (Fandom/MobileRead wiki 402/404, Amazon 503, Boox help 404, Apple developer pages returned only titles, the KOReader wiki page for statistics was empty). Those topics are marked [unverified]. No calibre source was opened. Search-result summaries mentioned calibre plugin and driver names; I did not open any calibre page or thread and use none of it. One page I opened (the-ebook-reader.com cover post) mentions calibre in its comments but shows no code.

Marking: [V] = read on a page I opened. [S] = only seen in a search-result summary, page not opened. [U] = unverified / general knowledge.

## 1. Findings

### 1.1 USB appearance in 2026, per family

| Family | USB mode | Notes and source |
|---|---|---|
| Kindle (up to 2023) | Mass storage drive, "Documents" folder | [V] Amazon USB page says transfers go to the Documents folder; 2024-and-later models need a separate app on Mac (digprjsurvey.amazon.com/csad/help/node/TCUBEdEkbIhK07ysFu) |
| Kindle 2024+ (new basic Kindle, Scribe, colour models) | MTP (not stated by Amazon on the page I read) | [V] Amazon: "Kindle Scribe and 2024-released Kindle devices require a separate application" on Mac; Send to Kindle app has a "USB File Manager", drag and drop only, max 2,000 files per batch, macOS 12+, Intel and M1-M3. [V] third-party: Amazon removed "Download and Transfer via USB" for library books on 2024 models (blog.the-ebook-reader.com 2025-01-15). [S] forum: newer Kindles are "MTP Kindles". Amazon's page does not say "MTP", so treat MTP as [S] |
| Kobo (recent) | Mass storage after the user taps "Connect" on the device; volume named KOBOeReader | [V] help.kobo.com article 360024775093; some hubs are not detected, plug directly; macOS Sonoma 14.4 problems fixed by skipping the hub |
| PocketBook | Mass storage (copy files in) | [V] pocketbook.ch page only says "connect and copy files"; folder layout not read [U] |
| Onyx Boox | Android tablet: MTP; one user reports PTP-only mode on a Tab Mini C blocking transfer | [V] the-ebook-reader.com 2023 "9 ways" (comment); Boox help via search says Mac needs Android File Transfer or OpenMTP [S] |
| Nook GlowLight | Mass storage after tapping "Connect"; volume "NOOK"; personal files go in "My Files" | [V] help.barnesandnoble.com article 5599565083547. Windows may offer a driver install for "Nook": cancel, none needed. EPUB and PDF named |
| Tolino | Older: drive. Vision 6, Shine 4, Epos 3: MTP | [S] mytolino.com FAQ (search summary). [V] sideloading FAQ: formats PDF, EPUB, TXT, MOBI, CBZ, CBR; books go in the "Books" folder; deselect the disk before unplugging |
| reMarkable | USB network interface; optional web interface at http://10.11.99.1 enabled in Settings > Storage | [V] remarkable.guide USB web interface: list, download PDF, download .rmdoc (v3.9+), upload goes into the last folder listed, thumbnails, search; no authentication described. [S] support page: PDF, EPUB, rmdoc, 100 MB limit over USB/web app |
| Supernote | Computer sees it over USB-C; on macOS an MTP app (OpenMTP, Android File Transfer) is required | [V] support.supernote.com/transfer-files |
| Android phones/tablets | MTP by default | [V] Wikipedia MTP; Android reference |
| iPhone/iPad | No MTP; external USB storage appears in Files | [U] Apple pages read returned titles only |

What each OS needs:
- Windows: MTP is built in since Vista [V Wikipedia]. MTP devices get no drive letter or UNC path, so ordinary file APIs cannot reach them; a program must use the Windows portable-device API [V Wikipedia, mtp-rs docs.rs page: it uses the native WPD COM backend; the driver "can stall calls for minutes after files change on the phone, so wrap calls in a timeout"].
- macOS: no native MTP (PTP only, through Image Capture) [V Wikipedia]. The system daemon ptpcamerad claims MTP/PTP devices when plugged in and blocks other apps; Android File Transfer can also grab it; sandboxed App Store apps cannot kill the daemon [V docs.rs mtp-rs]. Amazon, Supernote and Boox all tell Mac users to install a helper app [V].
- Linux: libmtp, GVfs, KDE KIO, FUSE tools [V Wikipedia]; user may need a udev rule for non-root access [V docs.rs mtp-rs]. libmtp README: other software (gvfs, gphoto2) may claim the device first [V].

Practical rule for the app: a device that was mass storage can become MTP in a later model of the same brand (Kindle 2024, Tolino 2023+). The device layer must be a trait with two backends (mounted folder, MTP object store), chosen per device at connect time, not per brand.

### 1.2 MTP versus a mounted drive

[V] libmtp README (github.com/libmtp/libmtp), Wikipedia, Android MtpDevice reference:
- Object model: files and folders are "objects" with 32-bit handles, listed by parent; duplicate names are allowed; moving may need re-upload (libmtp). Android's `getObjectHandles(storageId, format, parentHandle)` lists one folder at a time, then `getObjectInfo` per object, which is why large listings are slow; mtp-rs says recursive listing is slower because it walks manually.
- Writes: the file size must be known before sending (transactional protocol; libmtp). No random-access write; changing a file in place means copying it off and re-sending (Wikipedia). Android adds partial reads (`getPartialObject`, 64-bit offset is a vendor extension that the device may not support).
- Interrupted transfers are less likely to corrupt the device file system, because the device owns its file system [V Wikipedia].
- One session per device: mtp-rs "can't open the same device twice"; a long download holds the whole session, so other calls wait [V].
- Limits seen in the Rust crate: file names up to 254 characters, deleting a non-empty folder fails (delete contents first), cancelled uploads can leave a partial object [V docs.rs mtp-rs].
- Device quirks: many devices deviate from the spec; some Android phones show no files while the screen is locked; some devices fail when the session closes; certificate queries can crash devices [V libmtp README].
- Calls "may block for an arbitrary amount of time" and `deleteObject` can block for a long time on large folders [V Android MtpDevice reference].

Libraries and licences:
- libmtp: LGPL-2.1, C, built on libusb [V GitHub page]. Linking it dynamically from Apache-2.0 code is usually workable; static linking, or shipping it inside an iOS-style closed bundle, raises LGPL obligations. [U] legal advice needed.
- mtp-rs (Rust, built on nusb, no C dependencies): "MIT OR Apache-2.0", version 0.37.1 seen, pre-1.0 so minor versions can break [V docs.rs]. Windows uses WPD; macOS and Linux use direct USB. This fits an Apache-2.0 Rust core. Maturity and maintainer count unchecked [U].
- Android has its own android.mtp host classes (see 1.9). The MTP spec v1.1 (April 2011) is distributed by the USB-IF under an adopters agreement [V Wikipedia]; do not assume it is freely reusable.

### 1.3 Recognising a device and telling two units apart
- [V] Android's USB API exposes vendor ID, product ID, class/subclass/protocol, and lets a manifest filter on them; `MtpDevice.getDeviceInfo()` returns a device-info object (fields not read). Android `UsbDevice` ids are per connection, not stable [U].
- Stable IDs are [U] (not verified on pages I opened): USB serial string; on Kobo a version file in the hidden .kobo folder carrying a serial; on Kindle a version file in the system folder; KOReader and others keep no stable ID. Design: store several identifiers per device (USB vendor/product, USB serial if non-empty, MTP serial from device info, a marker file the app writes itself on first connect). The marker file is the only identifier the app fully controls and survives firmware changes, but a factory reset or copy of the drive duplicates or removes it.
- Same-model units commonly report the same vendor/product ID; some e-readers report an empty or all-zero serial [U].

### 1.4 Where sideloaded books go, subfolders, name rules, library build
- Kindle: Documents folder [V Amazon]. On Windows the full path should not exceed 256 characters [V Amazon]. Subfolders: [U] older Kindles list subfolder content; on current firmware it is flattened [U].
- Kobo: any non-hidden place in the drive; PDF and EPUB documented, DRM files not [V]. [S] forum: files in a dot-folder such as .kobo are not scanned; the device imports after eject ("Importing content" [V]); import can stall around 6% and reboot, leaving new books missing [S]; kepub files need the double extension .kepub.epub to be treated as kepub [S]. Content appears only on the device used for import [V].
- Nook: "My Files" folder; EPUB and PDF [V]. Sideload cap about 5 GB on 6-inch GlowLight 4/4e [S]; sideloaded EPUBs can sync position with Nook apps [S, the-ebook-reader 2024-06-20, not opened].
- Tolino: "Books" folder; PDF, EPUB, TXT, MOBI, CBZ, CBR [V]. Do not unplug mid-transfer [V].
- KOReader (runs on the above, and Android and others): reads the folder tree itself, no import step; metadata lives in per-book sidecar folders [V].
- FAT32 rules [V Wikipedia Design of FAT]: file max 4 GiB minus 1 byte; long names up to 255 UCS-2 code units; modification time has 2-second resolution, so "same mtime" comparisons need tolerance. Forbidden-character and case rules not read there [U]; assume a conservative portable set (no / \ : * ? " < > |, no trailing dot or space).
- Name and path budget: pick limits of 255 characters per name and about 240 for a full Windows-style path to be safe; MTP crate caps at 254.

### 1.5 Covers and thumbnails for sideloaded books
- Kindle: the cover comes from one of two places, depending on how the book is typed. [S] If typed as an Amazon book (EBOK) the device looks in its thumbnail folder (and may ask Amazon by ASIN); if typed as a personal document (PDOC) the cover is read from inside the file; firmware behaviour changed over versions and users disagree on which works. [V] the-ebook-reader 2020 post: the cover gets replaced by a generic image if the book lacks a valid ASIN matching Amazon's servers (disputed by commenters); commenters report a fix needs firmware 5.12.5+ and some covers vanish again. Treat as a firmware-dependent failure, not a rule.
- Kobo: [S] cover missing usually means the book's package file does not link the cover image correctly; kepubify docs list accepted cover references. The device builds its own thumbnails on import [U].
- Boox, PocketBook, Tolino, Nook: not found. [U]
- MTP gives a thumbnail per object, JPEG typically [V Android reference], but that is a host-side preview, not what the e-reader shows.
- The app should therefore make the book file itself carry a correct, standards-linked cover and treat device-side thumbnail folders as optional per-family extras.

### 1.6 Collections, shelves, series from outside
- Kobo: shelves exist in the device database as a table linking books to shelf names [S]; Kobo's help says collections can be made on the device and in the app and sync between them, but does not say whether sideloaded books can join [V]. Writing to the device database is risky (see 1.7).
- KOReader: collections are personal lists on the device, history and favourites are local [V].
- Kindle, PocketBook, Boox, Nook, Tolino, reMarkable, Supernote: not found. Folders are the only portable grouping; reMarkable supports folders, Supernote has its own folders [U].
- Series in file metadata: whether device UIs read series tags from the file was not verified [U].

### 1.7 On-device records worth importing
Vendor-documented: Kobo notebook exports (see below); Kindle Clippings text file (documented by Amazon in a help article I did not open; format known from parsers [S]); KOReader's guide for sidecars, sync and export [V].

- Kindle: one text file "My Clippings.txt" in the Documents folder holds highlights, notes and bookmarks for every book including sideloaded ones; entries are separated by a row of equals signs; bookmarks carry a "Your Bookmark" label; labels are localised, so parsers need per-language tables [S, several parser READMEs]. Per-book .sdr sidecar folders: contents [U], not found.
- Kobo: SQLite file at .kobo/KoboReader.sqlite holding book metadata, reading progress and annotations for store and synced books [V ogkevin.github.io kobo.koplugin docs]; shelf and shelf-content tables [S]; analytics events table [S]; exact columns for percent and time not confirmed. Community description only, no vendor schema; schema changes with firmware [U but follows from "no vendor docs"]. Stylus notebooks (Sage, Elipsa, Elipsa 2E, Libra Colour): synced to Kobo.com; export formats Word, Text, HTML, ZIP (advanced) or PNG, JPEG, PDF, ZIP (basic); USB export goes to an "Exported Annotations" folder; .nebo files are the notebook files; converted text cannot be imported back; sign-out or reset can delete notebooks [V help.kobo.com 360062226733].
- KOReader: each book gets "<book>.sdr" holding a Lua file with settings, progress, highlights and notes; custom metadata edits are saved there and the book file is not changed. Location setting "Book metadata location": book folder (default on new installs), ./docsettings, or ./hashdocsettings (named by content hash). Changing the file changes the hash and can orphan hash-based metadata [V koreader.rocks user guide]. Statistics plugin database: community says koreader/settings/statistics.sqlite3 with tables book and page_stat_data, time in Unix seconds; not updated on every page turn, so can lag [S MobileRead summary; no official schema found; the KOReader wiki page I tried was empty].
- Export formats from KOReader: plain text, HTML, Markdown, Kindle-clippings-style text, JSON; services Joplin, Readwise, Memos, Flomo, XMNote [V].
- PocketBook: not found beyond cloud sync of position through its app [V pocketbook.ch].
- Boox: notes export as PDF or PNG from the Notes app; move between devices by backup and restore [S].
- reMarkable: .rmdoc archive via USB web interface (v3.9+), PDF download [V].
- Supernote: INBOX folder receives files from the phone app [V]; other folders [U].

### 1.8 Wireless routes
- KOReader [V user guide]: Progress sync plugin. Server stores only username, a document ID (hash) and the furthest reading position; password kept as MD5 hash; HTTPS; sync on close or suspend or manually; two match modes: binary (default; partial MD5 of file contents, so identical files needed) or filename (tolerates edited copies; recommended if you save PDF highlights into the file). Self-hosting possible; Calibre-Web and Kavita bundle sync servers. OPDS catalog browser preconfigured with Project Gutenberg and Standard Ebooks. Cloud storage: Dropbox, FTP, WebDAV. Calibre wireless plugin (feature name only). SSH/SFTP server plugin on Kobo, Kindle, Cervantes (warns about passwordless access). HTTP Inspector: experimental, no HTTPS or authentication, trusted networks only.
- Kobo: Dropbox for Forma, Sage, Elipsa, Elipsa 2E, Libra Colour; Clara Colour and Clara BW with active Kobo Stack; software 4.18+; code from Settings > Accounts, enter at kobo.com/dropbox; files in Apps/Rakuten Kobo; needs Wi-Fi sync; DRM files need Adobe Digital Editions; free Dropbox allows 3 linked devices [V via search listing, summary of help.kobo.com 360033830114, article not opened]. Sideloaded Mode (firmware 4.31, signed out): library list, no sync, no store link [V the-ebook-reader 2022]. Kobo Stack upload to Kobo Cloud exists [S title].
- Send to Kindle: PDF, EPUB, DOC/DOCX, RTF, TXT, HTML, images; wireless limit 200 MB per file; app limit 50 MB mobile [V Amazon mirror page]; email: under 50 MB per attachment, up to 25 files, approved sender list required [S, Amazon help titles]. Mobi/AZW older lists [S].
- PocketBook: Cloud (web upload, app, position sync) and Send-to-PocketBook email to a per-device address; no size limit stated [V pocketbook.ch]; the 2013 PDF was unreadable.
- Boox: BOOXDrop (Wi-Fi, "recommended"), send2boox web/desktop, phone app, Dropbox and other cloud, Bluetooth, Nearby Share [V the-ebook-reader 2023].
- reMarkable: email send (PDF, PNG, SVG; Wi-Fi and account required; business addresses may need @remarkable.com allowed) [V]; 25 MB limit claim [S].
- Supernote: Supernote Cloud, Dropbox, Google Drive, email, USB, Browse & Access (same Wi-Fi, URL in browser, upload/download), Supernote Linking from phone app into INBOX [V].
- Tolino: tolino cloud recommended over USB [V].
- Nook: sideloaded EPUB position sync with Nook apps [S].

### 1.9 Phone-side developer rules
Android [V developer.android.com pages unless noted]:
- USB host: UsbManager enumerates devices; filter by vendor-id, product-id, class, subclass, protocol; user permission required each time unless the intent filter launched the app, in which case permission lasts until unplug; do transfers on a worker thread. `android.hardware.usb.host` feature must be declared.
- android.mtp: MtpDevice wraps a UsbDevice; open(UsbDeviceConnection), getStorageIds, getObjectHandles, getObjectInfo, importFile, sendObjectInfo then sendObject (API 24+), deleteObject, readEvent, getPartialObject; the package doc says it uses the PTP subset of MTP. Usable from an ordinary app.
- Storage: scoped storage from API 29; Storage Access Framework needs no permission and goes through the system picker; MANAGE_EXTERNAL_STORAGE exists but most apps should not use it.
- Long transfers: foreground service types are required from Android 14; `connectedDevice` fits USB if the app has called UsbManager.requestPermission(); `dataSync` has no prerequisite but Android 15 adds limits and a migration guide (not read); Android recommends companion device manager for continuous transfers; shortService lasts about 3 minutes. Declare service types in Play Console.
iOS [U, Apple pages returned only titles; from general knowledge]: document picker (open in place with security-scoped URLs, or copy); Files shows external USB drives; no MTP or raw USB class access for ordinary apps; local network permission prompt with NSLocalNetworkUsageDescription and NSBonjourServices keys; no unlimited background transfer. A phone app can therefore reach e-readers only by Wi-Fi or by files on a USB drive, not by cable to the e-reader as MTP. Confirm before design.

### 1.10 Failure cases seen
- Unsafe eject: Kobo, Nook and Tolino all say eject first [V]. Mass-storage FAT volumes can be left dirty [U]; MTP is less exposed [V].
- Import never finishing or rebooting (Kobo) [S]; covers vanish on Kindle after firmware changes [V]; books reported deleted by a Kindle after Wi-Fi stayed off for a while, for USB-sideloaded books (one commenter, unverified claim) [V as a report only].
- Device in the wrong USB mode (Boox PTP only; Kobo "Connect" tap missing) [V].
- Logging out of a Kobo to rebuild the library loses unsynced positions and annotations on sideloaded books [S].
- KOReader sidecars orphaned by moving files outside the app, or by changing files that alter the hash; sync matching broken by editing a book [V].
- Windows MTP stalls; macOS daemon grabs device [V].
- Full device, duplicate copies, two formats of one book, device-side renames: no page opened; follow from the design below [U].

## 2. Data the app must store
- Device record: id, user label, family, model, USB vendor/product, USB serial, MTP serial, app marker-file id, firmware string (store when seen), connection kind (folder, MTP, wireless), last seen, storage free/total, supported formats, folder rules (root, name template, allow subfolders, max name and path length).
- Device book record (what the app believes is on the device): device id, library book id, format and exact file hash, device path or MTP object handle (handles are not stable across sessions), size, device mtime (2-second resolution), cover state, date sent, status (sent, confirmed on next connect, missing, changed on device, renamed on device).
- Device-origin event records, kept separate from the library's own reading data: source (Kindle clippings, Kobo database, KOReader sidecar, KOReader statistics), source firmware, raw identifier, position (percent, location, CFI-like text), timestamps, kind (highlight, note, bookmark, session, finished), the quoted text and the user note, source file hash at import, import id so re-import does not duplicate.
- Raw import snapshots (copy of clippings file, database, sidecar) with hash, so a parser can be re-run after a bug or a firmware change.
- Sync account records: KOReader sync server URL, matching mode (binary or filename), per-book document hash computed the same way (partial hash), furthest-position value and its source device.
- Per-book "sendable variants": the format list, whether it was converted for this family, kepub marker, cover reference ok flag.
- Per-device conversion and naming preference, plus the portable-name sanitiser result (original title and sent filename).

## 3. Requirements existing products get wrong
- Treat MTP as a first-class transport: show "connecting, listing is slow", progress, cancellation, and one-client contention errors instead of a blank list.
- Never write into a vendor database by default; read a copy. Show the firmware and schema version before trusting parsed data; refuse or warn on unknown versions.
- Keep device annotations and positions separate from the library's own, import them idempotently, and keep the quoted text and the source label.
- Confirm after eject: rescan the device and mark each sent book as confirmed or missing, instead of assuming success.
- Detect two copies (two formats, renamed file, hash-matched file in another folder) and show them rather than send again.
- Keep the file untouched where possible (hash-based sync breaks if the file is edited after sending; record when a file changed).
- Check free space and the family's name, path and size limits before a batch, not mid-batch.
- Warn on unsafe eject and macOS daemon capture with a plain fix.
- Offer a dry run showing exactly which files, names and folders will be written.

## 4. Open questions
- Exact USB mode (MTP versus mass storage) of each 2024-2026 Kindle, Kobo, PocketBook and Boox model; vendor pages do not state it.
- USB vendor/product IDs and which serial or ID files exist per family: not verified.
- Kindle .sdr contents, whether Kindle honours subfolders, and Amazon's own clippings documentation.
- Kobo database columns for percent, time spent, read status, and shelf writes; firmware change history.
- PocketBook and Boox on-device record formats and folder layouts.
- Apple developer documentation was not readable through the tool; iOS capabilities above are from memory.
- Licence of the Android and macOS driver layers when shipped with a Rust core (LGPL linking if libmtp).
- Android Play policy for foreground service type `connectedDevice` versus `dataSync` for long USB transfers.

## 5. Source URLs opened
developer.android.com/develop/connectivity/usb/host
developer.android.com/reference/android/mtp/package-summary
developer.android.com/reference/android/mtp/MtpDevice
developer.android.com/training/data-storage
developer.android.com/develop/background-work/services/fgs/service-types
koreader.rocks/user_guide/ (3 offsets, plus 2 other offset reads)
digprjsurvey.amazon.com/csad/help/node/TCUBEdEkbIhK07ysFu
help.kobo.com/hc/en-us/articles/360024775093
help.kobo.com/hc/en-us/articles/360033887953-Organize-your-eBooks-on-your-Kobo-eReader
help.kobo.com/hc/en-us/articles/360062226733-Use-your-Kobo-eReader-as-a-notebook
blog.the-ebook-reader.com (Kobo sideloaded mode 2022; Kindle covers 2020; Kindle sideloading 2025; Boox 9 ways 2023)
github.com/libmtp/libmtp
docs.rs/crate/mtp-rs/0.37.1
en.wikipedia.org/wiki/Media_Transfer_Protocol
en.wikipedia.org/wiki/Design_of_the_FAT_file_system
remarkable.guide/tech/usb-web-interface.html
support.remarkable.com/articles/Knowledge/Send-by-email
support.supernote.com/transfer-files
pocketbook.ch/en-ch/news/unlock-the-full-potential-of-your-pocketbook-ch
mytolino.com/faq/how-can-i-import-books-ive-already-purchased-or-upload-my-own-content-onto-the-tolino-ereader-side-loading/
help.barnesandnoble.com/hc/en-us/articles/5599565083547
ogkevin.github.io/kobo.koplugin/dev/database/kobo/index.html
chrsolv.substack.com/p/i-turned-my-kobo-reading-data-into
Returned titles only or empty: developer.apple.com (document picker, TN3179), support.apple.com iPhone guide, github KOReader wiki, sync.pocketbook-int.com PDF (unreadable).
Failed: mobileread.fandom.com (402), wiki.mobileread.com Kobo_Reader (404) and Kindle (disambiguation only), amazon.com help pages (503), support.boox.com and help.boox.com (404/empty), support.remarkable.com importing page (404).
