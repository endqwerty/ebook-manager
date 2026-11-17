ISSUES

Why ISSUES: several primary sites blocked or failed (gutenberg.org 503 x3 with Retry-After 3600, Amazon help 503 x2, AO3 FAQ 525, Mozilla Pocket page failed to render). I did not retry them, and I used search snippets from the sites' own pages or third-party sources instead, marked below. No calibre code was seen. No possible training recall of calibre was written down.

Marking: [V] = read on a page I opened. [S] = seen only in a search-result summary (the summary may quote a primary page, but I did not open it). [U] = unverified.

## 1. Findings

### 1.1 Public library lending
- Libby: loans return automatically on the due date, so no late fees; "Return Early" exists under Manage Loan; holds sit on the Shelf with place in line; each library chooses its titles and formats; at US libraries most ebooks can be sent to Kindle or read in Libby; loans download over Wi-Fi by default. [V] https://help.libbyapp.com/en-us/6103.htm. The page gives no loan length or hold wait.
- Libby Kindle loans (US only): returned automatically at the end of the lending period; cannot be returned early inside Libby, only through Amazon or the Kindle; a Kindle loan can linger on the Libby Shelf if the Amazon side was not returned. [S] https://help.libbyapp.com/en-us/6015.htm and 6351.htm. What happens to Kindle-side annotations: [U] (one search summary says Kindle keeps them in the Amazon account; no primary page read).
- Libby notes: bookmarks, notes and highlights are saved in the book and reappear if the same title is borrowed again. [V] https://help.libbyapp.com/en-us/6136.htm. The page does not say what happens at return, but the export page below shows they stay exportable.
- Libby export: from a title's Reading Journey you can export notes, highlights, bookmarks and circulation activity, as a web "Table" or a "Spreadsheet" (bookmarks or highlights file); highlights export includes notes. [V] https://help.libbyapp.com/en-us/6151.htm. Export works for returned titles [S]. Timeline and tags export: one search summary says tags and timeline can be exported before resetting the app, and deleted tags/timeline cannot be recovered [S]; the page I opened (6151) does not mention them, and the sync page (6070) only says library cards, loans, holds and tags copy to another device through a recovery passkey or setup code [V] https://help.libbyapp.com/en-us/6070.htm. So tags/timeline export formats: [U].
- Internet Archive controlled lending: 1-hour BookReader loans (renewable by borrowing again, no waitlist) or 14-day loans with waitlist; up to 10 books at once; 14-day loans can be BookReader, or PDF/EPUB; downloaded PDF/EPUB are LCP-encrypted and need an LCP reader (Thorium on desktop, Cantook on mobile), other formats may need Adobe Digital Editions; return from the Loans tab, otherwise automatic at expiry; no late fees; cannot extend, must re-borrow. [V] https://help.archive.org/help/borrowing-from-the-lending-library/
- Hoopla: help page loaded as a topic list only (borrow limits, loan periods, offline downloads, formats, Kindle delivery); no values. [V, empty] https://help.hoopladigital.com/ . Specifics: [U]. cloudLibrary, BorrowBox: not researched [U].
- Readium LCP: vendor-neutral DRM run by EDRLab; licence document signed with RSA/SHA-256; passphrase-based, user passphrase hashed with SHA-256 and the hash cached by readers so users are rarely prompted; the licence rights live in the licence document; a License Status Document reports status and interactions that change it. [V] https://readium.org/lcp-specs/ . Key-retrieval note: a passphrase hint is shown when the passphrase is needed; a provider can deliver the hashed passphrase inside an OPDS feed so the client opens the book without typing; for lending, a "borrow" link may return an OPDS entry instead of a direct download, and a reserved book must carry an "unavailable" status. [V] https://readium.org/lcp-specs/notes/lcp-key-retrieval.html. Exact status values and rights fields (start, end, print, copy): the status-document page returned 404, so [U].
- OPDS and library platforms: opds.io names OPDS 2.0 as the latest spec and links specs.opds.io, drafts.opds.io and the opds-community GitHub. [V] https://opds.io/ . Palace Project (Lyrasis) builds on the Library Simplified code from NYPL; its circulation manager handles patron login via the "Authentication for OPDS" document (401 plus document, bearer token after SAML or other login), mixes licensed and open-access titles, and serves OPDS feeds; ODL is an OPDS extension that tells libraries the exact licence types held; DPLA's Palace Marketplace offers OPDS+ODL. [S] wiki.lyrasis.org pages and DPLA FAQ (ebooks.dp.la/?p=26692). Full ODL licence fields (checkouts, expiry): [U].

