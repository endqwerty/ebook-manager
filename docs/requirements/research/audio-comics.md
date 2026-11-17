ISSUES

Why ISSUES: Metron docs, Storyteller docs, Apple and Spotify help, Kobo (partly), Panels, Chunky, MangaUpdates, MAL and Kitsu docs could not be read (blocked, 404, login wall or not attempted). Those items are [unverified] or come from search summaries. No calibre code or calibre docs were seen. "Possible training recall of calibre's" nothing was needed.

Marks: V = read on a page I opened. S = from a search-result summary only. [unverified] = not read.

## 1. Findings

### Part 1. Audiobooks

Entity model
- V. Audiobookshelf (ABS): a "book" is a folder; one item holds audio files plus ebooks plus supplementary files. Single-file books may sit loose in the library root, otherwise one folder per book. https://www.audiobookshelf.org/docs/documentation/libraries/book-library/directory-structure
- V. ABS tracks: each audio file is one track; track number comes from filename and ID3 tag, "whichever is more accurate". Multi-disc: subfolders named Disc/CD/Disk + number, ordered by disc then track. Audio tags are read from the first track only. Same page and https://www.audiobookshelf.org/docs/documentation/libraries/book-library/book-metadata
- V. ABS audio file record: trackNumFromMeta, discNumFromMeta, trackNumFromFilename, discNumFromFilename, manuallyVerified, exclude, error, codec, bitRate, channels, language, embedded chapters, embeddedCoverArt, raw metaTags. Playback "audio track" adds startOffset and duration (so whole-book time = offset + file time). https://api.audiobookshelf.org/
- V. ABS book metadata: title, subtitle, authors[], narrators[], series[] (name + sequence), genres, publishedYear, publishedDate, publisher, description, language, explicit, isbn, asin. Plus title/author sort forms (titleIgnorePrefix, authorNameLF). https://api.audiobookshelf.org/
- V. A book can be in several series; sequence is decimal; non-numeric sequences are allowed but unsorted; subseries shown by collapsing. https://www.audiobookshelf.org/docs/documentation/libraries/book-library/series-management
- V. Folder naming conveys data: `Series/ 1 - Title`, year after the sequence, `{Narrator}` in braces, `[ASIN]` in brackets (Audible ASIN differs from Amazon ASIN). directory-structure page above.
- V. W3C Audiobooks: reading order must be audio only, no resource repeated; one file with many chapters uses media fragments (`#t=0,457.9`) or TOC links; per-item and total ISO 8601 duration that must sum; `readBy`, `abridged`, `inLanguage`, accessibility fields; supplementary HTML/PDF go in `resources`; cover with rel=cover; alternate text per item. https://www.w3.org/TR/audiobooks/
- V. Readium audiobook profile: media type `application/audiobook+json`, package `.audiobook` (`application/audiobook+zip`); each reading-order link needs `duration` in seconds, `bitrate` kbps when known; `alternate` array for other formats/bitrates of the same audio (same duration). Total `duration` is a display hint only. https://www.readium.org/webpub-manifest/profiles/audiobook
- V. Edition-awareness: a chapter is a navigation marker on a timeline of one resource; per-file chapters need offsets; a different narration or an abridged edition has a different timeline, so chapter numbers and resume times cannot be reused across editions. Emby article. https://emby.media/learn/articles/libraries-collections/books-spoken-audio/audiobooks/Library-Audiobook-Chapters.html
- V. Chapter sources: M4B chapter structures (the .m4b extension does not prove they exist); MP3 ID3v2 CHAP and CTOC frames (ms units, element IDs local to the tag). Same Emby page. Cue sheets: not covered there. [unverified] for cue-sheet handling anywhere.

