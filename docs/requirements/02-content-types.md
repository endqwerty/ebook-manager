# Formats and kinds of content

What the app has to open, catalogue and convert, and where "one book is one file with
one author" stops being true.

## Formats

"Read" means open in the built-in reader. "Catalogue" means import, show metadata and
a cover, and send to a device.

| Format | Layout | Metadata and cover inside the file | Read | Convert from | Convert to |
|---|---|---|---|---|---|
| EPUB 2 and 3 | Reflowable or fixed | OPF metadata, declared cover | core | core | core |
| PDF | Fixed pages | Info dictionary and XMP, which can disagree. No standard cover, so use page 1 | core | expected, lossy | expected |
| Kobo kepub | Reflowable | EPUB plus Kobo markup | as EPUB | core | core, for Kobo devices |
| MOBI and AZW3 (KF8) | Reflowable | EXTH records, cover record | expected, when not protected | expected | expected, AZW3 for older Kindles |
| KFX | Reflowable or fixed | Amazon container with no public specification | catalogue only | no | no |
| CBZ, CBR, CB7, CBT | Page images | Optional `ComicInfo.xml`; first image is the cover | expected | expected | CBZ, PDF |
| FB2 | Reflowable | XML description block with base64 cover | expected | expected | later |
| TXT, Markdown, HTML | Reflowable | Little or none | expected | expected | expected |
| DOCX, ODT, RTF | Word processor | Document properties | no | expected | later |
| DjVu | Scanned pages | Sparse | later | later | no |
| LIT, PDB, LRF and other legacy formats | Reflowable | Varies | no | later | no |
| M4B, MP3, Opus, FLAC audiobooks | Audio | MP4 atoms, ID3 with chapter frames, Vorbis comments, cover art | expected | no | no |
| Saved web article | Reflowable | HTML metadata, source URL | later | later | EPUB |

- `FMT-1` (core) Import, catalogue and read EPUB and PDF.
- `FMT-2` (core) The app recognises a format by its content, not its file extension,
  and records the format version. EPUB 2 and 3 share an extension and a media type.
- `FMT-3` (core) A file the app cannot read is still catalogued, with the reason
  shown: an unknown format, a damaged file, or protection the app does not support.
- `FMT-4` (expected) Comics, audiobooks, MOBI, AZW3, FB2 and plain text open in the
  app.
- `FMT-5` (expected) The app keeps a snapshot of the metadata each file carried at
  import. It can then show "what the file says" beside "what the library says" and
  restore the original.
- `FMT-6` (expected) Per file, the app records facts that change how it is handled:
  reflowable or fixed layout, protected, scripted, uses remote resources, has
  obfuscated fonts, has a text layer, has a page list.

## Protected files

- `FMT-10` (core) The app detects protection, names the scheme, keeps the book in
  the library marked as locked, and reads whatever metadata and cover are not
  encrypted. It explains that conversion, editing and reading may not work.
- `FMT-11` (core) The app contains no code, plugin, link or instruction for
  removing protection. See [constraints](10-constraints.md).
- `FMT-12` (later) Reading Readium LCP files with the user's licence and
  passphrase. Library loans and the Internet Archive deliver LCP. This needs a
  closed module and a contract with EDRLab, so the app must build and run without
  it.

## Where one "book" record is the wrong unit

Each case below broke at least one product surveyed. The data model must hold all of
them, even if the first release shows only some.

- One audiobook in many files. One audio file holding a whole box set.
- An ebook and an audiobook of the same work, read alternately.
- The same work in several narrations, translations or abridgements. Chapters and
  positions belong to one edition's timeline and cannot be shared.
- An omnibus, anthology or box set that contains other works.
- A comic archive that is a chapter, a volume, an issue or a whole run.
- A magazine issue with its own volume, number and date, holding articles.
- A web serial or fan work that updates: the file is replaced, and the reader's
  place and notes must survive.
- A dictionary, which is a lookup source for the reader and not something to read.
- A fixed-layout picture book where text, audio and layout form one unit.
- Sheet music with a score and parts in one file.
- A folder of thousands of small files, such as a web archive or loose page images.

## Comics and manga

