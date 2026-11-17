ISSUES

Reason: www.amazon.com help pages returned HTTP 503 on all four tries (one retry each at most; I did not work around it). I read the same Amazon help nodes through Amazon-owned mirror hosts (digprjsurvey.amazon.com and .co.uk) that search returned. The web-search budget ran out before I could cover My Clippings.txt internals, Kindle Translate reader labelling, Active Canvas details and Reading Insights from a primary page. Those are marked. No calibre code was seen. Some search result lists included calibre-related pages (a bug tracker, plugin names); I did not open them and used only plain-language statements from other pages.

Marks: [V] = read on a page I opened. [U] = unverified (search snippet, single comment, or my inference).

## 1. Findings

### 1.1 Send to Kindle routes, types, limits
- Routes on Amazon's help page: Send to Kindle for Web (upload, documents 200 MB or smaller; turn on "Add to your library" so any linked device can download), the Kindle app on iOS and Android, email to the account's Send to Kindle address (each compatible device and app has its own address), desktop apps for Mac and Windows, and a Chrome extension for web articles. [V] https://digprjsurvey.amazon.co.uk/csad/help/node/G5WYD9SAF7PGXRNA
- Types on that page: HTML/HTM, RTF, TXT, JPEG, GIF, PNG, BMP, PDF, EPUB. [V] same page. The notes-support page and the USB page add DOC and DOCX. [V] https://digprjsurvey.amazon.com/csad/help/node/TNtSm13k3txFI4EvDV and https://digprjsurvey.amazon.com/csad/help/node/TCUBEdEkbIhK07ysFu
- Email limits: total size 50 MB or less, up to 25 attachments per email, send from up to 15 approved addresses; ZIP files are opened and converted automatically. Each sender must be on the Approved Personal Document Email List in Manage Your Content and Devices. [V] https://digprjsurvey.amazon.co.uk/csad/help/node/G7NECT4B4ZWHQ8WV
- A third-party guide says mail from an address not on the approved list is silently dropped, so approve first. [U, search snippet]
- Conversion: ZIP contents are converted to Kindle format on email [V]. The 2018 eBook Reader explainer says EPUB can be sent and treats the result like a regular book for sync of notes and highlights; it is old (January 5, 2018) and its EPUB-renamed-to-PNG trick is obsolete now that EPUB is accepted. [V] https://blog.the-ebook-reader.com/2018/01/05/kindle-personal-documents-service-explained-easily-send-books-to-your-kindle
- "Archive in library": the help page only links to "Disable Document Archiving in Your Kindle Library" with no text. [V as absence]. What archiving does (keeps a cloud copy so the file can be redownloaded to any device) is [U, inferred from the "Add to your library" wording and a user's question on the 2018 blog].
- Amazon's own Voyage manual (older) says sync of notes, highlights, bookmarks and last page read applies only to personal documents archived in Kindle format; PDF and TXT read natively. [U, search snippet from manualslib]
- Which personal-document types support notes, by app, from Amazon's table [V]: Kindle Scribe: handwritten sticky notes, typed notes, Active Canvas, highlights, underlines, outlines, margin side panel. Other e-readers: highlights and typed notes only. iOS without stylus: sticky notes read/delete only. Android with stylus: full set. Kindle for Mac and PC: typed notes and highlights. Kindle for Web: no personal-document support at all. Handwriting is not supported in PDFs loaded over USB-C or sent before 11/11/2022, nor in EPUB comics, manga or children's books. https://digprjsurvey.amazon.com/csad/help/node/TNtSm13k3txFI4EvDV
- Send to Kindle for Word (Microsoft 365 integration) stopped working February 9, 2026 on Web, Win32 and Mac. That is Microsoft's integration, not Amazon's Send to Kindle app. [V] https://tech.slashdot.org/story/26/01/09/1516254/
- Personal documents defined by Amazon as "files such as PDFs and Word documents that can be added to your Kindle library". [V] https://digprjsurvey.amazon.co.uk/csad/help/node/TFTaJVr74SmBfVs4kD
- Readwise's page: highlights from personal documents do not appear on read.amazon.com/notebook, which lists only store-bought or borrowed books; they must be exported by email from the Kindle app notebook, or taken from My Clippings.txt. [V] https://docs.readwise.io/readwise/docs/importing-highlights/kindle

### 1.2 USB copy to a current Kindle
- Amazon's USB page: Windows needs no extra app and shows the device as "Kindle" or "Kindle Scribe" in File Explorer. Keep the full path under 256 characters; do rename and extract on the computer, not on the Kindle. [V] https://digprjsurvey.amazon.com/csad/help/node/TCUBEdEkbIhK07ysFu
- The same page says Kindle Scribe and Kindle models released in 2024 need a separate app for USB transfer on Mac: the Send to Kindle app for Mac, with USB File Manager (Intel and Apple M1 to M3, macOS 12 or later). It supports drag and drop only and not more than 2,000 files at once; files live in the "Documents" folder. [V] same page. The page has no Linux guidance. [V as absence]
- Android Authority (October 28, 2024): 2024 Kindles use MTP; macOS lacks MTP drivers, and Amazon names Android File Transfer and OpenMTP as free options. The "USB removed" rumour was false; only the website's "Download & Transfer via USB" for store purchases was removed. [V] https://www.androidauthority.com/2024-kindle-sideloading-3494778/ (The Android Authority text and Amazon's page differ on which Mac tool Amazon names; I could not settle which is current.)
- A reader on the eBook Reader blog (2025) says a 2024 Kindle works with a third-party Linux manager once unmounted from the system. [V, comment, single user] https://blog.the-ebook-reader.com/?p=26845
- Which types a current Kindle opens when copied by USB (EPUB, AZW3, MOBI, KFX, PDF, TXT): not settled. Amazon's USB page lists EPUB, PDF, DOC and others as sendable by Send to Kindle, which is a conversion service, not a USB statement. Search snippets say a USB-copied EPUB does not show in the library and that AZW3/KFX/PDF/TXT do. [U] Amazon's older Paperwhite guide lists downloadable types .AZW, .AZW1, .AZW3, .KFX, unprotected .MOBI, .PRC and .TXT. [V, but the guide is old] https://d1ergij2b6wmg5.cloudfront.net/EM10/Kindle_User_Guide_en-US.pdf
- Sideloaded books vanish: books copied by USB can be wiped after a long period in airplane mode followed by turning Wi-Fi on; Send to Kindle books are unaffected; stated as persisting for years (first post February 1, 2024; still listed December 2024; June 2025 commenters still ask for a fix). [V] https://blog.the-ebook-reader.com/2024/12/19/kindle-bugs-list-common-software-issues-and-how-to-fix-them and https://blog.the-ebook-reader.com/2025/02/25/amazon-keeps-finding-new-ways-to-anger-kindle-customers/ . One search snippet says books with a "personal document" type tag or no metadata are not deleted. [U]
- Covers on sideloaded books: Book Riot (August 14, 2023) says only store purchases are guaranteed to show covers on the lock screen; sideloaded files need an embedded cover plus metadata and sometimes a refresh of the device's cached metadata after sending. [V] https://bookriot.com/how-to-fix-missing-sideloaded-ebook-covers-on-kindle/ . Commenters (December 2024) say Send to Kindle shows covers automatically except for PDFs, and that a manager tool fixes covers but fails on Colorsoft. [V, comments] Another claim: a missing valid ASIN makes the device replace the cover with a generic thumbnail when online. [U]
- Amazon removed the website's "Download & Transfer via USB" on February 26, 2025 [V as stated by a third-party site, not Amazon]. EPUB/PDF download for eligible DRM-free purchases began January 20, 2026; borrowed books excluded. [V third-party; not confirmed on an Amazon page] https://www.itechguides.com/the-end-of-kindle-downloads-what-amazons-changes-actually-mean-for-your-digital-library/