Tag and sidecar reading (ABS, V)
- Default priority, highest first: ABS metadata.json, OPF, desc.txt and reader.txt, NFO, embedded tags (ffmpeg), folder structure. Per-library reorderable. Lower sources fill first, higher overwrite. https://www.audiobookshelf.org/docs/documentation/libraries/book-library/book-metadata
- V. OPF/NFO can give title, author, narrator (role `nrt`), publishYear, publisher, isbn, description, genres, language, series, volumeNumber; series name and index from OPF.
- V. metadata.json mirrors DB metadata per item, rewritten on every change; disabling it risks losing manual edits on next scan. "Embed metadata" writes DB values back to audio tags with a backup copy; not all formats support all tags.
- V. Cover: `cover.jpg` in folder wins; else first embedded cover.
- [unverified] ID3/MP4 atom/Vorbis tag-to-field mapping: ABS page gives none.
- V. "Match": manual (pick result and fields) and Quick Match (closest result by provider priority and duration closeness). Per-library default provider; API shows google (default) and openlibrary; library settings can skip matching when ASIN or ISBN already present. Duration closeness as a match key is a notable idea.

Progress, sessions, bookmarks
- V. ABS media progress: duration, currentTime, progress fraction, isFinished, finishedAt, hideFromContinueListening, startedAt, lastUpdate; keyed by libraryItemId plus optional episodeId. https://api.audiobookshelf.org/
- V. ABS playback session: id, userId, libraryItemId, episodeId, displayTitle, displayAuthor, chapters (snapshot), duration, playMethod (direct/transcode), mediaPlayer, deviceInfo (clientName, version, manufacturer, model, ipAddress), date, dayOfWeek, timeListening, startTime, currentTime, startedAt, updatedAt. Time listening is separate from position, which enables stats.
- V. Ebook progress is tracked separately from audiobook progress in ABS; only the primary ebook stores it; AZW3 and MOBI store none. https://www.audiobookshelf.org/docs/documentation/libraries/book-library/ebooks. So ABS does NOT sync read and listen positions.
- V. One primary ebook plus any number of supplementary ones (EPUB wins automatically). "Audiobooks only" mode makes all ebooks supplementary. Send-to-ereader mails only the primary ebook via SMTP.
- V. Collections (user-owned lists of books) and playlists (items may be podcast episodes) are separate records. Series is a third grouping. Author records have asin, description, image.
- V. Podcast episode fields: season, episode, episodeType, enclosure (url, type, length), pubDate, guid-like identity absent from view. Library settings: skipMatchingMediaWithAsin/Isbn, cron scan, disableWatcher.
- Bookmark fields, listening-stats, backup and share schemas: not found in the pages read. [unverified] ABS public shares exist (page named in sidebar, not opened).

Player behaviours
- V. BookPlayer (App Store notes): smart rewind, volume boost, sleep timer with adjustable duration and end-of-chapter mode (a bug existed for streamed books), chapter skip, "reload parsed chapters if missing", bookmarks with notes, Apple Watch standalone, CarPlay, Jellyfin and ABS streaming (v6), paid cloud sync. https://apps.apple.com/gb/app/bookplayer/id1138219998. Per-book speed: not stated.
- S. Smart AudioBook Player: global speed default with per-book override, 0.5x to 4x, pitch, skip silence with a max silence length 0.1 to 0.9 s (default 0.5 s), no trimming at file or chapter edges, sleep timer with shake to reset, library is folder-based with one folder per book. https://www.apkmirror.com/apk/alex-kravchenko/smart-audiobook-player/ (changelogs)
- V. Kobo iOS: 10 s skip, chapter list, speed picker, sleep timer. S. Kobo speed range 0.5x to 3x; sleep presets up to an hour or end of chapter; Libra Colour plays Kobo audio over Bluetooth but sideloaded audio is reported weaker. https://help.kobo.com/hc/en-us/articles/360017964534-Audiobook-controls-on-iOS
- S. Audible renamed Whispersync for Voice to "Read and Listen": needs both the ebook and the audiobook in the linked Audible and Kindle libraries; Immersion Reading (highlighted text while listening) limited to supported English and German titles in US, UK, AU, DE. The Audible help article itself was empty when fetched.
- S. Spotify: Premium includes 15 hours a month of the subscriber catalogue, unused hours expire monthly, top-ups in 10-hour blocks (reportedly valid 12 months, community post, 2023). Shows that "access quota" can be a user-level record.
- S. Storyteller: takes a DRM-free ebook and audiobook, aligns text to audio (CTC aligner for rough chapter boundaries then forced alignment), writes EPUB 3 Media Overlays so the sync lives inside the EPUB; ebook is source of truth so audio intro/outro with no text is missed; about 80% good before manual fixing (author of another tool). https://runtimewire.com/article/storyteller-v3-ctc-aligner-immersive-reading (login wall, one-line summary only) plus search summaries.
- S. Apple Books: read-aloud and narration highlighting need fixed-layout EPUB with Media Overlays; word, sentence or no highlight; `-epub-media-overlay-active` colour required if text is coloured. https://help.apple.com/itc/booksassetguide/en.lproj/
- [unverified] car mode specifics, Android Auto, Plappa, Prologue docs, listening-stats screens: not read.
- EPUB media overlays: V only that section 9 uses SMIL subset (seq, par, text, audio), properties duration, narrator, active-class, playback-active-class (Appendix D.8). Body not read. https://www.w3.org/TR/epub-33/

