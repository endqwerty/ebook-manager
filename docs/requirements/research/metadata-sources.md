ISSUES

Many primary pages returned 403/503 or bot walls (Hardcover docs, ISBNdb, Metron docs, AniList rate page, Gutenberg robot/offline pages, NCBI, id.loc.gov, LibraryThing, ISFDB, ISBN agency manual). I did not retry or work around them. Evidence tags: [V] = I opened the page and read the extract; [S] = only a search-result summary of the official page (page itself not opened); [U] = unverified or from general knowledge. No API calls were made. No calibre code seen. No "possible training recall of calibre's ..." items arose.

## 1. Findings

### Google Books
- Books API terms page (last modified 2012): you may not charge users any fee for using your application without Google's permission; must remove infringing content on request and publish a contact address for rights holders; must tell users that content they submit may become public on Google services. It incorporates the Google APIs Terms of Service. [V] https://developers.google.com/books/terms
- Google APIs ToS [V, extract of the page] https://developers.google.com/terms : do not keep cached copies longer than the cache header allows (5(e)(1)); do not "scrape, build databases, or otherwise create permanent copies"; delete cached content on termination; display any attribution the API docs require; no implied Google endorsement; Google sets and enforces limits and you may not circumvent them; keep credentials confidential and make reasonable efforts to stop others using them; "credentials may not be embedded in open source projects".
- Consequence: one shared key shipped in an Apache-2.0 app is not allowed. Each user must create their own key (free, Google Cloud console) and enter it in settings. Google Books can only be a transient lookup: show the result, let the user copy values into their own record, do not build a local mirror of Google data. [V for the clauses; the reading of them is mine.]
- Using the API: public-data requests need an API key or OAuth token; key goes in `key=`; imageLinks has six variants (smallThumbnail, thumbnail, small, medium, large, extraLarge) with zoom values 5,1,2,3,4,6; no pixel sizes, no default daily quota, no display or attribution rules on that page. [V] https://developers.google.com/books/docs/v1/using . The commonly quoted default of 1,000 requests/day per project is [U].

### Hardcover (pages blocked; facts from search listings of docs.hardcover.app)
- GraphQL endpoint https://api.hardcover.app/v1/graphql. [S]
- Limits: 60 requests/minute, 30 s timeout, max query depth 3 (directory pages, [S] weak). The Getting Started summary adds daily caps (free 5,000, supporter 50,000), small burst limits, 429 at the cap, daily reset at midnight UTC, header X-RateLimit-Daily-Reset, 401 for bad/expired token, each top-level field costing one token and aliases counting separately, and advice to use OAuth rather than personal tokens for anything a user signs into. [S] https://docs.hardcover.app/api/getting-started/ . Treat numbers as unconfirmed.
- Data: books schema lists editions, editions_count, contributions, book_series, cached_tags, taggings, rating (0 to 5), ratings_count; separate schemas for series, publishers, user_books, lists, reading journals, countries; search guide mentions top 5 genres and top 5 moods. [S] https://docs.hardcover.app/api/graphql/schemas/books/ . Whether third-party apps are allowed and the data licence: not found. [U] Hardcover is the only source found with moods and a user-facing community rating plus per-user library API; it needs a per-user token.

### ISBNdb (page blocked; search summaries of official pages)
- Plans: Basic $14.99/mo (5,000 calls/day, 1 req/s, 100 ISBNs per bulk call), Premium $35.99 (15,000/day, 3 req/s, 1,000 per bulk), Pro $99.99 (50,000/day, 5 req/s, real-time prices); annual $149/$359/$999. No free tier. Terms: auto-renewal, price change with 30 days' notice. [S] https://isbndb.com/isbn-database , https://isbndb.com/faq . Data-storage clauses of https://isbndb.com/terms-and-conditions not read (403). [U]
- Dated coverage comparison of Google Books vs ISBNdb vs Open Library: none found. Only vendor blog claims. [U]
- Verdict: paid, user's own key only, never default.

### BookBrainz
- Data licence: core data (BBIDs, aliases, types, dates, languages, status, disambiguation, relationship ids/types) is CC0; other public data CC BY-SA 4.0. [V] https://bookbrainz.org/licensing
- Site is labelled Alpha software; has a web service with live docs; says it holds "every book, magazine, journal or other publication". [V] https://bookbrainz.org/about . Entities named in the licence page: author, edition group, edition, publisher, work (series is also an entity on the site, [U]). Alpha, small coverage: use as optional, low priority.