### 1.3 Collections and series
- Cloud Collections (older Paperwhite guide): stored in the cloud and synced to every device and app on the account that supports them; unlimited items per collection; items can be in several collections; books and personal documents can be added; Audible content cannot. Deleting a collection removes it from the cloud and every device but not the books. Downloading a collection to a device is a device-specific setting not saved on deregister. The Whispersync for Books setting backs up annotations, last and furthest page read, and collections. [V] cloudfront guide above.
- Sideloaded books and collections: users report collections of sideloaded books fall back to "uncollected" and do not sync reliably, and moving sideloads between devices loses collections. [U, MobileRead snippets]
- Group Series in Library setting (firmware 5.13.4, December 2020) works for store content, per one tool's documentation; users say sideloaded books do not group even with series metadata. [U, search snippets] Amazon's 2026 Recaps text refers to a "series grouping" menu on devices. [V] https://www.aboutamazon.com/news/books-and-authors/kindle-recaps-feature-ebook-series-refreshers
- Household: filters "My Items" and "Shared Items" appear once sharing is on. [V, guide]

### 1.4 Notebook export and My Clippings
- Kindle Scribe sharing: Share icon, then Quick send to the registered email or Share via email; link works 7 days; sticky notes go at the end of the shared document; page markups and highlights stay only in the original; "Convert to text" gives a searchable PDF; you cannot share several notebooks at once; you cannot share notebooks or personal documents with Amazon Family members. No page limit stated. OneNote, Google Drive and OneDrive export only on Scribe models released in 2025 or later. [V] https://digprjsurvey.amazon.com/csad/help/node/TJE2UYmdw0ppUuR3Rs
- Book highlight export limit: publishers set a "copyright export limit", usually 10% to 20% of the text; excess is truncated, with a warning on the notebook page; fragments not starting or ending with punctuation get an ellipsis. [V] https://docs.readwise.io/readwise/docs/importing-highlights/kindle . The device-side message is "Unable to save clipping. You have reached the clipping limit for this item." and limits are per book, some books have none, and Amazon does not publish the limit. [V, 2009 post, old] https://burningbird.net/kindle-clipping-limits/
- My Clippings.txt: every highlight made on a Kindle device goes into this file on the device; needed mainly for non-Amazon documents. [V] Readwise page. Notes and bookmarks also go there per Amazon's older guide, backed up to the cloud when Whispersync for Books is on. [V] guide. Entry format, duplicate behaviour for edited or deleted highlights and language-dependent labels: not read from a primary page. [U]