### Part 2. Comics and manga

Hierarchy and numbering
- V. ComicInfo fields: Series, Number, Count, Volume, AlternateSeries/Number/Count, Title, Summary, Notes, Year/Month/Day, creator roles (Writer, Penciller, Inker, Colorist, Letterer, CoverArtist, Editor, Translator), Publisher, Imprint, Genre, Tags, Web, PageCount, LanguageISO, Format (TBP, HC, Web, Digital), BlackAndWhite, Manga, Characters, Teams, Locations, MainCharacterOrTeam, ScanInformation, StoryArc, StoryArcNumber, SeriesGroup, AgeRating, CommunityRating 0 to 5, Review, GTIN, Pages. Roles are comma lists, so a name containing a comma breaks. Volume "by number or year". https://anansi-project.github.io/docs/comicinfo/documentation
- V. ComicInfo history: 2.0 made Manga an enum (breaking), added AgeRating, StoryArc, SeriesGroup, Characters etc.; 2.1 (draft) adds Translator, Tags, StoryArcNumber, GTIN. https://anansi-project.github.io/docs/comicinfo/schemas/changelog
- V. Enums: Manga = Unknown, No, Yes, YesAndRightToLeft. AgeRating = Unknown, Adults Only 18+, Early Childhood, Everyone, Everyone 10+, G, Kids to Adults, M, MA15+, Mature 17+, PG, R18+, Rating Pending, Teen, X18+. https://anansi-project.github.io/docs/comicinfo/schemas/v2.1
- V. Per-page: Image (index), Type (FrontCover, InnerCover, Roundup, Story, Advertisement, Editorial, Letters, Preview, BackCover, Other, Delete), DoublePage, ImageSize, ImageWidth, ImageHeight, Bookmark, Key (unclear). The Delete type hides a page without editing the archive.
- V. MetronInfo (newer XML): arrays of elements instead of comma strings, data-source tags, ids on characters and creators, age-rating mapping matrix. ComicTagger lists it as having no plugin yet; CBI and CoMet deprecated. https://metron-project.github.io/blog/metroninfo-v1 and https://github.com/comictagger/comictagger/wiki/MetadataSchemes. Metron's own data docs were blocked (Anubis page, tried once); [unverified] series types, reprints, universes.
- V. Comic Vine model: publisher, volume (start_year, count_of_issues, first/last issue), issue (issue_number as text-ish, cover_date vs store_date, story_arc_credits, first appearances), story_arc, people, characters, teams, concepts, locations, objects, powers, origins. Credits per issue. https://comicvine.gamespot.com/api/documentation
- S. Comic Vine decimals like #10.1 and #33.5 exist and were dropped by an API change (bug thread). Annuals run a separate count and may be named by year. One-shots are conventionally #1 or 0. Forum posts, not official rules.
- V. Legacy vs volume numbering: restarts, restorations, shadow (double) numbering, fan-assigned continuous numbers; magazines restart volume numbers per year. https://comichron.com/faq/legacynumbering.html
- V. Kavita library types: Comic (strict Comic Vine style: series name is "Name (start year)", TPBs and annuals are separate series, no volume tabs), Comic (Flexible), Manga, Light Novel, Book, Image (loose images, folder = series). https://wiki.kavitareader.com/guides/admin-settings/libraries/
- V. Kavita naming parser: volume markers v, vol, volume and local words (tome, 巻, 卷, 권, Том ...); chapter markers ch, chapter, c, 話, 话, 화, Глава; decimals (025.5 = 25.5); ranges for both chapters (c001-006) and volumes (v16-17); a Specials subfolder; cover.jpg, !cover, folder treated as covers not pages. https://wiki.kavitareader.com/guides/scanner/manga/ and /scanner/. I did not open the parser test file the page names.
- V. Kavita maps Format values (Annual, TPB, Omnibus, One-Shot, Graphic Novel, GN) to Special; trades in a Comic library use `Number` "1-5" to stay in order or "TPB1" to stay out; series age rating = highest among files; Count non-zero = Ended, equals total = Completed; Web URLs help match CBL entries. https://wiki.kavitareader.com/guides/metadata/comics/
- V. Komga: series per subfolder at any depth; oneshot is a deliberate flag via a named directory (not just a single-book series); oneshot book supplies series title and summary, status Ended, count 1. Collections from SeriesGroup; read lists from StoryArc or AlternateSeries ordered by the paired number. Also reads ISBN barcodes from first and last three pages and Mylar series.json. https://komga.org/docs/guides/scan-analysis-refresh and https://komga.org/docs/guides/oneshots
- V. Read lists: any books from any series or library, manual order (drag) or release date order; ComicRack .cbl import matches by series plus number, partial match allowed. Kavita: CBL v1 and v2 (.json), unmatched items remapped with saved remap rules, URL lists re-sync every 3 days, a list may mix manga, EPUB and PDF, and a list may lose items the user cannot access by rating or library. https://komga.org/docs/guides/readlists and https://wiki.kavitareader.com/guides/features/readinglists/