- `FMT-20` (expected) The hierarchy is publisher, imprint, series, volume, issue or
  chapter. A volume is known by a number or by a start year. One-shot, annual,
  special, trade paperback and omnibus are explicit kinds, not guesses from counts.
- `FMT-21` (expected) Issue numbers are text with a sort key. Real cases: decimals,
  letters, ranges, a restart at 1 under the same series name, a second "legacy"
  number, and volumes named by year.
- `FMT-22` (expected) Read and write `ComicInfo.xml` and keep fields the app does
  not understand. Credits are stored as structured people with roles, never as a
  comma-separated string, which breaks on a name that contains a comma.
- `FMT-23` (expected) Per page: type (story, cover, advertisement, hidden), double
  page flag, width and height. A page can be hidden without rewriting the archive.
- `FMT-24` (expected) Cover date and store date are separate. Age rating is a
  listed value with a mapping to other scales.
- `FMT-25` (later) Story arcs and cross-series reading lists, imported from `.cbl`
  files.

## Audiobooks

- `FMT-30` (expected) An audiobook is an ordered list of audio files with a chapter
  list. Each file has a duration and a start offset in the whole book. Chapters come
  from embedded marks, one file per chapter, a cue sheet, a lookup or the user, and
  the source of each is recorded.
- `FMT-31` (expected) Narrators are contributors with a role. Abridged or unabridged
  is a field of the edition.
- `FMT-32` (expected) Sidecar files beside audiobooks are read: OPF, NFO,
  `desc.txt`, `reader.txt`, `metadata.json`, `cover.jpg`. Track order comes from the
  tags or the file names, whichever is consistent, and the user can correct it.
- `FMT-33` (later) A text edition and an audio edition of one work are linked by an
  alignment map from text position to audio time, coarse (by chapter) or fine. The
  reader can then switch between reading and listening at the same place.
  Audiobookshelf keeps the two positions apart with no way to cross over, and
  switching is the most repeated wish in user discussions.

## Other content

- `FMT-40` (later) Periodicals: a title with issues, each with a date, volume,
  number, cover and articles. Reading state is per issue.
- `FMT-41` (later) Academic papers: DOI, arXiv identifier, journal, volume, pages
  and abstract, with export to BibTeX, RIS and CSL JSON. A reference manager does
  this better, so the first aim is to store the identifiers and export cleanly.
- `FMT-42` (later) Saved web articles and serials: source URL, date saved, a stored
  copy of the text, and for serials the dates last checked and last updated as two
  fields. Pocket and Omnivore both closed, so the stored copy matters.
- `FMT-43` (later) EPUB dictionaries and StarDict files are installed as lookup
  sources per language.
- `FMT-44` (later) Personal documents such as scans and letters, private by default
  and kept out of sharing and device rules.

## What the app must store

- Per file: detected format and version, layout kind, protection scheme and whether
  this build can open it, the flags in `FMT-6`, validation results, and the metadata
  snapshot from import.
- Per multi-part item: an ordered list of parts, each with path, hash, size,
  duration, codec, track and disc numbers from the tag, from the file name and the
  one chosen, and its offset in the whole.
- Chapters or segments with start, end, title, level, source and the edition they
  belong to.
- Comic pages with type, double-page flag and dimensions.
- Series kind, volume label, start year, issue count and publication status.
- Links between editions: same text, translation, abridgement, aligned audio.
- Container links: this record contains those works.
- For serials: source URL, last checked, last updated, chapter count, and the chain
  of files that replaced each other.
- Alignment maps between a text file and an audio file, with their quality and how
  they were made.
- Installed dictionaries with source and target languages.

## Not verified

Current Kindle and Kobo format lists change with firmware and need a test on real
devices. Support for WebP, AVIF and JPEG XL images in comic archives, cue sheet
handling, and the tag-to-field mapping for audio files were not read from a primary
source.

## Sources

EPUB 3.3, EPUB Reading Systems 3.3, the W3C Audiobooks recommendation, the Readium
audiobook profile, Library of Congress format descriptions, the FictionBook 2.1
schema, the ComicInfo 2.1 schema, the Audiobookshelf, Kavita, Komga and Mihon
documentation, and Comic Vine's API documentation.