### Open Library
- Dumps (monthly): editions ~9.2 GB, works ~2.9 GB, authors ~0.5 GB (gzip text), "all types" ~12.4 GB, complete incl. history ~29.6 GB; ratings ~5 MB, reading log ~65 MB, redirects, deletes, lists, covers metadata ~70 MB, Wikidata ~700 MB (authors only). Format: tab-separated, columns type, key, revision, last_modified, then full record JSON. No rolling cover dump, only historic archives. No licence stated on the dump page. [V] https://openlibrary.org/developers/dumps
- Licensing page: Internet Archive "does not assert any new copyright or other proprietary rights"; rights issues on contributions may exist; nothing specific for data, covers, dumps or API. [V] https://openlibrary.org/developers/licensing . Open Library data is widely described as CC0 but I did not read that statement. [U]
- API page: append .json/.yml/.rdf to any key; do not bulk-download via APIs (use dumps); do not scrape HTML; cache; send User-Agent with contact email for 3 req/s instead of 1; do not spread requests over IPs; not meant for high-traffic commercial use. [V] https://openlibrary.org/developers/api
- Covers: https://covers.openlibrary.org/b/{isbn|oclc|lccn|olid|id}/{value}-{S|M|L}.jpg ; author photos under /a/; ?default=false returns 404 not a blank; lookups by anything except cover ID or OLID limited to 100 per IP per 5 minutes (403 beyond); "do not crawl"; bulk cover tars on archive.org (~10,000 images each). No copyright statement. [V] https://openlibrary.org/dev/docs/api/covers
- Field-level model (work, edition, author fields) not documented on the pages I could open; the JSON record in the dump is the schema. Edition/work/author split is standard: work holds title, authors, subjects, first publish; edition holds ISBNs, publishers, pages, format, language, covers. [U]
- Offline index plan: dump total for editions+works+authors is ~12.6 GB gzipped; build a local index once per month on user request. Sizes only [V].

### Library of Congress, VIAF, ORCID
- loc.gov JSON API: no key; rate limiting enforced, specific numbers on a "Working Within Limits" page I did not open (round 1 said 20/min); results capped at 100,000 items deep; no rights statement on that page. [V] https://www.loc.gov/apis/json-and-yaml/
- id.loc.gov (blocked): technical centre offers compressed bulk downloads (RDF/XML, N-Triples; MADS/RDF, SKOS) for LCSH, LCNAF and others; Wikipedia lists the content licence as public domain but I found no LC statement. [S] https://id.loc.gov/techcenter/
- MARC relator list: maintained by LC, downloadable as RDF/JSON/MADS/SKOS from id.loc.gov/vocabulary/relators; page has no licence text. [V] https://www.loc.gov/marc/relators/ . LC works are generally public domain as US government works; relators specifically [U].
- VIAF: data under ODC-By with canonical VIAF URIs accepted as attribution; OCLC refurbished web and API interfaces in January 2025 and a Dec 2025 paper raises governance concerns; I could not learn what changed in access terms. [S] https://oclc.org/developer/api/oclc-apis/viaf.en.html . Plan: treat VIAF live API as unreliable; use Wikidata/LC/ISNI-less matching; keep a VIAF id field only.
- ORCID public API: for non-members; register, verify email, accept Public API ToS; sandbox available; page gives no rate limit and no data licence (the CC0 on the page covers the website text only). [V] https://info.orcid.org/documentation/features/public-api/ . ORCID matters for academic books/papers only.

### ISFDB, Goodreads, LibraryThing, StoryGraph
- ISFDB: data CC BY 4.0, software BSD; complete MySQL backups (~315 MB compressed, ~1.4 GB unpacked, latest seen 2024-07-20) plus cover-image backups and a download script; no live API. [S] https://isfdb.org/wiki/index.php/ISFDB_Downloads . Best for SF/fantasy series order, anthology contents, and "contains" relations.
- Goodreads: no new developer keys since 8 Dec 2020; existing keys deactivated for inactivity; claim that its terms ban scraping and storing data comes from secondary sources. [S] https://www.goodreads.com/topic/show/21788520-api-deprecation . Only supported route: import the user's own CSV export. StoryGraph no API: [U] (not checked).
- LibraryThing: thingISBN (related ISBNs for one work) still reported available, free for non-commercial use, asked to stay under ~1,000 requests/day and download the whole feed instead; covers service needs a free member key. [S] https://www.librarything.it/developer/documentation/thingapis . Exact terms page blocked.