Reading settings
- V. Kavita reader: direction LTR or RTL, tap zones, webtoon infinite scroll, scaling (fit height, fit width, original), layout single, double, double (manga), double-page offset (shifts when no cover), split two-page images with left or right half first, width override, emulate-book spine shadow. Reading profiles are bound to a series or a library, optionally per device; lookup order: series implicit, series profile, library profile, default; changing a setting while reading silently creates a device-bound implicit profile. https://wiki.kavitareader.com/guides/readers/comic-manga/ and /user-settings/reading-profiles/
- V. Mihon: reader mode chosen per series (paged RTL default, LTR, vertical, long strip, long strip with gaps), crop borders, rotation lock modes, background, scale types (fit screen, stretch, fit width, fit height, original, smart fit), zoom start position; "split wide pages" not yet documented. https://mihon.app/docs/guides/reader-settings
- V. YACReader: library keeps files in place; reading lists, labels, favourites, ratings; double page, RTL, continuous scroll; iOS app has panel-by-panel reading, auto margin trim, image filters for old scans, on-device page translation. https://yacreader.com/
- V. OPDS-PSE: stream link with `{pageNumber}` and optional `{maxWidth}`, `pse:count`, optional `pse:lastRead` (reading position on the server). v1.0 says pages from 0, v1.2 draft says from 1. Known clients: Chunky, Panels, KOReader. Source: search summary of https://github.com/anansi-project/opds-pse (not opened directly). S.
- [unverified] WebP, AVIF, JPEG XL support: Komga media-types page returned 404; Kavita image formats not stated on the reader page. Colour e-ink handling: not found.

Trackers
- V. Mihon: supports MangaBaka, MyAnimeList, AniList, Kitsu, MangaUpdates, Shikimori, Bangumi, Hikka; tracker must be added per series; one-way push of chapters read, status, start/finish dates, score; scoring scale follows the tracker (AniList, Kitsu, MangaBaka); progress is pushed when last page of a chapter is reached or marked read; offline progress queues and syncs later; Komga/Kavita/Suwayomi are also supported as sync targets. https://mihon.app/docs/guides/tracking
- V. Kavita scrobbling is a Kavita+ feature and is enabled per library; Manga, Light Novel and Image types support it.
- [unverified] AniList field list (docs 403), MAL, Kitsu, MangaUpdates.

