ISSUES

Apple's current guide pages render as table of contents plus one article tail; only some articles could be read. Many questions stay unverified. No calibre code or calibre material was seen. No "possible training recall" flags.

Marking: [V] = read on a page I opened (the fetch tool summarises pages, so wording is paraphrase). [S] = from a search-result summary only. [unverified] = not found.

## 1. Findings

### Themes & Settings (iPhone guide, current "Read books")
- [V] Open via Menu > Themes & Settings. Controls: font size (large A / small A), Page Turn button (scroll or page style), Appearance button (Light/Dark/auto background mode), brightness bar, page themes (examples named: Quiet, Bold), Customize section with font choice (examples: Original, Palatino), Bold Text, Accessibility & Layout Options (line spacing, character spacing, word spacing, margins sliders, switched on by "Customize"), Justify Text on/off, Reset Theme. https://support.apple.com/guide/iphone/read-books-iphc1af7c57/ios
- [V] PDFs, manga and other fixed-layout books: vertical scrolling is turned on from the Appearance button. Same page.
- [V] Line Guide is a separate feature turned on from the Menu, not from Themes & Settings. Same page.
- [V, TidBITS, iOS 16] Six preset themes bundle font, size, colour and background; themes are the only way to change background colour. Text size, scroll vs page flip, and light/dark are separate. Light/dark can follow the system or the ambient light sensor. Accessibility & Layout added line, character and word spacing, justification and multiple columns. Vertical scrolling became the default and the page-flip animation was removed. https://tidbits.com/2022/10/03/apples-books-ios-16/
- [unverified] Full theme list and font list for 2026; whether columns, page-turn animations and the iOS 26 state are unchanged; per-book vs global scope. Apple's page does not say. TidBITS commenters report font settings sometimes reset and brightness can no longer be set apart from system brightness.

### Imported EPUB, PDF, audiobooks
- [V] Mac accepts PDFs, MP3 audiobooks, Audible audiobooks and "many EPUB"; ACSM, AZW and ODM files cannot be opened and need their own apps. Import via File > Import or drag and drop. The page says PDFs open in Preview or the default PDF reader on Mac (possibly outdated). Imported items reach other devices through iCloud for Books. https://support.apple.com/guide/books/ibkseed72068/mac
- [V, secondary] Imports must be DRM-free. If iCloud sync for the library is on, an imported EPUB uploads and appears on iPhone and iPad. If you only want store books synced, leave iCloud Drive off (so own files and store books share one sync switch). Source: MacRumors forum thread via search [S], https://forums.macrumors.com/threads/books-app-why-so-hard-to-put-your-epub-in-it.2182455
- [V, Allan Tépper, Jul 2024] Non-DRM EPUBs behave the same on macOS, iOS, iPadOS. M4B audiobooks open on Mac with cover, metadata and chapters, but on iPhone/iPad opening or downloading an M4B does nothing and there is no Import/Open function; he recommends another app for M4B on iOS. Apple has no Books app for Windows or Android. https://provideocoalition.com/apple-books-app-inconsistent-among-formats-platforms/
- [V, TidBITS iOS 16 comments] Metadata editing is limited, especially on Mac; commenters edit metadata in another tool first. Series sorting is missing; author and title sorting seen as unreliable; collections harder to find. Imports done by double-clicking on a Mac (with iCloud sync) or opening from Files on iOS.
- [S] Mark as Finished: from the ellipsis menu on a title; works for imported or outside-read books; the Finished collection fills automatically when you reach the end; Mark as Still Reading reverses it. Want to Read is manual (also filled from store wishlist). https://support.apple.com/guide/books/ibks33867842/mac ; https://discussions.apple.com/thread/254703939
- [unverified] What happens to the original file (copied into the Books container is likely but not read); size limits; series grouping for imported books.