### Audiobooks
- Audnexus: community aggregator whose code is GPL-3.0; no data licence or terms found; it relays Audible data, so Audible's own conditions (personal, non-commercial licence) apply and Audible publishes no public catalogue API. [S] https://github.com/laxamentumtech/audnexus . Not usable by default.
- LibriVox API: /api/feed/audiobooks, /audiotracks, /authors; xml/json; limit max 500 per call; fields include title, description, language, copyright_year, num_sections, totaltime, authors, urls, with extended=1 genres/sections/translators and coverart=1 covers; no rate limit now but 429 may come; no licence stated (LibriVox recordings are public domain). [V] https://librivox.org/api/info
- Open Library has audio-capable editions via its IA links but I found no audio-specific field documentation. [U]

### Comics and manga
- Metron: 20 requests/minute burst, 5,000/day sustained per authenticated user, X-RateLimit-* headers, a 304 still counts; use modified_gt for incremental sync, store IDs, If-Modified-Since on detail pages; terms for commercial software are on a separate blog post I could not read. [V] https://metron-project.github.io/blog/api-best-practices . A search result said 30/min plus 10,000/day at an earlier date; use the newer page and the headers.
- AniList: free if app earns under $150/month, otherwise commercial licence; "Using the AniList API as a backup or data storage service is strictly prohibited"; "hoarding or mass collection" prohibited; apps using the name must say unofficial. [V] https://anilist.gitbook.io/anilist-apiv2-docs/docs/guide/terms-of-use . Rate: 90 req/min, 1-minute timeout; guide also carries a notice of temporary 30/min. [S] https://docs.anilist.co/guide/rate-limiting
- MyAnimeList API agreement: no server-side storage of users' personal content; client-side storage only as long as needed; delete changed/removed content within 24 hours of request; no scraping; commercial apps (paid, subscription, ad-supported) need written authorisation; Client ID cannot be shared (so per-user or per-install client id registered by each user); no numeric limit, do not overload. [V] https://myanimelist.net/static/apiagreement.html
- GCD: CC BY-SA 4.0 per site footer; per-page CSV/JSON export, full dump needs a GCD account (~45 GB loaded). [S] https://www.comics.org/ . Kitsu: not found. [U]

### Academic
- arXiv: no more than one request every 3 seconds, one connection, across all machines you control; descriptive metadata CC0 (e-prints separate). [S] https://info.arxiv.org/help/api/tou.html (the opened URL 404ed on the old path)
- NCBI E-utilities: 3 req/s without key, 10/s with key; one key per account; higher on request. [S] https://www.ncbi.nlm.nih.gov/books/NBK25497/ (bot wall)
- OpenAlex: no key 10 req/s; free key 30/s and 10x daily budget; paid 50/s; semantic search 1/s; 429 for daily budget, per-second or new-account pace; key required for cursor paging; page says only "data is free" (CC0 is [U], mentioned in task). A forum post says keys became required from 13 Feb. [V] https://help.openalex.org/how-to-use-the-api/rate-limits-and-authentication
- Semantic Scholar: unauthenticated pool shared by everyone (docs say 1,000 req/s shared, tutorial says 5,000 per 5 minutes); key gives 1 req/s; licence agreement binds your organisation, operative terms unread. [S] https://semanticscholar.org/product/api
- Unpaywall: email parameter required; ~100,000 calls/day; use the snapshot for more. [S] https://unpaywall.org/api/v2 . DOI content negotiation: not researched beyond round 1 Crossref. [U]

### Public-domain catalogues
- Project Gutenberg: site intended for humans; automated tools risk an IP block; bulk harvesting only via the robot harvest URL with wget and delays; new JSON OPDS2 feed in testing (contact needed), XML OPDS feeds expected to be sunset in 2027; weekly CSV catalogue. [S] https://www.gutenberg.org/policy/robot_access.html . Both Gutenberg pages returned 503 with Retry-After 3600; not retried.
- Standard Ebooks: OPDS at /feeds/opds (OPDS 2 with Accept: application/opds+json); only the New Releases feeds are open, other feeds need a Patrons Circle membership (email as user, blank password) or having produced an ebook; honeypot link bans IP for 24 hours; its own content CC0. [V] https://standardebooks.org/feeds
- Internet Archive metadata read: GET https://archive.org/metadata/{identifier}; partial paths; no rate limits or terms on that page. [V] https://archive.org/developers/md-read.html