### Part 3. Other content

- V. Magazines vs comics: magazine volume numbers restart yearly and are known by cover date (Comichron).
- S. Kindle periodicals: issue holds sections, sections hold articles; cover view and section view; "keep this issue" stops auto-deletion (Fire deletes issues more than seven old); text view vs page view. https://www.dummies.com/article/technology/electronics/tablets-e-readers/kindles/newspapers-and-magazines-on-your-kindle-paperwhite-157145
- V. Fixed layout EPUB: spine items marked pre-paginated; spine properties page-spread-left/right; synthetic spreads; viewport dimensions deprecated in favour of content dimensions. Apple: read-aloud requires fixed layout. https://www.w3.org/TR/epub-33/ (outline only)
- V. EPUB scripting: scripted content documents allowed (also forms); two contexts (container-constrained, spine-level). Rules not read. Same page.
- V. EPUB Dictionaries and Glossaries: `dc:type=dictionary`, one search-key-map (`application/vnd.epub.search-key-map+xml`) per dictionary, entries with headword `dfn`, optional condensed entry for small lookup popups, single source language and one or more target languages (RFC 5646), `dictionary-type` (monolingual, bilingual, multilingual, thesaurus, encyclopedia, spelling, pronouncing, etymological), multi-dictionary via collection role=dictionary; match must treat NFC and NFD as equal; lookup can come from other books. A glossary is part of its book, not a lookup source. https://idpf.org/epub/dict/epub-dict.html
- V. Japanese layout: vertical writing, right-to-left page progression, tate-chu-yoko, ruby (mono, group, jukugo), kinsoku line breaking. https://www.w3.org/TR/jlreq/
- V. MusicXML 4.0 (June 2021): .mxl compressed container, can combine score and parts in one file, concert-score transposition, instrument-to-part links, part name credit type. https://www.musicxml.com/for-developers/. Compressed container details and per-part handling not read.
- [unverified] ZIM / WARC for web archives (Kiwix blocked, not retried). Interactive fiction beyond EPUB scripting. Cookbook widgets.

## 2. Data the app must store

Item and file structure
- Work (the abstract book) separate from Edition (ISBN, language, narration, abridged flag, translator, year) separate from Asset (an actual file group). One edition may have an ebook asset, an audio asset and a comic archive asset. Cross-asset link row: relation = same-text, same-narration, aligned (has overlays), translation, adaptation.
- Asset has an ordered list of Parts (audio file, archive, image folder, article file, spine item) with: path, hash, size, duration, bitrate, codec, channels, track and disc numbers from filename, from tag and the one chosen, error, excluded flag, manually-verified flag, startOffset in whole-book time.
- Chapter/segment: owner (part or whole asset), start, end, title, level (hierarchy), source (embedded, cue, lookup, user, generated), and edition scope. Keep chapter snapshots in sessions.
- Contributor-role link: person + role (author, narrator, translator, illustrator, penciller, inker, colorist, letterer, cover artist, editor) + per-edition, with ids, so that comma-separated names are never the storage form.
- Series link (many per work): series id, sequence as text plus numeric sort key (decimals, ranges 1-5, TPB1, lettered, year-named, shadow second number), series kind (ongoing, limited, one-shot, annual, TPB, omnibus, special), volume label (number or start year), start year, count, status (ongoing/ended/completed), publisher, imprint.
- Reading-order list entity (story arc, read list, CBL): ordered items by work, optional external match key (series+number, web URL, GTIN), unmatched entries kept with remap rules, visibility owner or shared, mixed formats allowed.
- Date fields: cover date and store/release date separately, plus year-only.
- Age rating as enum with a mapping table to other scales; per-series value derived as maximum; per-user rating cap.
- Identifiers: ISBN/GTIN, ASIN (audio and Kindle differ), ComicVine id, Metron id, ComicInfo, provider id plus source tag plus fetched time per field (MetronInfo style).
- Sidecar registry: which files were read (OPF, NFO, desc.txt, reader.txt, metadata.json, ComicInfo.xml, series.json, cover files) and a per-library priority order of sources; per-field provenance so a rescan does not overwrite edits.
- Page records (comics): index, type (Story, Ad, Delete ...), double-page flag, width, height, byte size, bookmark flag.