What a manager should model for a loan: source library and card, title and format delivered (Libby app only, Kindle, LCP EPUB/PDF, Adobe-DRM EPUB), loaned-on date, due date, state (active, returned early, expired, on hold with queue position and availability), the licence file or passphrase hint if downloaded, a return action (client-side for LCP/Adobe, on Amazon for Kindle), and annotations stored by the manager itself, linked to the title identity rather than to the loan so they survive expiry and re-borrowing. Because the file becomes unreadable at the end, the manager must keep the record after the file is gone.

### 1.2 DRM-free and open sources
- Standard Ebooks: four files per book: azw3 (Kindle devices and software, not the iOS Kindle app), kepub (Kobo), compatible epub, advanced epub (for Readium-style readers, not for general use); content dedicated to the public domain under CC0 1.0; feeds: Atom New Releases, OPDS New Releases, Atom blog. [V] https://standardebooks.org/help/how-to-use-our-ebooks
- Project Gutenberg: offline catalogues page lists RSS (daily today.rss), OPDS (search.opds, XML OPDS to be sunset in 2027, a JSON OPDS2 feed in testing), pg_catalog.csv (about 20 MB, updated Nov 2026) and RDF/MARC metadata; apps must send a proper User-Agent with contact address and heavy users are told to download the catalogue and host their own feed; GUTINDEX files are not for automation. [S] https://www.gutenberg.org/ebooks/offline_catalogs.html. The site returned 503 to me three times, including the robot-access policy page [V: blocked], so the exact robot rules and formats list: [U].
- Open Library: documented APIs for search, covers, works/editions/authors, lists and reading log; monthly bulk dumps for full datasets (APIs must not be used for bulk metadata); do not scrape; 1 request/second unidentified, 3/second identified; send a User-Agent naming the app plus contact. [V] https://openlibrary.org/developers/api
- Internet Archive: see 1.1; free public-domain items separate from lending [U].
- AO3 and others in 1.4. Baen, Humble, StoryBundle: Baen Ebooks sells DRM-free and lets a buyer re-download in any format years later; Baen Free Library is DRM-free; StoryBundle states "no DRM" and ships one zip with epub and mobi; Humble ebook bundles are typically DRM-free in EPUB, PDF, MOBI. [S] third-party posts only (tomsguide, mobileread, storybundle). Tor/Macmillan, Smashwords/Draft2Digital, itch.io, Leanpub, Wikisource export: not researched [U] (the itch.io docs URL and Baen FAQ URL returned 404).
- Kobo: a DRM-free EPUB/PDF purchase can be exported straight to another device; otherwise Adobe Digital Editions is needed; Japanese customers cannot download DRM files at all; "most items" qualify, not all. [V] https://help.kobo.com/hc/en-us/articles/360019527954-Download-books-from-your-Kobo-account-to-export-to-another-device-or-app
- Which have browsable feeds: Standard Ebooks (OPDS, V), Gutenberg (OPDS, S), Open Library (JSON API, V), Internet Archive (lending, V; feeds [U]). Others: [U].

Record needs: source, original URL, per-source stable ID (Gutenberg ebook number, Open Library work/edition ID, IA identifier, Standard Ebooks slug), licence text or "public domain / CC0" flag with country caveat [U], format variant chosen (kepub vs epub), a feed subscription (feed URL, last-seen entry), and "watermarked" flag with the buyer name in the file for social-DRM files [U on specifics].

### 1.3 Stores that keep files in their own apps
- Amazon: on 26 Feb 2025 the "Download & Transfer via USB" option for purchased Kindle books was removed; books already downloaded can be moved; "Deliver or Remove from Device" and Send to Kindle still work; Kindle Unlimited titles lost USB download in 2023. [S] https://www.techradar.com/... / goodereader.com/blog/kindle/amazon-removing-download-and-transfer-on-the-kindle-feb-26th (Amazon's own help pages returned 503; Amazon's announcement text not read).
- Kobo: see 1.2 [V].
- Apple Books: user guide shows "Import books, audiobooks, or PDFs" and a highlights/notes section; no word on exporting purchased books. [V, thin] https://support.apple.com/guide/books/welcome/mac . Official export of purchases in 2026: [U].
- Google Play Books: consumer export/upload limits not found; Partner Center says PDF/EPUB up to 2 GB per file for publishers only. [S] https://support.google.com/books/partner/answer/3424254. Nook: not researched [U].
- Readwise Reader notes that books from stores without a DRM-free EPUB cannot be imported to it. [S]