### National libraries
- BnF: all descriptive metadata under Etalab Open Licence since 1 Jan 2014; keep ARK id and RDF dump date; Gallica images under different terms. [S] https://data.bnf.fr/licence
- NDL Japan: bibliographic data free incl. commercial, compatible with CC BY 4.0; SRU at ndlsearch.ndl.go.jp/api/sru, OpenSearch and OAI-PMH; API use may need an application; provider-specific terms. [S] https://ndlsearch.ndl.go.jp/en/help/api
- KB (Netherlands): open datasets mostly CC0 metadata; SRU at jsru.kb.nl/sru (2015 evidence). [S]
- Trove v3: API key required; 200 calls/min with key (10/min without); metadata licence limited to your own sites and apps, non-commercial version of terms, user-contributed content only CC BY-NC-SA. [S] https://trove.nla.gov.au/about/create-something/using-api/trove-api-terms-use . Users bring own key.
- British Library: 3 million BNB records released CC0 in 2010; current Z39.50 service is password-protected for non-commercial library use; IFLA says redistribution licensed. No SRU found. [S] https://www.bl.uk/more/collection-metadata-services/
- K10plus: SRU at sru.k10plus.de/gvk, core data open via APIs and dumps; CC0 unconfirmed; Z39.50 at z3950.k10plus.de reported for one database. [S]
- LAC: AMICUS replaced by Voilà and Aurora (2018); no current SRU or Z39.50 found; a 2026 collection data export tool exists. [S]
- Z39.50: still offered by BL (restricted), K10plus. Worth supporting only as a late optional module; SRU first. [U judgement]

### Subject vocabularies and codes
- BISAC: viewing free; a paid licence is needed to "download the entire list for use in business systems"; no redistribution without permission. 2025 edition. [V] https://www.bisg.org/complete-bisac-subject-headings-list . So do not bundle BISAC; store whatever codes arrive in imported metadata as opaque strings.
- Thema: page shows "All rights reserved" and links to terms I could not open; secondary sources say free to use, CC BY 4.0, v1.7 current, ~27 languages, Excel/HTML/XML/JSON code lists, SKOS, SQLite for members only. [V for page, S for the CC BY claim] https://www.editeur.org/151/Thema/
- FAST: ODC-By, downloadable MARC XML/RDF; credit line suggested; last download update 20 Mar 2024. [S] https://oclc.org/research/areas/data-science/fast/download.html
- ISO 639-3: tables may be incorporated into software with attribution, but the product must not offer a way to redistribute the code set; SIL is the only authorised distribution site; older terms stricter. [S] https://iso639-3.sil.org . Safer: use the 639-1/2 and BCP 47 registry (IANA, public) in the data model and store 639-3 only as a code string.
- LCSH: see id.loc.gov above. Wikidata genres: CC0 (round 1).

### ISBN
- ISBN range file (RangeMessage XML): only offered from the agency's own site; the agency does not authorise other sites or products to reproduce it; XML only; no stated schedule. [V] https://www.isbn-international.org/range_file_generation . Consequence: the app should fetch the file at the user's request from the agency (or use a library that does) rather than shipping a copy in the repo; hyphenation is optional and can be done by a fallback table until then. Legal reading is mine.
- Rules (check digits, 10-to-13 conversion): ISBN-13 check = (10 - (sum of digits at weights 1,3,1,3... over first 12 digits mod 10)) mod 10; ISBN-10 weights 10 down to 2 mod 11, check may be X; convert 10 to 13 by prefixing 978 and recomputing the check digit; 979 numbers have no ISBN-10 form. [U, from the standard, not read this session]
- Why one work has many ISBNs: each edition and each format (hardcover, paperback, ebook, audiobook) gets its own ISBN; reprints may not; ebooks from different retailers may carry different ISBNs or none. [S, secondary sources] https://www.ola.org/en/office-assembly/library-research/isbn-issn-faqs-request-forms/isbn-faqs
- Identifiers when no ISBN: ASIN (Kindle), OCLC number, LCCN, DOI, Open Library OLID, Google volume id, Goodreads id, ISSN for serials, UUID in the book file. [U]

## 2. Corrected source table

Default = shipped, no user key.