Per-user and per-device state
- Position record per user per asset type (audio time, text locator, page index) plus a link table of "equivalent positions" (audio time to text locator via overlay map). Never assume one position per book.
- Progress: duration, current time, fraction, finished flag and date, started date, hide-from-continue flag. Tracker-visible counts: chapters read, volumes read.
- Listening/reading session: device (id, client, version, model), start and end, time spent, start and end position, play method (stream/download), chapter snapshot, day. Stats derive from sessions.
- Bookmark and clip: asset, position, title, note, optional end (clips), created time.
- Player settings with scope chain: global, library, series, book, device. Speed, skip silence (max silence length), volume boost, smart rewind, sleep mode (minutes, end of chapter, shake reset, auto start). Reader settings with same chain: direction, layout, scale, double-page offset, split mode, width override, crop, rotation, background. Include "implicit profile" created by changing a setting.
- Tracker link per user per series: service, remote id, status, score (service scale), start and finish dates, chapters and volumes read, pending queue for offline pushes, last sync, per-library scrobble allow flag.
- Quota/entitlement record (streamed audio hours) if streaming services are ever added.
- Download state per asset per device (whole book, partial, streaming), page-stream capability for large archives.

## 3. Requirements a new manager should adopt that existing products get wrong

1. Do not store read position and listen position in one field; ABS keeps ebook and audio progress apart and gives no way to jump between them. Add an alignment map (overlay-derived) so a user can switch at the same place.
2. Make chapters edition-scoped and source-labelled. Reusing chapters across narrations or abridged editions gives wrong navigation.
3. Store contributors as structured lists with roles. ComicInfo's comma-separated strings and Kavita's comma rule collapse names with commas.
4. Keep sequence as text plus a computed sort key. ABS leaves non-numeric sequences unsorted; Comic Vine dropped decimals once; Kavita needs hacks ("1-5", "TPB1") to place trades.
5. Treat one-shot as an explicit flag, not "series with one book" (Komga is explicit; others infer).
6. Keep reading-order lists as first-class, cross-series, cross-format entities with saved remap rules for unmatched imports.
7. Per-series reader profile with a device dimension, and show to the user which profile is active (Kavita creates hidden implicit ones).
8. Track field provenance and per-library source priority (ABS's order is a good model); rescans must not wipe manual edits, and metadata.json style export should be optional.
9. Record both cover date and store date; Comic Vine does, ComicInfo has one Year/Month/Day.
10. Preserve "Delete" page type and double-page flags without rewriting archives; do not edit user archives by default.
11. Handle several same-title editions: use duration closeness plus ASIN/ISBN as match keys, as ABS Quick Match does.

## 4. Open questions

- Format support for WebP, AVIF, JPEG XL, large-archive streaming limits, colour e-ink page handling: no primary page read. Needs Kavita and Komga image docs or tests.
- Metron series types, reprint and variant model; MetronInfo element list; Comic Vine volume restart rules (official wiki not read).
- Tag mapping for ID3, MP4 atoms (©nam, ©ART, ©nrt?), Vorbis, cue sheets: no doc read; use format specs (ID3v2.4 chapters addendum, MP4 spec) in a follow-up.
- Storyteller official docs, Plappa, Prologue, Smart AudioBook Player own docs, ABS bookmark/stats/share/backup schemas (API page offset 200000 onward unread).
- EPUB media overlay body, fixed-layout rules, scripting security rules, sheet music viewers, web archive formats (WARC, ZIM), interactive fiction, cookbook widgets, newspaper article models beyond Kindle (old manuals).
- AniList, MAL, Kitsu, MangaUpdates exact fields and rate limits.
- Cases where one "book" is the wrong unit (below) need a design decision on Work/Edition/Asset naming.

Cases where a single "book" record is the wrong unit
- One audiobook split into many files; one file holding many books (box set m4b); box sets and omnibuses holding several works; an ebook plus audiobook that are the same work; the same work in several narrations, translations or abridgements; a comic archive that is one chapter, one volume, one issue or a whole run; a magazine or newspaper issue containing many articles and its own volume/issue/date numbering; a dictionary used as lookup source across the library (not a read item); a glossary inside a book; sheet music with parts (score plus per-instrument parts in one .mxl); web archives or image folders with thousands of small files; a series that is also a collection; read lists that cross series and formats; a picture book whose text, audio and layout are one fixed-layout unit; a light novel volume with illustrations and vertical text where page direction is RTL.

## 5. Source URLs opened (fetched)

https://www.audiobookshelf.org/guides/book-library-setup/ (landing only)
https://www.audiobookshelf.org/docs/ (landing)
https://www.audiobookshelf.org/docs/libraries (landing)
https://www.audiobookshelf.org/docs/documentation/introduction
https://www.audiobookshelf.org/docs/category/libraries
https://www.audiobookshelf.org/docs/category/book-library
https://www.audiobookshelf.org/docs/documentation/libraries/book-library/directory-structure
https://www.audiobookshelf.org/docs/documentation/libraries/book-library/book-metadata
https://www.audiobookshelf.org/docs/documentation/libraries/book-library/ebooks
https://www.audiobookshelf.org/docs/documentation/libraries/book-library/series-management
https://api.audiobookshelf.org/ (twice, offsets 0 and 100000)
https://github.com/advplyr/audiobookshelf-app (README without feature list)
https://github.com/rasmuslos/ShelfPlayer (archived, no features)
https://www.w3.org/TR/audiobooks/
https://www.readium.org/webpub-manifest/profiles/audiobook
https://www.w3.org/TR/epub-33/ (twice, outline only)
https://idpf.org/epub/dict/epub-dict.html
https://www.w3.org/TR/jlreq/
https://www.musicxml.com/for-developers/
https://emby.media/learn/articles/libraries-collections/books-spoken-audio/audiobooks/Library-Audiobook-Chapters.html
https://apps.apple.com/gb/app/bookplayer/id1138219998
https://help.kobo.com/hc/en-us/articles/360017964534-Audiobook-controls-on-iOS
https://runtimewire.com/article/storyteller-v3-ctc-aligner-immersive-reading (login wall)
https://anansi-project.github.io/docs/comicinfo/documentation
https://anansi-project.github.io/docs/comicinfo/schemas/v2.1
https://anansi-project.github.io/docs/comicinfo/schemas/changelog
https://metron-project.github.io/blog/metroninfo-v1
https://github.com/comictagger/comictagger/wiki and /wiki/MetadataSchemes
https://comicvine.gamespot.com/api/documentation
https://comichron.com/faq/legacynumbering.html
https://wiki.kavitareader.com/ , /guides/scanner/ , /guides/scanner/manga/ , /guides/readers/comic-manga/ , /guides/features/readinglists/ , /guides/admin-settings/libraries/ , /guides/metadata/comics/ , /guides/user-settings/reading-profiles/
https://komga.org/docs/guides/scan-analysis-refresh , /oneshots , /readlists
https://mihon.app/docs/guides/tracking , /reader-settings
https://yacreader.com/
https://mokkari.readthedocs.io/en/stable/ (no schemas)
https://github.com/Kareadita/Kavita/wiki/Series-Naming-and-Folder-Structure (stub)

Blocked or failed (not retried beyond once): metron.cloud docs (Anubis), wiki.openzim.org (Anubis), docs.anilist.co (403), anilist.github.io (404), smoores.gitlab.io (login redirect, not followed), amazon help (503), spotify support (404), Audible help (empty body), Apple Books guide (links only), komga scanner/media-types/readers pages (404), specs.opds.io pse (404).
Search-result-only sources (marked S): Smart AudioBook Player changelogs, Kobo speed and sleep timer, Audible Read and Listen naming, Spotify audiobook hours, Apple Books Asset Guide, OPDS-PSE, Kindle periodicals, Comic Vine numbering forums.