"Owned without a file" record: store, store account, store product ID/ASIN/ISBN, purchase date, price and currency, order ID, deep link to open the title in the store app, region (stores are country-locked), "where is my copy" (store app, device name, shelf), DRM state (none, Adobe, Kindle, LCP, store-only), and whether a local file exists. The manager should be able to merge this stub with a file later, keeping the purchase facts.

### 1.4 Web serials and fan fiction
- AO3: a Download button on each work offers AZW3, EPUB, MOBI, PDF and HTML; no account needed; AZW3 added after the older PDF/HTML/EPUB/MOBI list. [S] https://archiveofourown.org admin posts and ao3wiki guide (official FAQ page returned 525, so exact feature statements: [U]). Series, collections, tags and subscriptions to works for update notices exist on the site [U].
- Royal Road: offline chapter download is a Premium app perk; third-party archivers exist. [S] App Store listing https://apps.apple.com/app/id6456945333 . Terms on downloading: [U].
- Wattpad: offline reading inside the app; free users limited per user reports; no official text export. [S]
- FanFiction.net: no download button; users use third-party tools that must go slowly. [S] Terms: [U]. I did not look for scraping tools' code; need only: copies are made by user tools, not an official feature.
Needs (from the user side, [U] beyond the above): chapter count known vs. total, status (in progress, complete, hiatus, abandoned), last update date, new-chapter notice, re-fetch that keeps reading position and highlights, fandom/relationship/character/warning/rating tags, series with order, author's pseudonyms mapped to one author, "gift" and "collection" membership, the work URL as stable identity.

### 1.5 Read-later and the open web
- Pocket: stopped saving 8 July 2025, export-only afterwards; export deadline reported as 8 October 2025, with one source saying it was extended to 12 November 2025, then deletion. [S] https://www.techradar.com/computing/websites-apps/pocket-shuts-down-today-heres-how-to-get-your-data-before-the-app-closes-plus-the-3-best-alternatives . Mozilla's own page failed to load, so dates are [U].
- Omnivore: hosted service ended November 2024, export cutoff 15 November in most sources; code is open source; destinations named: Readwise Reader, Matter, Wallabag, Instapaper. [S] https://buttondown.com/sethmlarson/archive/omnivore-is-shutting-down-soon-migrate-data-and
- Wallabag imports Pocket, Instapaper, Readability, Pinboard, elCurator and its own v1/v2 files, with a command-line import. [V] https://doc.wallabag.org/en/user/import/ . Export formats: the page does not say [U].
- Karakeep (was Hoarder): imports Pocket, Omnivore, Chrome, Linkwarden; hourly RSS pulls with dedupe; lists publishable as RSS; optional full-page archive; REST API, CLI. [S] https://docs.karakeep.app/integrations/rss-feeds/ . EPUB export: [U].
- Readwise Reader: holds articles, newsletters, EPUBs, PDFs; imports from Pocket, Instapaper, Feedly; RSS "Feed" vs saved "Library"; upload limit 500 MB (10 MB Markdown); exports documents CSV, feeds OPML, files zip, highlights Markdown; Send to Kindle for single items or daily/weekly digests. [V for first bullet, S for limits and export] https://docs.readwise.io/reader/docs
- Instapaper: help site did not resolve [U].
Lesson: hosted services die; a library that points at a service without keeping a local copy loses content. Store the extracted article (HTML/EPUB) and the original URL.

### 1.6 Academic and professional documents
- Zotero: free tier is 300 MB of file storage; item metadata is not limited; linked files avoid the quota but do not sync to Zotero servers or mobile; WebDAV can sync personal-library files. [V] https://www.zotero.org/support/sync ; [S] forum posts on linked files and on "retrieve metadata" working only on top-level PDFs with identifiers such as a DOI.
- arXiv, publisher PDFs: not researched [U].
Hand-over rule suggestion: a book manager should hold books and handle a paper only as a file link plus DOI/arXiv ID; offer export (BibTeX/RIS/CSL JSON [U]) or "open in Zotero" rather than copying, so citations, collections and annotations stay in one place.