### Highlights, notes, bookmarks
- [V, iPad guide] Touch and hold a word, adjust selection, Highlight, then choose a colour or underline. Tap a highlight to change colour or Remove Highlight. Add Note on a selection. View all via Menu > Contents and Highlights; tapping an item jumps to the spot. Share one item or several (Checklist) via the share sheet. Delete singly or in bulk. Translate, search, copy from the selection. https://support.apple.com/guide/ipad/annotate-books-ipade2f8027b/ipados
- [S] Share via AirDrop, Messages, Mail. Highlight colour count not listed in what I read [unverified].
- [V, TidBITS iOS 16] Double-tap creates a bookmark (easy to trigger by mistake); sharing all highlights at once was not possible; the colour menu sometimes fails to show. A "Bookmarks & Highlights" list sits in the reading menu.
- [V, Readwise] Books does not create standard PDF highlights, so PDF highlights cannot be exported reliably; EPUB highlights export is hard (email from iPhone/iPad "Bookmarks & Highlights", or read the Mac database). https://docs.readwise.io/readwise/docs/importing-highlights/apple-books
- [S] Text selection, annotation and search work for EPUB but not PDF or comics (forum). iCloud syncs highlights and notes.
- [unverified] Store quotation limits when copying/sharing, PDF markup tools, whether PDF markup syncs.