| Source | Class | Licence / terms | Limit | Best for | Attribution |
|---|---|---|---|---|---|
| Open Library (API, covers, dumps) | Default | IA, no new rights asserted; CC0 [U] | 1/s, 3/s with email User-Agent; covers 100 per IP/5 min except by cover id or OLID; no bulk via API | ISBN lookup, covers, works/editions, offline index | Courtesy link requested |
| Library of Congress (loc.gov, id.loc.gov) | Default | US government, public domain [S/U] | ~20/min (round 1) | US books, authorities, LCSH, relators | None required |
| DNB, BnF, NDL, KB, K10plus SRU | Default (each with its own rules) | CC0 / Etalab / CC BY-compatible; K10plus unconfirmed | polite, one at a time | National bibliographies | BnF: keep ARK id and dump date; NDL: credit provider |
| Wikidata | Default | CC0 | polite | IDs, series, genre cross-links | None |
| Crossref | Default | open metadata | polite pool | DOIs, academic | None |
| arXiv | Default | metadata CC0 | 1 request per 3 s, 1 connection | preprints | None |
| OpenAlex | Default with free key later | free data | 10/s no key | academic | none stated |
| PubMed E-utilities | Default | NCBI policy | 3/s, 10/s with key | biomedical | none stated |
| Unpaywall | Default | read terms [U] | ~100,000/day, email required | open access links | - |
| LibriVox API | Default | no terms stated | be considerate, max 500 per call | public-domain audiobooks | - |
| Standard Ebooks | Default for the open New Releases feed; full OPDS needs user's membership | CC0 content | no stated limit; honeypot ban | polished PD editions | None |
| Project Gutenberg | Default only through its harvest route; dump once | PD works | wget harvest with delays | PD catalogue | per its trademark rules [U] |
| Internet Archive metadata | Default | per-item rights | not stated | scans, audio | - |
| BookBrainz | Optional | CC0 core, CC BY-SA rest | not found | open entity IDs | BY-SA for non-core |
| ISFDB (dump) | Default as user-requested download | CC BY 4.0 | n/a | SF series, anthologies | credit ISFDB |
| GCD (dump) | Optional, needs account | CC BY-SA 4.0 | n/a | US comics | credit + share-alike |
| FAST | Default as download | ODC-By | n/a | subjects | credit line |
| Google Books | User's own key | no shared key in open source; no permanent copies; cache per header | quota per project [U ~1,000/day] | covers, gap filling, snippets | follow API docs; no endorsement claim |
| Hardcover | User's own token | terms not read | ~60/min, daily caps [S] | moods, genres, ratings, series, editions | not found |
| ISBNdb | User's paid key | unread | 1 to 5 req/s, 5,000 to 50,000/day | ISBN coverage | not found |
| Comic Vine | User's own key | non-commercial | 200 per resource per hour (round 1) | comics | per terms |
| Metron | User's own account | commercial post unread | 20/min, 5,000/day | comics, ComicInfo-style data | not found |
| AniList | User's own use or default under $150/month | no storage/hoarding | 90/min (30/min notice) | manga | say "unofficial" if name used |
| MyAnimeList | User's own client id | non-commercial free; no server-side storage | not numeric | manga | credit as source |
| Trove | User's own key | non-commercial, own apps | 200/min with key | Australian books | per terms |
| ORCID public | User's own registration | ToS | not stated | author IDs | - |
| Semantic Scholar | Optional key | agreement | key 1 req/s | academic | per agreement |
| LibraryThing thingISBN | Optional | non-commercial | ~1,000/day, 1 per s | related ISBNs | - |
| VIAF | Not for default | ODC-By data, API unstable since 2025 | unknown | author clustering | canonical URI |
| Goodreads | Not usable (CSV import only) | API closed | - | - | - |
| StoryGraph | Not usable [U] | no API | - | - | - |
| Audible / Audnexus | Not usable by default | unknown, Audible personal licence | - | audiobooks | - |
| WorldCat, Amazon PA-API, BISAC list, ISBN range file copy | Not usable (bundle) | see above | - | - | - |
| British Library Z39.50 | Not usable by default | registered non-commercial | - | UK | - |

## 3. Rate-limit rules the app must enforce and remember

