ISSUES

Reason for ISSUES: Reddit was blocked (old.reddit.com refused once; not retried), and Algolia HN search pages were not reachable through the fetch tool, so Hacker News coverage is one thread (Citadel Show HN, Jan 2024, 159 comments) plus one Show HN mirror (Jul 2025). The rest comes from Tildes (Feb 2026 and later), Lemmy, MobileRead (behaviour-only threads), GitHub issues, and blogs. "Counts" below are rough tallies across the 13 pages I opened and the search-result summaries, not survey data. No calibre code was seen on any page opened. I did not open calibre's manual, plugins or plugin threads; the fan-fiction item below is from a search summary only, and I did not open the plugin threads it came from. Possible training recall of calibre's internals: none written down.

## 1. Findings

### 1.1 What people praise and would not give up
- Powerful metadata/conversion toolbox: HN commenter calls it "a powerful tool suited to enthusiasts"; others say 13K and 25K+ books "run fine" (HN Citadel thread, https://news.ycombinator.com/item?id=38988019) [verified]. Count: about 4 sources.
- Plain files on disk, no proprietary ecosystem: "Zero proprietary ecosystem, so I can use Calibre to manage everything" (Lemmy quote surfaced in search results) [unverified, snippet only]. Users sync the library folder to a phone with Syncthing (HN 38988019) [verified].
- Kobo-style sync through a server that mimics the store: Calibre-Web/CWA users say Kobo sync was "a dealbreaker" in choosing a server (https://tildes.net/~books/1soy/here_are_your_choices_for_a_self_hosted_ebook_server) [verified].
- KOReader fans praise "in-page footnotes, OPDS, wireless Calibre transfer, and progress tracking" (https://lemmy.mariusdavid.fr/post/569564) [verified].
- Audiobookshelf "rock solid as an LXC" (Tildes 1soy) and Ubooquity "reliable for years" [verified].
- BookLore's metadata editing "significantly less clunky than Calibre" (Tildes 1soy) [verified].

### 1.2 Pain points (rough count of pages mentioning it, of 13 opened)
- Interface/approachability (5): "unfamiliar and kinda random" icons, "click five levels deep in a dozen menus", "very bloated and cumbersome", "outdated UI" (HN 38988019; Tildes https://tildes.net/~books/1v2d/ebook_management_for_kobo; HN Librari thread https://hn.svelte.dev/item/44608840) [verified].
- Large libraries / speed (4): "absolutely craters after 20,000+ ebooks"; a 100GB library "crashed often and was slow to search"; a few hundred thousand backup ebooks where "even listing the directory is slow" (HN 38988019) [verified]. Search-result summaries add 4-minute and 30+ minute startups on 10K and 20K+ libraries, attributed to hidden columns/caches (MobileRead, 2011-2025) [unverified, snippets only]. A third-party app "stopped working around 3,000 entries" (HN Librari) [verified].
- Server / NAS / remote access (4): Calibre "was never designed to be a hosted library accessible over the web"; "next to impossible to self-host on a NAS" (HN 38988019; Lemmy mariusdavid) [verified]. Kobo sync via a home server "does need to be publicly accessible" off the LAN, or Wi-Fi/VPN at home (Tildes 1soy; Lemmy mariusdavid) [verified].
- Reading progress split across apps (4): "A bunch of different devices having different progress points... is awful"; wish for one app syncing ebook and audiobook progress "like Amazon's whispersync" (https://lemmy.linuxuserspace.show/post/1148791; search-result quote of another Lemmy thread) [verified/unverified]. Web reader of CWA "doesn't sync progress with KOReader" [unverified, snippet]. KOReader's OPDS is "download-only, with no progress or highlight upload" (Tildes 1v2d) [verified]. KOReader sync into Kavita failed on a file-matching setting (Filename vs Binary hash) (https://github.com/Kareadita/Kavita/issues/4518, Mar-Jul 2026) [verified].
- Highlights/notes stuck per device (4): "Amazon's Kindle Notebook page is basically unsearchable... there's no single place to see any of it" (https://www.mobileread.com/forums/showthread.php?p=4599073, 2026); "syncing highlights and notes was the one sync type he gave up on" (https://tech.stonecharioteer.com/posts/2025/kollector); Syncthing "progress sync works well, but syncing highlights is a separate problem" (search summary, MobileRead) [mixed]; "None of them stores the annotations in a fashion which other programs can easily access" (https://notes.andymatuschak.org/zX95uZHiGWNiNTDF3wETWtq, undated, likely older than 2022) [verified].
- Forced folder layout / duplicates / orphan folders (3): both "King, Stephen" and "King_Stephen" folders, ghost folders after dedupe, "deleting files directly in the library folder is a big no-no because it's a database"; Japanese titles transliterated in folder names (search summary of MobileRead/Lemmy; HN 38988019) [mixed]. One Lemmy user wanted to "dump everything in one place and use their reader's interface to build series" [unverified, snippet].
- Metadata quality (3): "Many commenters say older titles lack ISBNs"; scanning "pulls in a wrong book" (HN Librari) [verified]; "automatic metadata scraping can cause drift in large libraries" (Lemmy linuxuserspace) [verified]; Calibre's metadata handling "clunky" (Tildes 1soy) [verified].
- Sending to devices (4): USB sideloading "is a chore" and a work laptop's USB policy locked an author out of his own Kindle (https://hn.jakechampion.name/reader/https://philwornath.com/garden/self-hosted-ebook-sync-koreader, Apr 2026) [verified]; Send-to-Kindle by email needs SMTP, "clunky, took forever to configure" [unverified, snippet]; Kobo's API redirection "requires an official Kobo account" and "marketing popups and discovery fluff" (Tildes 1v2d) [verified].
- Multi-user households (3): "separate libraries for different household members, assigned by login" (HN 38988019); members of a library with personal ratings and reading logs (HN Librari); Kavita shared position vs per-device accounts conflict (Lemmy linuxuserspace) [verified].
- Plugin breakage/abandonment (3): "Calibre plugins didn't work for him" (stonecharioteer); "graveyard projects" fear (HN Librari); BookLore deleted by its developer, forked as Grimmory (https://store.elfhosted.com/blog/2026/08/10/chaptarr-has-arrived-grimmory-steps-up-and-readarr-sails-away/); Readarr retired and its metadata servers gone, "no in-place database migration" to Chaptarr [verified].
- Corruption/backups (2): Citadel warns it "could corrupt a library, so back up Calibre first"; BookLore README warns of file-corruption risk on certain mounts [unverified, search snippets].
- DRM friction (3, pain only): users want DRM removal "automated"; Amazon changes forced a move to Kobo (Lemmy mariusdavid; search snippets) [verified/unverified]. Not researched further.
- Covers, series, PDFs, comics, audiobooks: comics/manga in CBZ/CBR/PDF need a second tool (Kavita or Komga beside Calibre-Web) and "zoom into specific comic panels on double-tap" was not found in Kavita/Komga (Tildes 1soy, 1v2d) [verified]. Audiobooks need Audiobookshelf; Opus-transcoded audiobooks had "bad/nondeterministic seeking" on iOS (Lemmy mariusdavid) [verified]. EPUB table of contents empty in a third-party Komga client; EPUB reading-list navigation bug (Dec 2025) [unverified, snippet].
- Subscriptions/licensing: Readest "500 MB free tier" and cloud-sync moved behind a paywall (Tildes 1v2d; aggregator snippets) [verified/unverified]; "one-time payment rather than a subscription" (Tildes 1v2d); "Less AI/LLM involvement" and BookLore criticised for "undisclosed AI usage" (Tildes 1v2d; industrialmonitordirect, Jun 2026) [verified].

### 1.3 Multi-tool workflows and the glue people wish was built in
- Buy DRM-free epub -> library manager -> Syncthing -> Calibre-Web -> Kobo `api_endpoint` edited over USB; audiobooks -> Audiobookshelf; Libation for Audible; CWA ingest folder (Lemmy mariusdavid) [verified].
- Jailbroken Kindle + Android: KOReader + Syncthing + VPS + CWA + KoShelf dashboard for stats/highlights (Wornath, Apr 2026) [verified].
- Library manager + Kavita for comics + Audiobookshelf for audio ("running two tools side by side") [unverified, search summary].
- Highlights: export from each device to HTML/JSON, "join them on your computer"; a separate tool built to be "Calibre, but for highlights" (MobileRead 4599073; stonecharioteer) [verified].
- Wished-for glue: Kobo pulling directly from Audiobookshelf/Kavita without USB; automated DRM step; highlight sync for read-it-later articles; a single app syncing all progress (Lemmy mariusdavid; Lemmy linuxuserspace) [verified].

### 1.4 Specific requests no tool does well
Reading status/dates across devices; one place for highlights from all devices; send-to-device from a phone; automatic organisation of a messy downloads folder (ingest folders exist in CWA but metadata is "significantly limited" per industrialmonitordirect, Jun 2026); "clear import feedback" per file (HN 38988019); custom columns such as read status (HN 38988019); a CLI ("Calibre library CLI" drew interest, HN 38988019); an API; annotations that survive replacement of a file (fan-fiction updates leave the old Kobo copy in place and lose position; search summary of MobileRead, [unverified]); "last checked" separate from "updated" for serial stories [unverified]; reading statistics (KoShelf mention); sharing with family; full-text search [not seen in the pages I read: [unverified]]; mobile app that syncs progress with ebook+audio together.

### 1.5 Library sizes and what breaks
- ~8 GB: typical single reader (HN 38988019). 3,000 entries: third-party tool features stop (HN Librari). 10K-25K books: reported fine by some, slow startup by others. 20,000+ : "craters" (HN) / 30+ min startup (snippet). 100 GB: crashes and slow search. Hundreds of thousands: directory listing slow. [verified for HN items]

### 1.6 Why people abandon a new manager
- Missing import of an existing library (Calibre-compatibility is the selling point of Citadel and CWA); no Kobo/KOReader device support ("a dealbreaker"); no custom fields / weak metadata editing; stalled or deleted projects (BookLore -> Grimmory, Readarr retired, Calibre-Web "development has stalled"); AI-code and governance disputes ("slop code ... pulled in faster than it can be checked", search snippet) [unverified]; paywalled sync; resource use ("resource hog", Tildes 1v2d); instability of forks ("extremely unstable").

### 1.7 What newer-project users still miss
- BookLore/Grimmory: native Kobo sync needed a test device; "server does need to be publicly accessible"; metadata editing slow in Grimmory; migration paths from BookLore 2.2.0 shared by the community [verified Tildes; unverified snippets].
- Kavita: basic progress works in web reader but KOReader hash-matching trips users; scrobbling is paid (Kavita+) [verified issue; snippet for paid claim, disputed in thread].
- Komga: Kobo sync with read-state and full metadata editing is the closest match for ex-BookLore users, ebook robustness doubted by one commenter (industrialmonitordirect; Lemmy linuxuserspace).
- Stump: Kobo sync "still in development" [unverified].
- Readest: storage cap/paywall, TTS needs internet, WebView2 dependency on Windows [unverified aggregators].
- BookFusion: "isn't self-hostable, and KOReader support came later" (Tildes 1v2d) [verified]; Calibre-to-BookFusion highlight sync is one-way (Reddit 2023 via mirror, snippet) [unverified].
- Alfa: nothing found.

## 2. Data the app must store
- Per-user, per-book reading state: status, started/finished dates (several), percent, position in a format-independent form plus a per-device last-known position, device of last read, with history so positions survive file replacement.
- Annotations as their own records with anchors that survive a changed file (text quote plus context plus location), colour, note, device/source label, original source file, created/modified times; imports from Kindle, Kobo, KOReader files; export templates.
- File hash set per book file (binary hash and filename) to match sync clients such as KOReader.
- Multiple files per book, with role (original, converted, replaced), provenance, and "replaced from" link for serial updates.
- Source URL for serials with last-checked and last-updated separately.
- Import log per file: status, reason, matched metadata, undo link.
- Per-library and per-user permissions; household members each with their own shelves, ratings and progress; shared libraries.
- Device records: type, transfer method (USB mount, email address, OPDS, Kobo-style sync), per-device path rules and what was sent when.
- User-defined fields with types, applied across libraries; folder-like "path" field kept separate from storage layout.
- Audiobook files linked to an ebook with synchronised positions.
- Reading sessions (start/stop, duration) for statistics.
- Metadata provenance per field (source, confidence, locked by user) to prevent "drift".
- Operation journal and snapshot for backups/undo.

## 3. Requirements existing products get wrong
- Open any existing Calibre-style library without changing or locking it; keep plain files readable without the app.
- Start fast and search fast at 20K, 100K and larger libraries; never read every file at start.
- Reader-side and server-side parity: one position for web, phone and e-ink devices, including audiobook pairing.
- Per-file import feedback with undo; dry-run organiser for a messy downloads folder.
- Safe on network mounts; documented backup and restore; detect corruption.
- Never require public exposure of a server for Kobo/Kindle sync; send to Kindle without SMTP setup.
- Metadata editing that is fast, bulk-capable, and does not overwrite locked fields.
- Free local features: no paywall on sync of your own data; one-time pricing if any.

## 4. Open questions
- Real HN/Reddit threads beyond Citadel were not read; counts may under-represent r/ebooks, r/kindle, r/kobo.
- Whether full-text search is a top request (not found in opened pages).
- Which Kavita features are paid and whether CWA web reader now syncs with KOReader.
- Date of the Andy Matuschak note (probably before 2022).
- Alfa could not be identified.

## 5. Ranked list: twenty most requested capabilities (rough frequency across sources)
1. Single reading position across every device and app, ebook and audiobook paired.
2. Kobo and KOReader sync built in, with no public exposure needed.
3. Library that scales to 20K+ and 100 GB without slowdown or long start-up.
4. Import of an existing library, folders or Calibre-style, non-destructively.
5. One searchable store for highlights/notes from all devices, with export (JSON, CSV, Markdown).
6. Clean, approachable interface; fewer menu levels; good keyboard control.
7. Server/NAS-friendly operation with web access and a phone app.
8. Fast, bulk metadata editing with trusted sources and no silent drift.
9. Custom fields (read status, rating, folder-like paths) as first-class.
10. Multi-user households with per-user progress, shelves and permissions.
11. Wireless send-to-device from phone or desktop (Kindle email, Kobo, USB) without SMTP setup.
12. Automatic ingest and organisation of a downloads folder with per-file feedback.
13. Plain files and transparent layout; no forced folder scheme.
14. OPDS plus progress/highlight upload, not download only.
15. Duplicate detection and merge with safe cleanup.
16. Comics, PDF and audiobook support in the same library.
17. CLI and API for scripting.
18. Reading statistics and dashboards.
19. Backup, integrity check and corruption recovery.
20. Serial and fan-fiction updates that keep position and notes.

### Trust requirements before people move their library in
- Never modify or lock the existing library; read-only import first, with a visible report and undo.
- Files stay as plain files and open without the app; documented, stable on-disk layout and export of everything (metadata, annotations, progress).
- Backups and corruption checks built in; safe on network storage.
- Open licence and governance; no single-developer deletion risk; clear AI-use policy.
- No paywall on syncing your own data; clear, stable pricing.
- Device and Kobo/KOReader support at launch, not "planned".
- Migration paths out as well as in.
- Written commitment to maintenance and a plan if the maintainer leaves.

## 6. Source URLs opened
- https://news.ycombinator.com/item?id=38988019
- https://tildes.net/~books/1soy/here_are_your_choices_for_a_self_hosted_ebook_server
- https://tildes.net/~books/1v2d/ebook_management_for_kobo
- https://lemmy.mariusdavid.fr/post/569564
- https://lemmy.linuxuserspace.show/post/1148791
- https://store.elfhosted.com/blog/2026/08/10/chaptarr-has-arrived-grimmory-steps-up-and-readarr-sails-away/
- https://www.mobileread.com/forums/showthread.php?p=4599073
- https://tech.stonecharioteer.com/posts/2025/kollector
- https://notes.andymatuschak.org/zX95uZHiGWNiNTDF3wETWtq
- https://hn.jakechampion.name/reader/https://philwornath.com/garden/self-hosted-ebook-sync-koreader
- https://hn.svelte.dev/item/44608840
- https://industrialmonitordirect.com/blogs/knowledgebase/kobo-e-reader-sync-alternatives-to-booklore
- https://github.com/Kareadita/Kavita/issues/4518
Blocked or failed: old.reddit.com (refused, not retried), deddit.petersanchez.com (DNS), lemmy.sudovanilla.org (404), tech.yahoo.com (403), lemmy.manganiello.tech (502). Roughly ten web searches supplied the [unverified] snippet items.