### 1.7 Physical books
- StoryGraph FAQ page is a placeholder listing Goodreads/CSV import, export, barcode scanning, owned-format tracking, reading status and tags. [V, thin] https://www.storygraph.com/faqs . Libib, BookBuddy, Bookshelf not read [U].
Record: ISBN (barcode), edition, "owned in print", shelf location, condition, lent to (person, date, due, reminder), cover photo.

### 1.8 Wish lists, gifts, sharing
Not researched on primary pages [U]. Fields implied: status "wanted", target store, price seen with date, pre-order release date, gift from/to, message, redeem state. Household sharing (Apple Family Sharing, Amazon Household, Google family library, Kobo, library cards shared in a family) is [U].

## 2. Data the app must store (beyond first-round list)
Per acquisition (one book may have several): source kind (library loan, free download, purchase, bundle, gift, self-made, web capture, feed), source name and account, original URL, store/source product ID, acquired date, price, currency, order reference, licence terms (CC0/public domain note, personal-use, watermark), DRM kind, file present or not, "where is my copy", region.
Per loan: library, card, due date, state, hold queue position, licence/passphrase hint, return method, annotations kept independent of the file.
Per feed or serial: feed URL, type (OPDS, RSS/Atom, site), last checked, last seen entry, chapter count, status, last update date, per-source ID.
Per physical copy: print flag, location, lent-to record.
Per wishlist item: target price, price history, release date.
Identity: ISBNs, store IDs, source IDs, Open Library/Gutenberg/IA IDs, so a loan, a purchase stub and a file merge into one work.
Rate-limit and User-Agent settings for each source (Open Library: 1/s unidentified, 3/s identified; Gutenberg: contact address in User-Agent).

## 3. Requirements existing products get wrong
- Keep annotations and the book's record after a loan ends, and re-attach them when the title is borrowed again (Libby only exports them per title and only for returned loans).
- Show the due date on every loaned title everywhere, across libraries, and warn before expiry; never silently drop an expired file entry.
- Let a title exist without a file (purchase stub) and be merged later.
- Honour each source's access rules in code: User-Agent with contact, one feed fetch, cache, prefer catalogue dumps over crawling.
- Keep a local copy of web articles and serials, as services such as Pocket and Omnivore closed.
- Treat sunsetting feed formats (Gutenberg XML OPDS in 2027) as migration work: support OPDS 2.0 JSON first.

## 4. Open questions
- Libby timeline and tags export format and what is in it; whether Kindle annotations from a library loan survive.
- Hoopla, cloudLibrary, BorrowBox limits and formats.
- LCP status document values and rights fields; ODL licence fields.
- Gutenberg robot policy and format list (site blocked).
- AO3, Royal Road, FanFiction.net, Wattpad official terms on downloading and update features.
- Apple Books, Google Play Books, Nook official export in 2026.
- Household sharing rules for each store; gift mechanics.
- Whether the Pocket deadline was extended (Mozilla page unreadable).

## 5. Source URLs opened
Opened and read: help.libbyapp.com/en-us/6103.htm; /en-us/6070.htm; /6151.htm; /en-us/6005.htm; /en-us/6136.htm; standardebooks.org/help/how-to-use-our-ebooks; readium.org/lcp-specs/; readium.org/lcp-specs/notes/lcp-key-retrieval.html; opds.io; openlibrary.org/developers/api; doc.wallabag.org/en/user/import/; help.archive.org/help/borrowing-from-the-lending-library/; docs.readwise.io/reader/docs; zotero.org/support/sync; support.apple.com/guide/books/welcome/mac; support.google.com/books/answer/3165723; help.kobo.com (article 360019527954); storygraph.com/faqs; help.hoopladigital.com (topic list only); archive.org/details/inlibrary (title only).
Failed: gutenberg.org robot_access, help/mirroring, offline_catalogs (503); amazon.com help (503 x2); archiveofourown.org FAQ (525); support.mozilla.org future-of-pocket (page error); readium lsd note (404); baen.com FAQ (404); itch.io docs (404); zotero books (404); readwise exporting FAQ (404); help.instapaper.com (DNS).
Search summaries only: the other URLs cited with [S].