Per host token bucket stored in the database with the last 429 time, Retry-After value and daily counter, surviving restarts:
- Open Library 1/s (3/s only with contact-email User-Agent); cover fetches by ISBN count against 100 per IP per 5 minutes (use cover id or OLID after the first lookup; remember the cover id).
- Library of Congress 20/min; honour 429 backoff and no deep paging past 100,000.
- arXiv one request per 3 s and a single connection across the whole app.
- Metron 20/min and 5,000/day; read X-RateLimit-* headers; 304 counts; 2 to 3 workers at most.
- AniList 90/min (drop to 30 if the docs notice is live); 1-minute timeout when exceeded; do not store.
- NCBI 3/s, 10/s with key. OpenAlex 10/s keyless. Unpaywall 100,000/day with email. Trove 200/min with key.
- Hardcover about 60/min plus a daily cap; reset at midnight UTC; read X-RateLimit-Daily-Reset.
- ISBNdb per plan (1/3/5 per s, daily cap). Google Books per user's project quota; honour cache header.
- Comic Vine 200 per resource per hour (round 1). LibraryThing 1/s and under 1,000/day.
- Generic: identify the app by User-Agent with contact address; cache every response; never run in parallel to one host; never crawl covers; stop on 403 for 5 minutes and on 503 Retry-After.

## 4. Fields only one source supplies (as read)
- Hardcover: moods, community genres, per-user reading data, book_series with position.
- ISFDB: anthology/collection contents, SF series order, title/publication split.
- LibriVox: narrators/sections, total running time, audio file URLs.
- Metron/GCD: issue-level credits, story arcs, publication-month data for comics.
- AniList/MAL: manga volume/chapter counts and external ranks.
- LibraryThing thingISBN: related ISBNs of one work.
- LC/BnF/DNB: authority ids and subject headings in national schemes.
- Unpaywall: OA location of a DOI. arXiv: arXiv categories.
- Open Library dumps: lists and reading-log aggregates, ratings dump.
(Each only as stated on the pages above; coverage studies not found.)

## 5. Data the app must store
- Per-source credentials table (user key, token expiry, registered client id); per-source rate state; ETag/Last-Modified/modified_gt cursor per source.
- Per-field provenance: source, source id, fetch time, licence class (CC0, CC BY, CC BY-SA, ODC-By, no-store, transient), and whether permanent storage is allowed (Google, AniList, MAL: no).
- Identifier set per edition with scheme names: isbn10, isbn13, asin, oclc, lccn, olid, ol cover id, google volume id, hardcover id, viaf, isni, orcid, wikidata Q, doi, arxiv id, metron id, comicvine id, gcd id, isfdb id, bnf ark, dnb id.
- Work vs edition vs format split; many ISBNs per edition group; contributors with role codes (MARC relator three-letter code as string).
- Language as BCP 47 tag; subject as (scheme, code, label), schemes include BISAC, Thema, FAST, LCSH stored as opaque.
- Cover record: source, size variant, original url, fetched time, cover id for source lookup.
- Series: name, position (fractional allowed), source.

## 6. Requirements existing products get wrong
- Shipping a shared API key in an open-source build breaks Google's terms; prompt each user for their own and explain why.
- Hold only what the licence allows; mark records transient for no-store sources and drop them on request (MAL 24 hours).
- Throttle by rule, not by guess; persist limits so a restart does not trigger bans.
- Offer a monthly Open Library dump import for offline lookup instead of hammering the API.
- Do not bundle BISAC or the ISBN range file; fetch from the owner.

## 7. Open questions
- Hardcover third-party terms and real limits; ISBNdb storage clauses; Google daily default; Thema's own licence text; VIAF 2025 terms; Open Library licence statement for dumps; id.loc.gov licence; LAC and British Library SRU; Kitsu; DOI content-negotiation limits; StoryGraph; Audnexus terms; ISBN agency pages on conversion and many-ISBNs.

## 8. URLs opened (fetched)
developers.google.com/books/terms; developers.google.com/terms; developers.google.com/books/docs/v1/using; bookbrainz.org/about; bookbrainz.org/licensing; openlibrary.org/developers/dumps, /developers/api, /developers/licensing, /dev/docs/api/covers; info.orcid.org public-api; loc.gov/apis/json-and-yaml; loc.gov/marc/relators; librivox.org/api/info; metron-project.github.io/blog/api-best-practices; anilist.gitbook.io terms-of-use; myanimelist.net/static/apiagreement.html; standardebooks.org/feeds; archive.org/developers/md-read.html; help.openalex.org rate limits; isbn-international.org/range_file_generation; bisg.org complete BISAC list; editeur.org Thema page. Blocked and not retried: docs.hardcover.app, isbndb.com, id.loc.gov/techcenter, isfdb.org, librarything.com, metron.cloud/docs, docs.anilist.co, gutenberg.org (503), ncbi.nlm.nih.gov, unpaywall.org, isbn-international users manual (404), github openlibrary wiki (no content).
