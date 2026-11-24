# Limits from outside

Law, licences, store rules and safety. These remove some features, reshape others,
and add records to the data model. This is a summary of what public documents say,
not legal advice.

## Protected books

- `LAW-1` (core) The app does not ship, link to, host or describe a way to remove
  protection from a book. Its documentation, issue tracker and any list of
  extensions refuse them too. United States law exempts certain acts of
  circumvention for certain people, such as readers with print disabilities, and
  does not exempt distributing tools. Done: the app has none, and the
  [contributing guide](../../CONTRIBUTING.md) and [README](../../README.md#project)
  state the refusal for the documentation and the issue tracker. The app has no
  extension list yet.
- `LAW-2` (core) When a user opens a file protected by a scheme the app does not
  support, the app names the scheme, says it cannot open it, keeps the book in the library, and offers no workaround.
  Done for EPUB on Linux: see [protected books](../design/protected-books.md).
- `LAW-3` (later) Readium LCP is the only scheme found with a published route for an
  independent reader to support it. Other schemes were not researched. It needs a signed agreement with EDRLab, a yearly
  fee, a yearly conformity check, and a confidential production module. The
  public app must build and run without the production module or certificate
  material. EDRLab's Thorium Reader establishes an open-source precedent.
  Eligibility and terms for Ebook Manager still need confirmation.
  [EDRLab provider process](https://www.edrlab.org/readium-lcp/how-to-lcp-reading-system-provider/)
  · [Thorium Reader](https://www.edrlab.org/software/thorium-reader/)

## Licences of parts

The project is Apache 2.0. Code under the GPL or AGPL cannot be built into it.
See the [platform design](../design/platforms.md#licence-checks-and-protected-books)
for proposed components and release audit requirements.

Each licence below was read on 19 November 2026 in the part's own licence file or
on its official site, linked in the row. Usable means the part can be linked or
bundled in this Apache 2.0 app under LIC-1, with the conditions given. Every
usable part also carries the notice duties of its licence: its copyright notice,
licence text and any credit it asks for go into the release manifest that LIC-2
requires. A row may repeat such a duty and always names one that goes beyond it. Listing a file in the
manifest does not by itself meet its licence: a part that requires source access,
a credit in documentation or a way to replace the library needs that as well.

| Need | Part | Licence, from its source | Use in the app |
|---|---|---|---|
| EPUB and reader toolkits | Readium Swift and Kotlin toolkits | BSD 3-Clause. [Swift](https://github.com/readium/swift-toolkit/blob/main/LICENSE) · [Kotlin](https://github.com/readium/kotlin-toolkit/blob/main/LICENSE) | Usable. Keep the notice. Their own dependencies need the release audit |
| PDF drawing | PDFium | BSD 3-Clause style, with the Apache 2.0 text in the same file. [Licence](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE) | Usable. Binary releases reproduce its notice and disclaimer. The `third_party` code built into the pinned revision is not checked yet |
| PDF drawing | pdf.js | Apache 2.0. [Licence](https://github.com/mozilla/pdf.js/blob/master/LICENSE) | Usable |
| PDF drawing | MuPDF | AGPL 3.0 or later, or a paid licence from Artifex. [README](https://github.com/ArtifexSoftware/mupdf/blob/master/README) | Not usable under the AGPL |
| PDF drawing | Poppler | GPL. [Licence](https://gitlab.freedesktop.org/poppler/poppler/-/blob/master/COPYING) | Not usable |
| Conversion | kepubify | MIT. [Licence](https://github.com/pgaskin/kepubify/blob/master/LICENSE) | Usable |
| Conversion | Pandoc | GPL 2.0 or later, with some modules under other GPL-compatible licences. [Copyright file](https://github.com/jgm/pandoc/blob/main/COPYRIGHT) | Only as a separate program the user installs |
| OCR | Tesseract | Apache 2.0. [Licence](https://github.com/tesseract-ocr/tesseract/blob/main/LICENSE) | Usable |
| OCR | Tesseract language data: `tessdata`, `tessdata_fast`, `tessdata_best` | Apache 2.0, stated for all data in each repository with no exception for any language. [tessdata](https://github.com/tesseract-ocr/tessdata/blob/main/LICENSE) · [README](https://github.com/tesseract-ocr/tessdata/blob/main/README.md) · [fast](https://github.com/tesseract-ocr/tessdata_fast/blob/main/LICENSE) · [best](https://github.com/tesseract-ocr/tessdata_best/blob/main/LICENSE) | Usable. List each bundled file under LIC-2 |
| Archives | libarchive | BSD 2-Clause for most files; a few files are under the 3-clause UC Regents licence, public domain, or a choice of CC0, OpenSSL or Apache 2.0. Its RAR reader is its own code. Build scripts vary file by file. [Copying file](https://github.com/libarchive/libarchive/blob/master/COPYING) | Usable. Keep the notices |
| Archives | UnRAR source | Its own freeware licence: free to use for reading RAR, but not to build a RAR-compatible archiver or recreate RAR compression. Modified copies must carry its paragraph 2. Read in `license.txt` inside the source archive from [RARLAB](https://www.rarlab.com/rar_add.htm) | Avoid. Read RAR through libarchive. RAR cannot be created |
| MTP | No part named yet | | Unconfirmed. Choose a permissively licensed implementation, audit its native dependencies, and test transport on each target. Omit cable transfer on iOS |
| Audio | FFmpeg | LGPL 2.1 or later; optional parts are GPL 2.0, and using them puts all of FFmpeg under the GPL. [Legal page](https://ffmpeg.org/legal.html). Three files taken from libjpeg require a credit to the Independent JPEG Group in the documentation of a binary-only release. [Licence file](https://github.com/FFmpeg/FFmpeg/blob/master/LICENSE.md) | The platform's own decoders on mobile. FFmpeg only built without `--enable-gpl` and `--enable-nonfree` and linked dynamically. A build that ships it also carries the LGPL notice and licence text, offers the corresponding FFmpeg source, lets the user replace the FFmpeg libraries with a modified build, and credits the IJG. Under LGPL 2.1 section 6 the build also gives prominent notice that FFmpeg is used and under which licence, and its own terms permit modifying the app for the user's own use and reverse engineering it to debug such changes. If any of the three IJG-derived files is changed, the documentation says so |
| Speech | Piper | The old repository is MIT and archived. Development moved to `piper1-gpl`, which is GPL 3.0. [Old licence](https://github.com/rhasspy/piper/blob/master/LICENSE.md) · [New licence](https://github.com/OHF-Voice/piper1-gpl/blob/main/COPYING) | Only as something the user installs. Each voice model has its own licence (DERIV-11) |
| Hyphenation | `hyphenation` Rust crate 0.8.4 and the `hyph-utf8` patterns it carries | The crate's code is MIT or Apache 2.0. Each pattern file has its own licence, stated in the header of its source `.tex` file. The crate takes its patterns from tex-hyphen revision [`49706f9`](https://github.com/hyphenation/tex-hyphen/tree/49706f9cfa97f6ead26b473ec10d23d5a651318a/hyph-utf8/tex/generic/hyph-utf8/patterns/tex). Of its 80 languages, 48 offer MIT, BSD or a short notice-keeping permission, among them American and British English, German, French, Spanish, Italian and Portuguese. 21 are under the LPPL alone, Dutch among them ([`hyph-nl.tex`](https://github.com/hyphenation/tex-hyphen/blob/49706f9cfa97f6ead26b473ec10d23d5a651318a/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-nl.tex): LPPL 1 or later). Eight are GPL, LGPL or MPL with no permissive choice: Czech, Slovak, Indonesian (GPL 2), Macedonian and Serbian Cyrillic (GPL), Hungarian, Latvian and Armenian. Romanian states no licence, and the crate's Albanian and school Finnish files have no source file at that revision. The crate also ships extended Hungarian (MPL 1.1) and Catalan (LGPL 3.0) files. [Crate licence](https://github.com/tapeinosyne/hyphenation#license) | Prefer the web view's own hyphenation. Never enable the crate's `embed_all` feature. Bundle only a language whose source file, at the revision the shipped patterns were built from, offers MIT, BSD or a notice-keeping permission, and ship that file's notice. [Current upstream `hyph-nl.tex`](https://github.com/hyphenation/tex-hyphen/blob/master/hyph-utf8/tex/generic/hyph-utf8/patterns/tex/hyph-nl.tex) offers MIT, so Dutch may ship only when built from the current upstream file under that MIT grant, not from the crate's copy. LPPL-only files need their own review first. Leave out the GPL, LGPL, MPL and unlicensed files |
| Fonts | Literata | SIL Open Font License 1.1, with no reserved font name. [Google Fonts copy](https://github.com/google/fonts/blob/main/ofl/literata/OFL.txt) · [Project](https://github.com/googlefonts/literata/blob/main/OFL.txt) | Usable. Bundle its `OFL.txt` with the font files |
| Fonts | Atkinson Hyperlegible | SIL Open Font License 1.1, with no reserved font name. [Licence](https://github.com/google/fonts/blob/main/ofl/atkinsonhyperlegible/OFL.txt) | Usable. Bundle its `OFL.txt` with the font files |
| Dictionaries | Wiktionary data | CC BY-SA 4.0, also GFDL 1.1 or later, for the original text of entries. Reusers must attribute and share alike. Entries can also contain text, images or sounds from outside sources, used with permission or as fair use, which keep their own terms. [Wiktionary copyrights](https://en.wiktionary.org/wiki/Wiktionary:Copyrights) | Dictionaries the user installs. If shipped, use only the original entry text, attribute it, keep CC BY-SA on it and on anything derived from it, and keep it apart from the code |
| Storage | SQLite, bundled by `rusqlite` | SQLite is public domain; the bundled 3.50.2 source carries the authors' disclaimer. `rusqlite` and `libsqlite3-sys` are MIT. [SQLite copyright](https://sqlite.org/copyright.html) · [rusqlite licence](https://github.com/rusqlite/rusqlite/blob/master/LICENSE) | Usable |
| Images | `image` crate with `zune-jpeg`, `png`, `gif` and `image-webp` | `image`, `png`, `gif` and `image-webp` are MIT or Apache 2.0; `zune-jpeg` adds Zlib as a third choice; `moxcms`, pulled in for colour profiles, is BSD 3-Clause or Apache 2.0. [image](https://github.com/image-rs/image/blob/main/LICENSE-MIT) · [png](https://github.com/image-rs/image-png/blob/master/LICENSE-MIT) · [gif](https://github.com/image-rs/image-gif/blob/master/LICENSE-MIT) · [image-webp](https://github.com/image-rs/image-webp/blob/main/LICENSE-MIT) · [zune-jpeg](https://github.com/etemesi254/zune-image/blob/dev/LICENSE-MIT) ([its manifest](https://github.com/etemesi254/zune-image/blob/dev/crates/zune-jpeg/Cargo.toml) names the three choices) · [moxcms](https://github.com/awxkee/moxcms/blob/master/LICENSE.md) | Usable |
| Sorting | `icu_collator` with its bundled Unicode data | Unicode License v3 for the crate and its compiled data crates. [ICU4X licence](https://github.com/unicode-org/icu4x/blob/main/LICENSE) | Usable. Keep the Unicode notice |
| Linux interface | `gtk4` and `libadwaita` Rust bindings | MIT. [gtk4-rs licence](https://github.com/gtk-rs/gtk4-rs/blob/main/LICENSE); `libadwaita` from the `LICENCE` file in its published crate | Usable |
| Linux interface | GTK 4, libadwaita, GLib, Pango, GDK-Pixbuf and FriBidi | LGPL. [GTK](https://gitlab.gnome.org/GNOME/gtk/-/blob/main/COPYING) ([GitHub mirror](https://github.com/GNOME/gtk/blob/main/COPYING)) · [libadwaita](https://gitlab.gnome.org/GNOME/libadwaita/-/blob/main/COPYING) ([GitHub mirror](https://github.com/GNOME/libadwaita/blob/main/COPYING)). The others as declared by the Arch Linux packages | Usable only as shared libraries from the distribution or Flatpak runtime. Don't link them statically. A build that ships them itself takes on the same LGPL section 6 duties as the FFmpeg row gives |
| Linux interface | Cairo and FreeType, linked by GTK | Cairo: each library source file under LGPL 2.1 or MPL 1.1, by choice. [Cairo copying file](https://gitlab.freedesktop.org/cairo/cairo/-/blob/master/COPYING) ([Cairo's site](https://cairographics.org/) states the same choice). FreeType: the FreeType licence or GPL 2.0, by choice. The FreeType licence requires a credit in the product's documentation that FreeType code is used, and disclaims all warranty. [FreeType licences](https://github.com/freetype/freetype/blob/master/LICENSE.TXT) · [FreeType licence text](https://github.com/freetype/freetype/blob/master/docs/FTL.TXT) | Usable as shared system libraries. Choose FreeType's own licence. Any build that ships FreeType, such as a bundled Windows or macOS copy, credits it in its documentation, for example "Portions of this software are copyright © <year> The FreeType Project (https://freetype.org). All rights reserved." |
| Drawing a chapter on Linux | WebKitGTK 6.0 | LGPL version 2 or later for most of it, and BSD for the rest. [Licence page](https://webkit.org/licensing-webkit/) | Usable only as a shared library from the distribution or Flatpak runtime, as for GTK. Don't link it statically or bundle it |

The Rust workspace's dependency tree on 19 November 2026
(`cargo tree --workspace --edges normal --format '{p} {l}'`) declares only MIT,
Apache 2.0, Zlib, Unlicense, 0BSD, Unicode 3.0 and Apache 2.0 with the LLVM
exception, mostly as choices. None is GPL, AGPL, LGPL, MPL or missing. The LGPL
appears only in the native GNOME libraries above, which the bindings load from the
system.

- `LIC-1` (core) The app neither links nor bundles GPL or AGPL code. A tool under
  such a licence may be run as a separate program that the user installed, in direct
  desktop builds only.
- `LIC-2` (core) Every bundled part, font, model and data file is listed with its
  licence, and the list ships with the app.
- `LIC-3` (core) Vendor and format names (Kindle, Kobo, EPUB) are used only to say
  what the app works with, with a trademark notice, and never with their logos.

## Platform rules

See the [distribution design](../design/platforms.md#distribution-and-feature-limits)
for the planned packages and their capability checks.

| Feature | iOS | Android | macOS | Linux | Windows |
|---|---|---|---|---|---|
| Watched folders | Only while the app is open, in a folder the user picked | A folder the user picked. Access to all files needs Google's approval, which is uncertain | A folder the user picked, remembered by a bookmark that can go stale | Free outside Flatpak. Inside it, a portal grant | Free for a classic desktop app |
| Local server | Needs the local network prompt. Cannot run in the background | Runs as a visible foreground task | Allowed | Allowed. Flatpak needs the network permission | Allowed |
| Long jobs (import, conversion) | Foreground by default. Background work depends on OS scheduling | Foreground task with a notification, time limited for some kinds | Free | Free | Free |
| USB device transfer | Omitted | Requires host support and permission per device | MTP needs testing in both direct and sandboxed builds | Flatpak requires supported portal access | Test with supported devices |
| Code extensions | Omitted | Omitted | Direct build only | Direct build only | Direct build only |
| Checkout links in OPDS | Omitted | | | | |

- `PLAT-1` (core) The desktop app runs on macOS and Linux, and Windows is expected.
  The mobile apps are native on iOS and Android.
- `PLAT-2` (core) Every folder the app may read is recorded as a grant from the
  user, with the platform's token for it, and the app copes when a grant goes stale.
- `PLAT-3` (core) Features that a platform forbids are absent there, not broken.
  The app's own sync covers the gap on iOS, where there is no cable transfer and no
  background service.
- `PLAT-4` (expected) The Android app works with folder grants alone. A build that
  asks for access to all files is separate and is not the store build.
- `PLAT-5` (expected) On iOS the app is more than a catalogue browser, and an
  in-app OPDS browser omits external checkout steering. Apple's minimum
  functionality and commerce rules apply at submission.
  [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)

## Access for disabled readers

- `A11Y-1` (core) The reader meets the requirements in [reading](04-reading.md),
  `READ-70` to `READ-76`. European law has applied to e-book reading software since
  28 June 2025. Whether a free project falls under it is unsettled, so the app aims
  to conform anyway.
- `A11Y-2` (expected) The project publishes an accessibility statement and a way to
  report barriers.

## Privacy

- `PRIV-1` (core) The app works with no account and no network. Nothing leaves the
  machine unless the user turned that feature on.
- `PRIV-2` (core) No telemetry by default. Crash or usage reports are opt-in and
  show what would be sent.
- `PRIV-3` (core) A reading history can reveal religion, politics, health and
  sexuality, which European law treats as special categories. The app never sends
  the library list, reading history or annotations to any service as a side effect.
- `PRIV-4` (core) A log shows every time data left the machine: when, to which
  service, and what kind of data.
- `PRIV-5` (core) Secrets live in the operating system's keychain, never in the
  library or in an export.
- `PRIV-6` (expected) A book can be marked private: left out of sync, sharing,
  export and device rules.
- `PRIV-7` (expected) Deleting a book removes its derived data too: thumbnails,
  index entries, extracted text.
- `PRIV-8` (later) If the project ever hosts a sync service, it takes on data
  protection duties that a local app avoids. That is a separate decision.

## Opening untrusted files

- `SEC-1` (core) Scripts in a book are off by default. When allowed, they run in
  their own origin with no access to local files, the network or the app.
- `SEC-2` (core) Remote resources in a book are blocked by default, and loaded only
  over HTTPS when allowed. A book that asks for one is flagged, because a remote
  image can report that the book was opened.
- `SEC-3` (core) Archives are unpacked with limits on entry count, total size and
  compression ratio. Entries that point outside the archive, absolute paths and
  symbolic links are refused.
- `SEC-4` (core) XML is parsed without external entities or outside definitions.
- `SEC-5` (core) Parsing, drawing and conversion of files run in a separate process
  with no network and with limits on time and memory.
- `SEC-6` (core) PDF scripts, launch actions and attachments never run.
- `SEC-7` (core) Links out of a book open in the system browser after the user
  confirms.
- `SEC-8` (core) Anything the app serves is bound to the local machine by default,
  needs a key on the network, and serves book content from a different origin than
  the app's own pages.
- `SEC-9` (expected) An import records what a file tried to do: scripts found,
  remote resources referenced, limits hit.
- `SEC-10` (core) No file, however broken or hostile, crashes the app or one of
  its processes. A parser or converter that fails or hits a limit stops with an
  error the app can show, and the next file works. Done for EPUB reading: see
  [the reader spec](../../specs/epub-reader-mvp.md) F7.

## Fetched data

- `DATA-1` (core) Covers, descriptions and reviews fetched from sources belong to
  their publishers and writers. The app keeps them for the user's own library and
  household, and does not republish them.
- `DATA-2` (core) Each fetched field keeps its source and that source's terms. See
  [metadata](03-metadata.md).

## What the app must store

- Per file: protection scheme and whether this build can open it.
- Folder grants with the platform token, a stale flag and when each was last
  checked.
- Per book: permission for scripts and for network, and the log from `SEC-9`.
- The log of data that left the machine: when, which book, which service, what
  kind and range of data, and why. The user's consent per service and per book,
  with its scope, when it was given and when it lapses.
- Private flag per book.
- Per profile: whether it is a child's.
- A manifest of bundled parts and their licences.
- Per extension: kind, origin, signature, licence, permissions.

## Not verified

The text of the European directive and its exemption for very small businesses
and the EU's law on circumvention need review against the primary text.
The licence table leaves open the PDFium `third_party` code, an MTP part, and the
exact files of any bundled hyphenation patterns. The GitLab links for Poppler,
GTK, libadwaita and Cairo can show a browser check to automated readers; GTK and
libadwaita have identical copies on GNOME's GitHub mirror, and no official mirror
was found for Poppler or Cairo. Platform viability and the
package-level licence audit remain open as described
in the [platform design](../design/platforms.md).

## Sources

Apple's App Review Guidelines, Google Play's policy on access to all files, Android's
storage and foreground service documentation, the Flatpak sandbox documentation, the
United States rule at 37 CFR 201.40 and the Copyright Office's pages on the current
rulemaking, EDRLab's page for reading system providers, the Apache and GNU licence
compatibility pages, the licences of each part named, the W3C note mapping the
European Accessibility Act to EPUB, EPUB Reading Systems 3.3, and GDPR Article 9.