### 1.5 2024 to 2026 features, as Amazon describes them
- Recaps: "Previously on" style refreshers for series you have read; US only; Kindle devices, Kindle iOS app, and series pages on Amazon.com in a browser (July 2026 update); supported books in a series, bought or borrowed; spoiler-avoiding. [V] https://www.aboutamazon.com/news/books-and-authors/kindle-recaps-feature-ebook-series-refreshers
- Story So Far: recap of storylines and character arcs up to where you are, thousands of popular fiction titles, US, Kindle devices and iOS, July 2, 2026 update. [V] same page.
- Ask this Book: answers about plot, characters, themes; limit to current position or whole book; ask from a highlighted passage; thousands of best-selling English ebooks, bought or borrowed; US only; Kindle iOS only, with Kindle devices and Android "later this year". The page was first published April 2, 2025 and updated July 2, 2026. [V] same page. One report says it reached iOS in December 2025. [U]
- Kindle Translate (KDP): invite-only beta, free, for eligible KDP ebooks; not on kdp.amazon.co.jp; source languages English, French, German, Italian, Portuguese, Spanish; from English into French (France), German, Italian, Portuguese (Brazil), Spanish (Spain); option to auto-translate into new languages. [V] https://kdp.amazon.com/en_US/help/topic/G9LWBTHMUDT8EQPK . How a reader sees a translation labelled: not found. [U]
- Colorsoft highlights: yellow, pink, blue, orange at launch, filterable by colour, carried to the mobile app (Amazon via search snippet). [U] Green as a fifth colour and eight bookmark colours appeared in firmware 5.19.2 (reported February 19, 2026); release notes were blank; colour sync for bookmarks not stated. [V] https://blog.the-ebook-reader.com/2026/02/19/kindle-colorsoft-now-has-colored-bookmarks-and-green-highlights/
- Scribe (Amazon page, December 4, 2024, updated April 28, 2026): AI notebook search (natural language, short AI summaries, follow-up questions); notebook summarization (up to 15 pages; short or long; professional or casual); "Refine writing" converts handwriting to a script font inside the notebook; Send to Alexa (needs Prime or Alexa+); import from Google Drive and OneDrive, write on PDFs, export annotated PDFs; export to OneNote as converted text or an embedded image; dark mode on Colorsoft models; Smart Shapes. [V] https://www.aboutamazon.com/news/devices/kindle-scribe-new-features . This page does not describe Active Canvas; a snippet says it boxes an annotation and rewraps the page and came to the original Scribe in firmware 5.17.2. [U]
- Reading Insights and Kindle Challenges: Amazon's page says Kindle books bought or borrowed count toward books read, streaks and achievements; linking Goodreads lets Goodreads Read Shelf books count; there are "mystery achievements"; tracking needs a Kindle, app or web with mobile data enabled; whether sideloaded content counts is not stated. [V] https://digprjsurvey.amazon.com/csad/help/node/TdP7Fpp3mrAY4gibL9 . Daily and weekly streaks and an opt-out are from third-party snippets. [U]

