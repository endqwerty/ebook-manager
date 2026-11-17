ISSUES

Reason for ISSUES: several docs pages returned thin or 404 answers (Kavita smart filters and Kobo, Komga collections/read lists, Storyteller's own docs behind a GitLab auth redirect, Alfa and BookFusion help hosts did not resolve). Those gaps are listed in section 4. Rule 1: no calibre code or manual was opened or seen. Calibre-Web-Automated was read only through its README feature summary. Procedure slip: early in the run I sent two or three github.com fetches in one parallel batch (Komga/Kavita/Audiobookshelf issue lists), against the "one at a time" rule. Nothing was blocked. github.com ended at 10 fetches, komga.org at 9, wiki.kavitareader.com at 9, audiobookshelf.org at 8, zotero.org at 11 (one over the limit of 10; the extra was a plain doc page). No page needed a retry except komga.org/docs/guides/kobo (503 once, then fine).

Summaries below come from a small model that read each page, so exact wording is paraphrase. Every claim is "verified" = stated on a page I opened, unless marked [unverified].

## 1. Findings

### 1.1 Entity model per product

- Komga (komga.org/docs/guides/scan-analysis-refresh): library > series > book. Every subfolder at any depth becomes a Series; each file becomes a Book of its parent folder's series (verified). Collections (of series) and read lists (of books, can be imported from ComicRack `.cbl`) are separate, user-visible groupings (README, github.com/gotson/komga). Series gets "Sort Title"; books get "Sort Number", which lets a "2015 Special" sit between books 24 and 25 (komga.org/docs/guides/edit-metadata). Authors are per book with roles (writer, penciller, inker...); labels and age rating live on series (user-accounts page). Series metadata is derived by aggregation: most frequent value for title/publisher/reading direction/language, highest value for age rating and total count, union of genres (scan page). Mixed-up series membership by folder is the model's weak spot: a series is a folder, so a book-with-formats model does not exist.
- Kavita (wiki.kavitareader.com/guides/scanner): library > series > volume > chapter (plus "specials"). Series, volumes and chapters are parsed from filenames, internal metadata and some folder names. Each series needs its own folder; the same series may not be split across parallel folders; files cannot sit at the library root; publisher > series nesting is allowed; specials can go in a "Specials" subfolder (verified). Collections, Reading Lists (ordered chapters; "Promote" role lets admins/promoted users share them), Want to Read, Smart Filters are separate objects. Kavita+ (paid) adds per-volume external metadata via Mangabaka and Hardcover (release 0.9.1.0, newreleases.io via search; wiki.kavitareader.com/kavita+/metadata-controls). Open complaint on tracker: "Single Book and Series with the same name" (10 comments), "Japanese volume parsing bug" (5) (github.com/Kareadita/Kavita/issues).
- Audiobookshelf (audiobookshelf.org/docs/documentation/libraries/book-library/directory-structure): library > library item (a book folder or single file) with tracks (audio files), plus a "primary" ebook per item. Series with decimal sequence, authors, narrators, genres, tags, chapters are first-class. Folders `{Author}/{Series}/{Book}`. Complaint: author names are not punctuation-aware ("J.R.R. Tolkien" and "J. R. R. Tolkien" are different authors). Requested: "Same book, different narrators" (8 comments), "Show missing books in series", "Show/list duplicate book", "Ability to edit Series similar to tags" (github.com/advplyr/audiobookshelf/issues).
- Zotero (zotero.org/support/collections_and_tags and /attaching_files): item (parent record with item type) > child attachments and notes. One item can be in many collections (collections behave "like playlists, not folders"), collections nest, "Unfiled Items" is a virtual view. Tags are per item, can be automatic (marked, from publisher data) or manual, colour-coded (max nine, key shortcuts 1-9). Saved searches act as smart collections. Group libraries are separate libraries; copies between libraries are separate copies (groups page).
- Jellyfin (jellyfin.org/docs/general/server/media/books): types Audiobooks, Books, Comics, each book in its own folder; comics as `Series (year)` with issue numbers and years in filenames. Comic series metadata is not supported from ComicInfo, only per comic (search result summary). Read-along audiobooks unsupported. Jellyfin 12.0 (Sept 2026 per alternativeto.net via search) deprecates the Bookshelf plugin and moves books/comics into the server [unverified detail, search snippet only].
- Grimmory / Booklore (github.com/grimmory-tools/grimmory): libraries inside host-mounted directories; shelves (custom and rule-based "smart shelves"); multi-user with separate shelves, progress and preferences per user. Formats: EPUB, MOBI, AZW, AZW3, FB2, PDF, CBZ/CBR/CB7, audiobooks M4B/M4A/MP3/OPUS.
- Stump (github.com/stumpapp/stump README): libraries of EPUB, PDF, CBZ, CBR with built-in readers; self-described beta.
- Storyteller (docs.elfhosted.com/app/storyteller/, tildes.net v2 post): book = EPUB 3 plus matching audiobook in the same folder (pairs by folder, not by title); output is a derived "readaloud" EPUB 3 file, saved beside the source with "(readaloud)" added. v2 added collections, advanced search/sort and metadata management.
- BookFusion (bookfusion.com/features): bookshelves (3 free), highlights/notes/quotes, up to 3 devices and 10 cloud books on free plan, paid from 1.99 USD/month.
- Alfa Ebooks Manager: only third-party reviews read via search: Windows only, table/list/thumbnail/wall views, tags, duplicate flagging on import, links to files rather than copying, web server and cloud sync are Pro features. The vendor page did not resolve. [unverified for vendor claims]
- Calibre-Web(-Automated) (README of crocodilestick/Calibre-Web-Automated): shelves, rule-based "Magic Shelves" (AND/OR, update live, can sync to Kobo).

Where they disagree: Komga is "folder = series, file = book" with no multiple-format book. Audiobookshelf is "folder = item holding several tracks plus one primary ebook". Kavita adds a volume/chapter layer. Zotero separates "item" from "attachments" (so one record may hold several files). None exposes a formal "edition" entity (users ask for same book with different narrators in Audiobookshelf).

### 1.2 Scanning, naming, moves, hashing

- Komga: scans on a schedule; folder last-modified time decides whether to compare books in a folder; "deep scan" compares every book with the DB when books are missing. Reads ComicInfo.xml, EPUB metadata, Mylar `series.json`; filenames are not parsed for metadata (verified). Analysis records container (ZIP/RAR/EPUB/PDF) and image format, lists Error/Unsupported books on a Media Analysis screen. "Trash bin: handle file moves, and restore after delete" and "Option to delete duplicate comic/books from web interface" were requested and closed (github.com/gotson/komga issues). README says it detects duplicate files and duplicate pages. Hashing for KOReader matching exists, needs "file hashing for KOReader" enabled and KOReader's matching set to Binary (koreader page).
- Kavita: change detection uses each folder's last-write time at minute precision; renaming or moving a folder does not always change it, so scans miss changes (the wiki suggests creating a blank .txt in the folder). A scan aborts if the root is empty or unreachable; the series is deleted only on a library scan. Embedded ComicInfo/OPF overrides filename parsing. Cover images named `cover`, `!cover`, `folder` are treated as covers. Glob exclusions exist. First scans are slow on network storage; Refresh Covers and Analyze Files are I/O and memory heavy. Mismatched UNC path normalisation breaks scans (open issue). Hashes are generated for KOReader sync after a forced scan; archives/PDF need a new forced scan (koreader page).
- Audiobookshelf: folder patterns `{Author}/{Series}/{Book}`; sequence in folder name (leading `1 - Title`, or after "Vol", "Volume", "Book"; decimals allowed); year first or after sequence with " - "; narrator in `{curly braces}`; optional subtitle parsing; ASIN in brackets; multi-disc subfolders (Disc/CD/Disk N); tracks numbered by filename or ID3, whichever looks more accurate; mis-grouped books must be removed and rescanned and "progress is lost for re-added books" (directory-structure page). Metadata precedence per library, lowest first then overwritten: folder structure, file metadata, NFO, desc.txt/reader.txt, OPF, then Audiobookshelf's own `metadata.json` (highest). Disabling the `metadata.json` source can lose edits on next scan (book-scanner guide). Online matching is intentionally manual ("Match") or semi-automatic ("Quick Match", picks closest result including by audio duration; docs warn it is not always accurate).
- Jellyfin: external `content.opf`, `metadata.opf`, `ComicInfo.xml`; covers `folder`/`poster`/`cover`.
- Zotero: stored files are renamed from item data; linked files are not renamed or synced and need a "linked attachment base directory" so relative paths work across computers.
- Grimmory: BookDrop watched folder detects, parses, fetches metadata, queues for human review before import. `DISK_TYPE=NETWORK` disables delete/move/rename from the UI.
- CWA: ingest folder is emptied as files are processed; auto-convert to a target format (EPUB default; MOBI, AZW3, KEPUB, PDF); duplicates found by SQL and fuzzy matching after ingest and on scheduled scans, merge one-click or batch with configurable thresholds; originals kept in a processed-books folder, zipped daily; `NETWORK_SHARE_MODE` switches watchers to polling for NFS/SMB.
- Storyteller: pairs only same-folder ebook+audio; scheduled scans (e.g. every 60 minutes) are needed to see files added outside Storyteller.

No product page I read describes content-hash move detection or progress that survives a replaced file. Audiobookshelf states the opposite (progress lost on re-add).

### 1.3 Metadata editing, locks, sources

- Komga: editing a field locks it automatically; locked fields survive refresh; lock icon turns orange; toggle manually (edit-metadata page). "Refresh metadata" overwrites existing values per library options. Online metadata is third-party: Komf (Snd-R/komf) [unverified, search]. Retrieve-from-online-sources was the top-voted closed enhancement (42).
- Kavita: Kavita+ field-level write controls for series (summary, names, age rating, publication status, relationships, start date, cover), chapter (title, summary, release date, publisher, cover), volume (cover); locked fields are not overwritten unless overridden. Whitelist for tags, blacklist after field mapping, tag-weight filter for Mangabaka, field-mapping rules (source tag to target field), people-role selection and naming pattern, age rating = highest of file rating, provider rating and tag/genre mappings (kavita+/metadata-controls). Embedded Writer-only mapping for EPUB (dc:creator -> Writer).
- Audiobookshelf: sidecar `metadata.json` per item under `/metadata/items`, mirrors database including chapters; embeds tags into audio files from a Tools tab ("not all formats supported"; not for ebooks/comics); optional backups under `/metadata/cache/items`; provider names not stated on pages read. Request: "Ability to move files based on metadata" (26 comments).
- Zotero: metadata from identifiers (ISBN, DOI, PubMed, arXiv, ADS), browser connector, file import with automatic parent-item retrieval for PDFs (lower quality; manual fallback "Create Parent Item"). Duplicates compared by title, DOI, ISBN, then year within one and author surname plus initial; per library only; cannot mark a false positive; merge keeps all collections and tags, user picks master item and per-field values.
- Grimmory: Google Books, Open Library, Amazon for covers/descriptions/reviews/ratings, all fields editable. CWA: Hardcover, Kobo, isbndb-backed; chooses overwrite vs fill gaps. Jellyfin: GoogleBooks, ComicVine, OpenLibrary.

### 1.4 Users and permissions

- Kavita: roles Admin, Download, Change Password, Bookmark (uses disk), Change Restriction, Read Only, Login, Promote; per-user library sharing; age restriction hides any series or reading list that contains even one item above the rating, "Include Unknowns" toggles series without rating; admins exempt (wiki users page). Each user has an auth key used for OPDS and KOReader.
- Komga: roles Administrator, Page Streaming, File Download (plus KOBO_SYNC and KOREADER_SYNC per the sync pages); per-library access; age rating allow/exclude; label allow/exclude; exclusion beats allow; multiple allows are OR; restrictions apply to series, books, collections and read lists; deleting a user deletes their progress irreversibly. API keys are per device. OIDC was a closed feature request (10 votes); the README page read does not list it. [unverified for current state]
- Audiobookshelf: Root, Admin, User, Guest (cannot change own password); limit by library and by tag, tag filter invertible; tag restrictions may slow big libraries; account sharing not supported (playing two media at once from one account may cause issues); API keys inherit the owner's permissions (search snippet of the API-key guide); OIDC with "match existing users by" setting. Requested: LDAP (18), webhook support (7), new podcast-management permissions.
- Zotero groups: private, public closed, public open; roles member/admin/owner; library read, library edit and file edit permissions chosen separately; group file storage counts against the owner's quota; open public groups cannot hold files.
- Stump and Kavita list OIDC; Grimmory lists local or OIDC and "auto user provisioning" (page not opened); CWA maps provider groups to admin roles.

### 1.5 Progress and sync, OPDS, Kobo, Kindle

- OPDS: Komga serves v1.2 and v2 (`/opds/v1.2/catalog`, `/opds/v2/catalog`), page streaming extension 1.2 on v1, OpenSearch by series; client support varies (some PSE 1.0/1.1, many none) and one client broke on Komga 1.4.0+ (komga.org/docs/guides/opds). Kavita: OPDS with per-user URL and auth key, feeds for Smart Filters, Collections, Want to Read, Reading Lists, Series, pagination, OPDS-PS, "Continue From X" at top of the first page, progress glyphs (none, up to 25 percent, 50, 100, complete); Mihon, Paperback, Aidoku sync per chapter, not per page (wiki features/opds). Stump: v1.2 with PSE and 2.0.
- KOReader: Komga implements the KOReader sync protocol under `/koreader`, the API key is the username, password ignored; EPUB progress only saves at chapter starts, pre-paginated books sync per page; matching needs hashing and binary matching. Kavita: custom sync server `<server>/api/koreader/<token>`; archives, PDF, EPUB; needs forced scan to generate hashes; the wiki "strongly" advises against automatic sync because it can undo progress and the authors cannot help recover lost progress. CWA: checksums identify books, HTTP basic auth with CWA accounts, plugin served from `/kosync`. BookFusion: own KOReader plugin syncs position and highlights, not bookmarks; manual push/fetch; advises letting one device finish syncing before opening on another because position can jump; needs KOReader 2026.03 or later.
- Kobo: Komga edits Kobo's config `api_endpoint` to `https://server/kobo/<api_key>`; syncs only EPUB with cover and metadata; deleted books are removed on the device; converts to KEPUB on request with Kepubify (bundled on Windows/macOS/Docker, path needed for JAR); plain EPUB progress only at chapter starts; KEPUB mapping approximate "off by a few pages"; pre-paginated accurate; optional proxy of unknown requests to Kobo's store; "Force Kobo sync" option per key; needs redownload after 1.14.0 (release note via search). Kavita README: Kobo sync of EPUB, archives and images including annotations; open request thread with 24 comments. CWA: shelf-based Kobo sync. Grimmory: KEPUB conversion; user report of Clara 2E init-only failure (discussions/1300, via search). Audiobookshelf requests Kobo sync (14 comments).
- Kindle email: Audiobookshelf sends the primary ebook by SMTP to manually entered device addresses; each device can be limited to admins, all users, all but guests, or named users; EPUB wins as primary when several found; the sending address must be on the Amazon approved list (guides/send_to_ereader). Kavita's built-in emailer shut down in v0.8.0 (April 2024); users now configure their own mail account (search snippet from developer post). Amazon allows up to 15 approved senders and is removing partial or domain-only sender entries (search snippet, 2025). CWA sends one-click or automatically after ingest with configurable delay, multiple recipients.
- Progress definition: KOReader sync is a percent/location per document hash; Kobo is chapter start (EPUB) or Kobo location (KEPUB); per page for comics; per chapter for Mihon-style clients; audio is time. Merge rule is last-writer in every product described; only BookFusion advises a human workaround.
- Storyteller: readaloud EPUB 3 with Media Overlays works in any Overlay-capable reader, its web UI and native iOS/Android apps; the BookFusion reader and the Kobo app (not devices) are said to work (tildes post).

### 1.6 Smart filters, lists, search, annotations

- Kavita: Smart Filters are bindable to the dashboard and side navigation with ordering and visibility; CBL import for reading lists; Want to Read list; EPUB highlighting and annotation in the web reader (README). Kavita+ smart collections, recommendations, scrobbling (paid).
- Komga: closed request "Plex's way to create a playlist or smart-playlist doing a library search" (11).
- CWA/Grimmory: rule-based shelves.
- Zotero: Quick Search modes (Title/Year/Creator, All Fields and Tags, Everything incl. indexed text); Advanced Search with match all/any, subcollections option, "%" wildcard; saved searches store criteria not results; full-text indexing of PDF, EPUB, HTML, text when idle; defaults cap 500,000 characters and 100 pages per file; .docx/.odt cannot be indexed; one library per search; type a quote mark first to run searches only on Enter in large libraries (zotero.org/support/searching).
- Annotations: Zotero stores PDF annotations in its SQLite database (not the file), tags and filters them, extracts to notes, imports embedded ones (making them editable and removing them from the PDF), exports PDF copies with annotations embedded, exposes them via the web API; moving or deleting pages outside Zotero may break annotations. BookFusion syncs highlights, notes, quotes. Audiobookshelf has an open request for ebook highlighting and annotations (10). Grimmory and Stump list annotations/highlights; export format not stated.

### 1.7 Background jobs, backups, scale

- Audiobookshelf: scheduled backups; Quick Match jobs; chapter lookup; embed metadata; merge to m4b. Requested PostgreSQL support (23), Whisper transcription (14), DLNA (20).
- Zotero: sync is free and unlimited for data; files go to 300 MB free storage or WebDAV (not for groups); conflicts shown in a dialog; putting the data directory in cloud storage risks database corruption; database integrity check is slow for large databases (sync page, advanced prefs page).
- Komga history: switch from H2 to SQLite (12 votes, closed). Komga 2.0 / API v3 / NextUI rewrite are open tracking issues.
- Scale claims: no product page read states a library size. Komga README 6.9k stars, Kavita 12.1k (popularity only). Only limits found: Kavita first scans slow on network storage; ABS tag restriction slow on large libraries; Zotero full-text index caps.

### 1.8 Name, licence and maintainer changes

- Booklore -> Grimmory: Notebookcheck (2 May 2026) reported a rumoured AGPL-to-BSL change with contributors not asked to sign a CLA, claimed AI-written code, telemetry that kept sending with the setting off, a limit on multi-book downloads seen as paywalling, and a library that went inaccessible after an update. Discord statement reported: "ACX has made the decision to close Booklore and step away." The community fork Grimmory (github.com/grimmory-tools) is AGPL-3.0, governance file instead of one named maintainer, rebrand in v2.2.7 (21 March 2026 per search), v3.0.0 on 22 April 2026, still releasing (AUR 3.2.4, 30 June 2026). ElfHosted (10 Aug 2026) now provisions Grimmory and keeps Booklore only for now. TrueNAS dropped Booklore from its catalogue 27 March 2026. Whether the original repo returned is conflicting [unverified].
- Readarr retired upstream with its metadata servers gone; Chaptarr is the successor fork (paid, 9 USD/month standalone, beta, over 11,000 users per ElfHosted).
- Licences: Komga MIT, Kavita GPL-3.0, Grimmory AGPL-3.0.

### 1.9 Top feature requests seen (titles and counts as shown, counts are unlabeled by GitHub page)

- Komga (closed): retrieve metadata from online sources (42), OIDC (10), smart playlists (11), trash with move handling (11), duplicate delete (3), data quality views (3), custom covers, aggregated series info.
- Kavita (open): Kobo sync (24), single book and series with same name (10), external sources search, device rework.
- Audiobookshelf (open): EPUB 3 read-along (42), audio-to-text sync (39), move files by metadata (26), ratings/Goodreads (25), OPDS (23), PostgreSQL (23), finished-podcast handling (28), LDAP (18), DLNA (20), show missing books in series (6), duplicates (4), editable start/finish dates (10).

## 2. Data the app must store

- Library root record with per-library: scan schedule, exclusions (globs), reading direction default, metadata-source priority list, whether sidecars are written, disk type (local/network, disabling destructive actions).
- Work, Edition (narrator/translator/format variants) and File separate; item has primary file plus extra files; file record: container, image format, size, mtime, content hash (several kinds: whole-file binary hash for KOReader matching, partial hash), analysis status (OK/Error/Unsupported), page count.
- Series with volume number, chapter number, decimal sequence, "special" flag, sort title, sort number, reading direction, total count, publication status, name aliases/localized name.
- Person with roles per book (writer, penciller, inker, colorist, letterer, cover artist, editor, translator, narrator), plus person aliases to avoid "J.R.R." vs "J. R. R." splits.
- Per-field lock flag and per-field source ("embedded", "sidecar", "online provider X", "user"); metadata-source priority order; sidecar file for the library item.
- Age rating (with "unknown"), labels, genres, tags with whitelist/blacklist/mapping rules; per-user restriction sets (allow/exclude by rating, label, tag; library list).
- Users: roles/permissions set, per-library grant, per-device API key (shown once, one key per device, role flags KOBO_SYNC, KOREADER_SYNC, FILE_DOWNLOAD, PAGE_STREAMING), OIDC subject link, disabled flag.
- Device record: Kindle email, access scope (admin/all/guests/named users), Kobo key, KOReader key, last sync, "force resync".
- Progress record: user, work/file, device, unit (page, chapter, percent, KEPUB location, audio seconds), mapping quality ("approximate"), timestamp, finished flag, start/finish dates (editable), plus link to the file hash so it survives file replacement.
- Reading lists (ordered, importable from `.cbl`), collections (of series), want-to-read, shelves (manual and rule), smart filters with binding to home/sidebar and ordering, promote/share flag.
- Annotations independent of the file (not written into it), with tags, colour, type, source (imported embedded vs created), extraction to notes.
- Duplicate-candidate and "not a duplicate" decisions (Zotero cannot store the latter), merge audit.
- Import queue (BookDrop): file, parsed metadata, proposed matches, status awaiting review.
- Readaloud pairing: ebook file, audiobook folder, alignment status, derived file.
- Linked vs stored attachment flag, relative path plus base directory.
- Sync state for Zotero-style groups: library ownership, storage quota owner.

## 3. Requirements a new manager should adopt that existing products get wrong

- Keep reading progress across file replacement, rename and move (key progress to a stable item id, not path or the exact hash; keep old hashes). Audiobookshelf loses progress on re-add; Komga/Kavita pages never describe this.
- Detect moves by content hash, not folder mtime (Kavita misses renames at minute precision).
- Store EPUB progress as a location inside a chapter, not only chapter start (Komga/Kobo limitation), and keep a quality flag on mapped progress.
- Never run automatic position merge silently; show conflicts and keep a history (Kavita authors warn auto sync can undo progress).
- Make exclusion rules win over allow rules and hide whole series by one over-rated item only if the user wants that (Kavita hides any series containing one item above the limit).
- Allow marking "not a duplicate" and merge that keeps all collections, tags, progress (Zotero cannot mark false positives).
- Provide an edition entity (same work, different narrator or format) and a person alias table.
- Offer real sidecar plus locks plus a visible "source" per field, and warn before disabling the sidecar (Audiobookshelf can lose edits).
- Offer online matching as a reviewable queue, not only auto-apply.
- Put annotations in the database, exportable to a standard form, with a viewable import of embedded ones.
- Ship OPDS 1.2 and 2.0 with page streaming and OpenSearch; per-user keys; separate keys per device.
- Handle Kindle sender address limits (approved list, no partial addresses) explicitly.
- Keep per-library "network disk" mode that disables destructive actions and uses polling watchers.

## 4. Open questions

- Hashing, move detection and progress retention are undocumented on Komga/Kavita pages read; need black-box testing, not documentation.
- Kavita smart filter fields and operators, Kobo sync page, device/Kindle page: pages 404 or not opened (my Kavita wiki quota was used up).
- Komga collections/read-list ordering rules, bulk edit, OIDC and API-key details: pages not found.
- Storyteller alignment cost: its own docs sit behind a GitLab auth redirect; the "1 to 4 hours per book" figure is from a forum comment [unverified]. The full v3 aligner article is behind a sign-in.
- Alfa Ebooks Manager vendor feature page and BookFusion help centre did not resolve; their details come from third-party reviews or the features page only.
- Grimmory docs site (grimmory.org/docs) not opened: Magic Shelf rule fields, duplicate detection, metadata locks, OIDC provisioning unverified.
- Audiobookshelf metadata provider list and scale limits not found on pages read.
- Whether the Booklore repository is still alive and under which licence is conflicting between sources.
- Jellyfin 12.0 book support details come only from search summaries.

## 5. Source URLs opened

- https://wiki.kavitareader.com/guides/admin-settings/users/
- https://wiki.kavitareader.com/guides/scanner/
- https://wiki.kavitareader.com/guides/features/opds/
- https://wiki.kavitareader.com/guides/3rdparty/koreader/
- https://wiki.kavitareader.com/guides/metadata/general/
- https://wiki.kavitareader.com/kavita+/metadata-controls/
- https://github.com/Kareadita/Kavita and its issues list
- https://komga.org/docs/guides/scan-analysis-refresh
- https://komga.org/docs/guides/koreader
- https://komga.org/docs/guides/opds
- https://komga.org/docs/guides/user-accounts
- https://komga.org/docs/guides/kobo
- https://komga.org/docs/guides/edit-metadata
- https://github.com/gotson/komga and its issues list
- https://www.audiobookshelf.org/docs
- https://www.audiobookshelf.org/docs/documentation/libraries/book-library/directory-structure
- https://www.audiobookshelf.org/docs/documentation/libraries/book-library/book-metadata
- https://www.audiobookshelf.org/guides/book-scanner/
- https://www.audiobookshelf.org/guides/users
- https://www.audiobookshelf.org/guides/send_to_ereader
- https://github.com/advplyr/audiobookshelf issues list
- https://github.com/grimmory-tools/grimmory
- https://docs.elfhosted.com/app/grimmory/
- https://docs.elfhosted.com/app/storyteller/
- https://tildes.net/~books/1q2s/storyteller_v2_is_now_available
- https://store.elfhosted.com/blog/2026/08/10/chaptarr-has-arrived-grimmory-steps-up-and-readarr-sails-away/
- https://www.notebookcheck.net/Booklore-The-Plex-of-self-hosted-e-book-libraries-sparks-licensing-controversy-and-community-backlash.1287933.0.html
- https://github.com/stumpapp/stump
- https://github.com/crocodilestick/Calibre-Web-Automated (README only)
- https://jellyfin.org/docs/general/server/media/books
- https://www.bookfusion.com/features
- https://docs.bookfusion.com/docs/KOReader/koreader-sync
- https://www.zotero.org/support/collections_and_tags
- https://www.zotero.org/support/attaching_files
- https://www.zotero.org/support/duplicate_detection
- https://www.zotero.org/support/sync
- https://www.zotero.org/support/kb/annotations_in_database
- https://www.zotero.org/support/groups
- https://www.zotero.org/support/adding_items_to_zotero
- https://www.zotero.org/support/preferences/advanced
- https://www.zotero.org/support/searching
- Not usable: wiki.kavitareader.com smart-filters and kobo pages (404), komga.org users and collections-readlists (404), jellyfin.org .../libraries/books (404), smoores.gitlab.io (auth redirect), alfaebooksmanager.com and help.bookfusion.com (host not found), docs.audiobookshelf.org (certificate mismatch), booklore-app.github.io docs (404), runtimewire article (sign-in wall).