### Reading goals
- [V, Mac guide] Books tracks minutes read per day and books or audiobooks finished per year; counts consecutive days the daily goal is met and notifies on a record; yearly goal appears after the first finished book; Adjust Goal for each; Reading Goal Coaching and Completion notifications are separate switches; Books > Settings > General > Enable Reading Goals; Clear Reading Goals Data erases time and streak data. Streak lives under Reading Goals on Home. https://support.apple.com/en-ge/guide/books/ibks7c41beea/8.0/mac/26
- [S] Default daily goal 5 minutes. Year in Review needs at least three finished titles; with goals on it shows days goal met, longest streak, average monthly minutes. A "Books Read This Year" collection appears after finishing a book or audiobook. Year in Review launched Dec 2023 (marketscreener summary).
- [unverified] What counts as reading (page-turn timing, idle cut-off), PDF and audiobook treatment, history editing (only "Clear data" and Mark as Finished/Still Reading exist as far as I saw; a community tip says marking physical books Finished feeds Year in Review: https://matthewcassinelli.com/apple-books-year-in-review-physical-books-mark-as-finished/ [S]).

### Progress display
- [V, TidBITS] Progress indicator shows pages left in the chapter and current page of total (iOS 16). Real publisher page numbers appear only when the EPUB provides them. Generated page counts change with font size [unverified in an Apple page, standard for reflowable text]. Speak Screen reads the pages-left number aloud before each page (AppleVis forum, 2025) [S].

### Audiobooks
- [V, iPhone guide iOS 14] Speed control bottom-left (1x normal; the list of speeds not stated on the page), skip intervals configurable in Settings > Books > Audiobooks, sleep timer duration, chapter list (some titles say tracks or have none), drag playhead with a grey marker to return to the start point, cellular streaming warning, Playback Destination button. Linking text and audio editions: not covered [unverified]. https://support.apple.com/guide/iphone/listen-to-audiobooks-iphac1971248/14.0
- [S] CarPlay has a "Play audiobooks" topic; steps not read.

### Accessibility
- [S] No Apple page found on store-page accessibility metadata. AppleVis threads describe Speak Screen stopping at chapter end and hiding its controls, scrolling mode breaking VoiceOver continuous read, and a braille user skipping two pages in scroll mode (Sep 2025). https://www.applevis.com/forum/ios-ipados/apple-books-speak-screen
- [unverified] Assistive Access behaviour.

### iOS 18, iOS 26, macOS 26 changes
- [V] MacRumors' iOS 26 top-100 list has no Books item (Preview on iPhone gets PDF markup/AutoFill, not Books). https://www.macrumors.com/2025/09/16/ios-26-top-100-changes/
- [S] 2025 Year in Review is in the Home tab. On Mac the sidebar item is now "Home" (was Reading Now), seen in the macOS 26 guide URL.
- [unverified] Any AI summaries or recaps (none found), library redesign, new collection behaviour.

### Family Sharing, hidden purchases
- [unverified] Not reached (page-fetch budget spent on higher-value topics, Apple search returned no page).

### Repeated complaints (2024-2026, from Apple Community and forum threads; Reddit not retrievable through search)
Evidence is thin; most threads are older. [S] unless noted.
1. Library vanishes or empties when iCloud sync for Books is turned off or reset. https://discussions.apple.com/thread/256222592 , https://discussions.apple.com/thread/250331732
2. Collections do not sync between devices or have wrong counts. https://discussions.apple.com/thread/253576554
3. Collection contents get scrambled ("To be read" mixed with others).
4. Sorting by author or title unreliable; no series sorting; series clutter on the library screen. [V comments, TidBITS]
5. Importing EPUBs is hard on iOS; no import on iPhone/iPad for M4B. [V]
6. Metadata editing is minimal. [V]
7. Highlights and notes are hard to export; no bulk share; PDF highlights not exportable. [V]
8. Accidental bookmarks from double-tap; colour menu failures. [V]
9. Books offloaded with no "keep downloaded" option; position lost; font settings reset. [V]
10. Speak Screen/VoiceOver reading problems in scroll mode and at chapter ends. [S]
Also iCloud sync re-enabling after OS updates [S].

## 2. Data the app must store
- Per-title reading state: Want to Read, Reading, Finished, with explicit Still Reading, and a user-set finished date separate from progress (to log books read on paper).
- Reading-time log: per session start, end, active seconds, source (text, PDF, audio), edit/delete rights, and a clear-all action. Daily goal minutes, yearly count goal, streak and best streak, coaching/notification flags.
- Per-book vs global reader settings: theme, font, size, bold, spacing (line, character, word), margins, columns, justify, scroll vs paged, Line Guide, with "reset theme" and separate defaults for fixed-layout books.
- Annotation records: range anchor (not page number), colour or underline style, note text, created/modified times, deleted flag (to sync deletions), format of origin (EPUB vs real PDF annotation object), export history.
- Bookmarks as their own type, with creation method (manual or double-tap) to allow undo.
- Progress model: chapter pages left, total generated pages by layout settings, publisher page-list when present, plus a layout-independent locator.
- Audiobook: chapter or track list, speed, skip seconds each way, sleep timer, last start-point marker for "jump back".
- Linkage record between a text edition and an audio edition of one work.
- File provenance: original path, import date, whether copied or referenced, DRM status, rejected-format reason (ACSM, AZW, ODM).
- Per-account sync scope: store content vs own files vs annotations as separate switches.

## 3. Requirements existing products get wrong
- Never lose the library when sync is turned off; make sync removal non-destructive and explain it first.
- Series, author, title sort and series grouping for imported files as first-class.
- Full metadata editing for imports, with bulk edit.
- Real import on every platform including audiobooks (M4B) and EPUB.
- Exportable highlights and notes in bulk, including PDFs, using standard annotation objects.
- Editable reading history and a way to log paper books; goals that state what counts as reading.
- Keep-downloaded pin; per-book settings that do not silently reset.
- Bookmarking by deliberate action, with undo.
- Screen-reader reading that does not announce page counters and survives chapter boundaries.

## 4. Open questions
Apple's per-book scope for reader settings; highlight colours and quote limits; PDF markup tools and sync; audiobook speed range and CarPlay behaviour; text/audio linking; Assistive Access; Family Sharing and hidden purchases; iOS 18/26 Books changes and AI features; Reddit complaint ranking; original-file handling on import.

## 5. Source URLs opened (fetched)
- https://support.apple.com/guide/iphone/customize-the-appearance-of-books-iph1e9cd3f1d/ios (TOC only)
- https://support.apple.com/guide/iphone/read-books-iphc1af7c57/ios (two reads, one offset)
- https://support.apple.com/guide/ipad/annotate-books-ipade2f8027b/ipados
- https://support.apple.com/guide/iphone/set-reading-goals-iph5ff2b0d33/ios (wrong slug, TOC only)
- https://support.apple.com/en-ge/guide/books/ibks7c41beea/8.0/mac/26
- https://support.apple.com/guide/books/ibkseed72068/mac
- https://support.apple.com/guide/iphone/listen-to-audiobooks-iphac1971248/14.0
- https://www.macrumors.com/2025/09/16/ios-26-top-100-changes/
- https://tidbits.com/2022/10/03/apples-books-ios-16/
- https://provideocoalition.com/apple-books-app-inconsistent-among-formats-platforms/
- https://docs.readwise.io/readwise/docs/importing-highlights/apple-books
- Blocked (403, not retried): https://www.oreilly.com/library/view/take-control-of/9781990783753/text/ch008.xhtml
Search-only [S] sources are cited inline.