### 1.6 What a book file must contain for each feature
- Enhanced Typesetting: applied automatically to reflowable Kindle books; improves kerning, ligatures, hyphenation, line height, margins, drop caps, tables and contrast in night and sepia; it unlocks Page Flip, progressive download, high definition visuals and ragged right. Devices: e-readers 6th generation or newer; older devices get a KF8 copy. The page lists no author-side switch. Detail pages show "Enhanced Typesetting: Enabled" and "Page Flip: Enabled". [V] https://kdp.amazon.com/en_US/help/topic/GNY87A6WM6EK6YEE
- KDP tells authors that sideloading does not give an accurate preview of Enhanced Typesetting, so use Kindle Previewer. [V] https://kdp.amazon.com/en_US/help/topic/GGRXLC5USU4H67YM . Inference: a sideloaded file gets none of the store-side features. [U]
- X-Ray: English-language ebooks only; KDP generates entries (about 4 hours) from public sources, machine learning and excerpts; entries up to 1,200 characters; readers press and hold a word; once on, it stays on. [V] https://kdp.amazon.com/en_US/help/topic/G202187230
- Word Wise: English only, shown on a product page as "Word Wise: Enabled"; any Kindle e-reader 6th generation or newer and Fire 3rd generation and newer. [U, from a search snippet of Amazon's help node 201645250; page not opened because the www host failed]
- Real page numbers: KDP lists "Kindle Real Page Numbers (when available)" and has a "Page Number Guidelines" section; the ToC must not contain page numbers. The exact markup requirement was not read. [V partly]. A forum report says one bad entry makes the whole page-number set be rejected and only Arabic or Roman numerals are accepted. [U]
- Vocabulary Builder: every dictionary lookup is added automatically; list by word or book; flashcards; learning and mastered counts; turning it off stops capture. [V, older guide] cloudfront guide.
- Page Flip: peek at other pages without losing place; on select e-readers, Fire and iOS/Android apps. [V] KDP page above.
- Missing on sideloaded books: users report that after firmware 5.13.7 sideloaded DRM-free books no longer fetch Word Wise and X-Ray automatically while store books do. [U, one forum snippet]

### 1.7 Kindle apps for PC, Mac, Web
- Legacy Kindle for PC ends June 30, 2026; the replacement is Kindle for Windows 11 from the Microsoft Store; Windows 10 users can read downloaded books only; commenters say they have lost their route to DRM-free local backups, and some report downloading one book at a time in the new app. [V] https://blog.the-ebook-reader.com/2026/04/16/amazon-pulling-the-plug-on-kindle-for-pc-on-june-30th/
- Kindle for Mac is an App Store app since 2023. [V as a statement on the same blog]. Kindle for Web has no personal documents and no stylus notes. [V] notes table above.

### 1.8 Household, Family Library, Kids
- Family Library links two adult accounts for ebooks, audiobooks, apps and games; adults can share ebooks, apps and games with children; teen logins cannot share through Family Library; if sharing stops, downloaded shared content stops working; child profiles inactive for 18 months are removed. [V] https://digprjsurvey.amazon.com/csad/help/node/GEX2D6QN8PKQJAJ2
- Per-reader position and highlights across the two adults: not addressed on the page; each account keeps its own sync [U, inference]. When a child profile becomes its own account the organizer chooses whether to transfer the Kindle library including reading history, notes and highlights. [U, search snippet of Amazon family page]
- Kindle FreeTime (older guide): up to four child profiles, store and web blocked, children read only what you grant. [V, old guide]

### 1.9 Assistive Reader and VoiceView
- Assistive Reader: Kindle e-reader 11th generation and later, Scribe, Colorsoft; also Kindle apps; only books with Enhanced Typesetting; languages English, Spanish, French, German, Italian; highlights words as read; adjustable speed; skip back 30 seconds; pauses on opening navigation or creating a note. Personal documents not addressed. [V] https://digprjsurvey.amazon.com/csad/help/node/TdHCSXXqsdhaVKt3Tc
- VoiceView: gesture navigation and audio over Bluetooth only (needs a Bluetooth audio device); languages English, French, German, Italian, Portuguese, Spanish; power button three times to search for Bluetooth audio; personal documents not stated. [V] https://digprjsurvey.amazon.com/csad/help/node/202051910

### 1.10 Ten most repeated complaints (2024 to 2026; mostly blog posts and their comments; Reddit and MobileRead not opened directly)
1. Removal of Download & Transfer via USB (February 2025) and worry about backups of purchases. [V]
2. Sideloaded USB books deleted after airplane mode then Wi-Fi. [V]
3. Sideloaded files lack covers or show as documents. [V, comments]
4. Forced Send to Kindle conversion for EPUB; no native EPUB. [V]
5. DRM and lock-in; claims DRM added to "DRM-free" titles. [V]
6. Mandatory updates and regressions (5.19.2: battery, fonts, manga formatting). [U, snippet]
7. Kindle for PC shutdown and Windows 11 only. [V]
8. Highlight export caps, and personal-document highlights missing from the web notebook. [V]
9. Sideloaded books cannot group by series, and collections do not sync for them. [U]
10. Hardware: yellow light band and banding on Colorsoft, touchscreen too sensitive, no page-turn buttons, old Kindles losing store access. [V/U mixed]

## 2. Data the app must store
- Per file: source origin (store, Send to Kindle, USB sideload), a stable identity independent of path, ASIN or none, DRM state, embedded cover present, and the format family (EPUB source, converted KFX/AZW3).
- Per annotation: kind (highlight, note, bookmark, handwritten sticky note, underline, outline), colour (five highlight colours plus eight bookmark colours), page and location, whether the text was truncated by an export limit, source (device file, cloud, app), and a duplicate-link to the same highlight in other sources.
- Per device: filesystem or MTP transport, path-length limit (256), whether a computer helper app is needed, per-model notes, and a record of what the app copied (to detect vanished sideloads).
- Per book sent: route used, size, approved sender, conversion outcome, archive flag.
- Reading state: last page read and furthest page read, each with a source and time.
- Collections: membership with cloud or device scope and an item type (book or personal document).
- Series: name, index and whether the series grouping came from the file or a store.
- Feature eligibility flags per book: X-Ray, Word Wise, Enhanced Typesetting, Page Flip, real page numbers, text-to-speech.
- Per notebook: pages, export format, share link expiry.
- Household profile and family-share flags on each library item.

## 3. Requirements existing products get wrong
- Keep a local copy of every sent file and every annotation before any device sync; the Kindle can delete USB books.
- Detect vanished sideloads after each connection and warn.
- Embed and re-embed a cover and metadata on every send, and offer a refresh step for the device's cache.
- Keep highlights exportable past the 10 to 20 percent cap by reading them from the device file, and flag truncation.
- Show per-file why a feature (X-Ray, Word Wise, series group) is missing.
- Work on Linux and Mac with MTP without telling the user to buy a helper.

## 4. Open questions
- Exact behaviour of each type (EPUB, AZW3, MOBI, KFX, PDF, TXT) when copied to a 2024 or later Kindle, from a primary Amazon page or a tested device.
- What "archive in library" does and whether it is on by default.
- My Clippings.txt entry layout, duplicate and language quirks.
- Active Canvas rules; Kindle Translate reader labelling; whether sideloaded books count for Reading Insights.
- Whether Word Wise and X-Ray can ever attach to a sideloaded book without third-party tools.
- Which Mac tool Amazon names today for 2024 models.

## 5. Source URLs opened
https://digprjsurvey.amazon.co.uk/csad/help/node/G5WYD9SAF7PGXRNA
https://digprjsurvey.amazon.co.uk/csad/help/node/G7NECT4B4ZWHQ8WV
https://digprjsurvey.amazon.com/csad/help/node/TCUBEdEkbIhK07ysFu
https://digprjsurvey.amazon.com/csad/help/node/TNtSm13k3txFI4EvDV
https://digprjsurvey.amazon.co.uk/csad/help/node/TFTaJVr74SmBfVs4kD
https://digprjsurvey.amazon.com/csad/help/node/TdHCSXXqsdhaVKt3Tc
https://digprjsurvey.amazon.com/csad/help/node/202051910
https://digprjsurvey.amazon.com/csad/help/node/TJE2UYmdw0ppUuR3Rs
https://digprjsurvey.amazon.com/csad/help/node/TdP7Fpp3mrAY4gibL9
https://digprjsurvey.amazon.com/csad/help/node/GEX2D6QN8PKQJAJ2
https://www.aboutamazon.com/news/devices/kindle-scribe-new-features
https://www.aboutamazon.com/news/books-and-authors/kindle-recaps-feature-ebook-series-refreshers
https://kdp.amazon.com/en_US/help/topic/GPNJPYK298J8TRRV
https://kdp.amazon.com/en_US/help/topic/GNY87A6WM6EK6YEE
https://kdp.amazon.com/en_US/help/topic/G9LWBTHMUDT8EQPK
https://kdp.amazon.com/en_US/help/topic/GGRXLC5USU4H67YM
https://kdp.amazon.com/en_US/help/topic/G202187230
https://d1ergij2b6wmg5.cloudfront.net/EM10/Kindle_User_Guide_en-US.pdf (Paperwhite 2nd edition guide, old)
https://www.androidauthority.com/2024-kindle-sideloading-3494778/
https://blog.the-ebook-reader.com/?p=26845
https://blog.the-ebook-reader.com/2018/01/05/kindle-personal-documents-service-explained-easily-send-books-to-your-kindle
https://blog.the-ebook-reader.com/2024/12/19/kindle-bugs-list-common-software-issues-and-how-to-fix-them
https://blog.the-ebook-reader.com/2025/02/25/amazon-keeps-finding-new-ways-to-anger-kindle-customers/
https://blog.the-ebook-reader.com/2025/06/16/are-kindle-ereaders-less-appealing-now-than-ever/
https://blog.the-ebook-reader.com/2026/02/19/kindle-colorsoft-now-has-colored-bookmarks-and-green-highlights/
https://blog.the-ebook-reader.com/2026/04/16/amazon-pulling-the-plug-on-kindle-for-pc-on-june-30th/
https://bookriot.com/how-to-fix-missing-sideloaded-ebook-covers-on-kindle/
https://www.itechguides.com/the-end-of-kindle-downloads-what-amazons-changes-actually-mean-for-your-digital-library/
https://tech.slashdot.org/story/26/01/09/1516254/
https://docs.readwise.io/readwise/docs/importing-highlights/kindle
https://burningbird.net/kindle-clipping-limits/
Failed or empty: www.amazon.com help nodes G5WYD9SAF7PGXRNA, G7NECT4B4ZWHQ8WV, TJE2UYmdw0ppUuR3Rs and www.amazon.com/sendtokindle (503); tomsguide.com Colorsoft article and techradar Active Canvas article (text truncated).
