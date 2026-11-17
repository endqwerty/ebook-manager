ISSUES

(Issues: EUR-Lex, eCFR/Federal Register, the Apple local-network page, Apple sandbox page and parts of the EPUB specs would not load or were truncated; those points are marked [unverified] or sourced from secondary pages. Not legal advice. No calibre code was seen on any page. No training-recall flag raised.)

## 1. Component licences (Apache-2.0 app, shipped to app stores)

"V" = read on a page I opened; "[unverified]" = from memory, check before relying.

| Component | Licence | Compatible with Apache-2.0 app? | Catch |
|---|---|---|---|
| Readium swift-toolkit, kotlin-toolkit | BSD-3-Clause (V: github.com/readium/swift-toolkit, /kotlin-toolkit) | Yes | LCP needs a private binary from EDRLab (R2LCPClient.framework / liblcp). Swift toolkit: Adobe ACS "not planned", CBR not planned, DAISY wanted. Swift develop branch needs iOS 15, Swift 6.2; Kotlin needs minSdk 24 and core-library desugaring. |
| Readium LCP client | Open C++ test module; production module is precompiled and confidential (V: edrlab.org/readium-lcp/how-to-lcp-reading-system-provider/) | Not shippable from an open repo | Production certificate and module need signed Terms of Use, an annual conformity fee (amount on EDRLab's pricing page, not read), "client company must be trusted", annual re-check. Cannot live in the public repo; the app must build and run without it. |
| PDFium | BSD-3/Apache-2.0 [unverified] | Yes | Binary builds must be maintained by us; no Rust-native option. |
| MuPDF | Dual AGPLv3 / commercial Artifex licence (V: artifex.com/licensing, page names Artifex products generally, not MuPDF specifically) | AGPL is one-way with Apache: an app linking it becomes AGPL as a whole | "If you can't meet the AGPLv3 ... a commercial licence is required." Page says nothing on app stores. Avoid unless project accepts AGPL. Apple's store terms conflict with GPL-family licences [unverified]. |
| Poppler | GPL-2/3 [unverified] | Avoid linking; separate process only | Same as MuPDF in effect. |
| pdf.js | Apache-2.0 (V: github.com/mozilla/pdf.js) | Yes | Needs a web view or JS runtime; fine on desktop, heavy on mobile. |
| pdf-rs, lopdf, other Rust PDF crates | MIT/Apache mostly [unverified] | Likely yes | Parse-only; rendering is the gap. |
| foliate-js, epub.js | MIT / BSD [unverified] | Yes | Web-view based. |
| KOReader engine (CREngine) | GPL-3 / LGPL parts [unverified] | No static reuse | Treat as a design reference only through its documentation. |
| EPUBCheck | BSD-3 [unverified] | Yes | Java; ship as optional validator, not required. |
| Ace by DAISY | MIT [unverified] | Yes | Node.js. |
| kepubify | MIT [unverified] | Yes | Go binary; Kobo-format conversion. |
| Pandoc | GPL (V: pandoc.org, version not stated) | Only as a separate executable the user runs | Cannot link; bundling a binary means distributing GPL source offer. Not usable on iOS (no child processes). |
| Tesseract | Apache-2.0 code; Leptonica BSD-2 (V: github.com/tesseract-ocr/tesseract) | Yes | Traineddata licence not stated on that page; check per language file. |
| UnRAR source | Own licence that forbids using it to recreate the RAR compressor [unverified; rarlab page I opened gave no licence text] | Not a standard OSI licence | Prefer libarchive (BSD) for RAR read; RAR5 coverage must be tested. Cannot create RAR. |
| 7-zip / LZMA SDK | LGPL / public domain for LZMA SDK [unverified] | LZMA SDK yes | Use SDK, not 7-zip UI code. |
| libmtp | LGPL (V: libmtp.sourceforge.net, version not given) | Dynamic link fine on Linux/desktop | Static link into an iOS app is a problem because the user cannot relink [unverified reasoning]; page lists POSIX only, no Windows/macOS/Android support claim. iOS has no USB MTP at all (see section 2). |
| FFmpeg | LGPL-2.1+ by default; any GPL option makes the whole build GPL (V: ffmpeg.org/legal.html) | Yes if built without --enable-gpl / --enable-nonfree and dynamically linked | Must ship matching source, configure line, about-box notice; patent exposure for codecs (page cites MPEG LA). Use platform codecs (AVFoundation, MediaCodec) on mobile instead. |
| SQLite | Public domain (V: sqlite.org/copyright.html) | Yes | Some jurisdictions do not recognise dedication; paid Warranty of Title exists. |
| ICU, HarfBuzz | ICU permissive; HarfBuzz MIT-style [unverified] | Yes | Large ICU data size on mobile; OS provides ICU on Android/iOS. |
| Fonts under SIL OFL | OFL 1.1 (V: openfontlicense.org/ofl-faq/) | Yes: bundling with software of any licence allowed | Cannot sell fonts alone; keep notice and licence text; modified fonts stay OFL; Reserved Font Names forbid reuse of the name on modified versions. |
| OpenDyslexic | OFL-style [unverified] | Yes | Same as above. |
| Atkinson Hyperlegible (and Next, Mono) | Custom Braille Institute EULA, "free for personal use and all commercial applications" (V: brailleinstitute.org/freefont/) | Must read the EULA before bundling; page did not say OFL | Treat redistribution inside our repo as unverified. Google Fonts hosts an OFL build [unverified]. |
| Hyphenation dictionaries | Vary: LGPL, MPL, GPL per language [unverified] | Check each | Ship only permissive ones in app; download others as optional data. |
| Wiktionary data / dumps | CC BY-SA 4.0 and GFDL; some text CC BY-SA only (V: dumps.wikimedia.org/legal.html) | Data, not code; share-alike applies to a derived dictionary | A generated dictionary file must carry attribution and stay CC BY-SA. Do not mix into Apache-licensed code. |
| StarDict dictionaries | Per-dictionary; many unlicensed or GPL [unverified] | Do not bundle | Let users import their own. |
| Piper TTS | Active repo is OHF-Voice/piper1-gpl, GPL-3.0; separate voice-licence page (V: github.com/OHF-Voice/piper1-gpl) | Not linkable; GPL | Voice models have per-voice licences; use platform TTS first (AVSpeechSynthesizer, Android TextToSpeech). Offer Piper only as user-installed. |

Apache 2.0 rule (V: apache.org/licenses/GPL-compatibility.html): Apache-2.0 code may go into GPLv3 projects, not the reverse; never compatible with GPLv2. FSF list (V: gnu.org/licenses/license-list.html) confirms the same and that LGPL/AGPL combine only as separate modules.

## 2. Store and platform rules

Apple App Review Guidelines (V: developer.apple.com/app-store/review/guidelines/, page truncated at 5.2.5):
- 2.5.2: apps "may not download, install, or execute code which introduces or changes features or functionality of the app". This bans downloadable executable plugins on iOS. Data-only plugins (metadata source definitions, CSS themes, dictionaries) are not code. Scripting plugins that run in a JS engine are a grey zone [unverified].
- 4.7 allows HTML5/JS mini apps and "plug-ins" with developer responsibility, an index of software, and age restrictions (4.7.1 to 4.7.5). It is written for games/mini-apps; fit for an ebook plugin store is unclear.
- 3.1.3(a) reader apps: may "access previously purchased content ... books"; may offer account creation for free tiers; External Link Account Entitlement for an informational link; not required on the US storefront. 3.1.1: outside entitlements and the US store, no "buttons, external links, or other calls to action" toward other purchasing. A free, open-source manager with no sales has no payments issue; an in-app catalogue browser that links to stores outside the US may violate 3.1.1 unless the catalogue is not Apple's definition of a store (unclear) .
- 4.2.2: apps "shouldn't primarily be ... content aggregators, or a collection of links" (catalogues excepted). An OPDS browser must not be the whole app.
- 5.2.2: displaying content from a third-party service requires permission under that service's terms; 5.2.3: no saving/converting media from third-party sources without authorisation.
- 2.5.15: apps that open files should include the Files app and iCloud documents. 5.1.1(iii): use the out-of-process picker.
- 5.1.1 and 5.1.2: privacy policy, consent for data collection, account deletion if accounts exist, no repurposing of data.
- Kids category 1.3 / 5.1.4: no third-party analytics or ads; links out and purchases behind a parental gate; privacy policy for any app that can collect minors' data.
- 2.5.4: background services only for intended purposes (audio playback for read-aloud, local notifications, task completion). No general-purpose background sync daemon or watched-folder service on iOS.
- Local network: no dedicated App Review rule found. Apple's Local Network privacy prompt and NSBonjourServices declaration apply from iOS 14; multicast needs an Apple-approved entitlement [unverified: Apple page returned no text]. Consequence: a household server on the LAN and Bonjour discovery need a usage string and declared service types; a denied prompt silently breaks discovery.
- Security-scoped bookmarks, user-selected-file entitlements, stale-bookmark refresh, start/stop pairing [unverified: page returned no text; widely documented]. Consequence: a watched folder on macOS or iOS exists only because the user picked it, and must be re-picked if the bookmark fails. File Provider extensions and iCloud containers [unverified]: cloud-evicted files must be downloaded before reading.
- macOS notarisation and Mac App Store sandbox (2.4.5(iii): no login-launch without consent) [partly V]. A direct-download build can be notarised but not sandboxed; a Mac App Store build is sandboxed and cannot read arbitrary folders or raw USB devices without user selection or entitlements.

Google Play and Android:
- All-files access (V: support.google.com/googleplay/android-developer/answer/10467955): permitted for file managers, backup, anti-virus, "document management", on-device search; refused where the Storage Access Framework or MediaStore suffices; "User-driven file selection" is explicitly not a valid reason. Needs a Play Console declaration and described core functionality. A library manager may argue "document management" but risk of refusal is real; plan for SAF tree grants (ACTION_OPEN_DOCUMENT_TREE) and persisted URI permissions, and a MANAGE_EXTERNAL_STORAGE flavour for F-Droid or sideload only.
- Android docs (V: developer.android.com/training/data-storage/manage-all-files): with all-files access, still no write to /Android/data of other apps; access to USB OTG and SD roots comes with it.
- Foreground services (V: developer.android.com/develop/background-work/services/fgs/service-types): from Android 14 each needs a declared type and permission; Play Console declaration required. dataSync is for upload, download, import/export, local file processing, and cannot start from BOOT_COMPLETED on Android 15; mediaPlayback for read-aloud; connectedDevice for USB/Bluetooth/network device interaction; specialUse needs a free-text justification reviewed by Play; mediaProcessing (conversions of media) has a 6-hours-in-24 limit; shortService about 3 minutes. Consequence: long conversions, imports and a local server cannot run silently in the background; they run as user-visible work with a notification, and a LAN server on Android is not a persistent daemon. (The 6-hour dataSync timeout appears in a linked page, not read.)
- USB: UsbManager.requestPermission is the connectedDevice prerequisite; MTP to an e-reader from Android needs host-mode USB code, not a filesystem path.

Linux (V: docs.flatpak.org/en/latest/sandbox-permissions.html): default no network, no devices, own data dir only; portals give a file chooser grant for the selected file only; prefer portals and specific XDG folders over `home`/`host`; `--device=usb` allows any USB device, the narrower USB portal asks per device. Consequence: Flatpak watched folders need a persisted portal grant or a declared filesystem permission; device transfer needs the USB portal or broad permission; `--share=network` also exposes abstract Unix sockets. Flathub's review stance on broad permissions was not stated on that page.

Windows MSIX / device access: the Microsoft page 404'd. [unverified] broadFileSystemAccess and runFullTrust are restricted capabilities that Store review questions; unpackaged or MSIX-with-full-trust is the usual route for a desktop app.

What this means:
- Watched folders: possible on Linux and Windows (with permissions), on macOS via bookmarks, on Android via SAF tree or all-files access; on iOS only while the app runs, via a picked folder or a File Provider location; no background daemon.
- Local server: LAN prompt on Apple platforms; foreground-service rules on Android; sandbox network permission in Flatpak; Mac App Store needs the network-server entitlement [unverified].
- Plugins: data-only plugins everywhere; code plugins only on desktop direct-download builds, not iOS store builds, and discouraged for Play if they download code [unverified for Play].
- Device transfer: no MTP on iOS or Mac App Store without entitlements; use Wi-Fi/OPDS or server push there.

## 3. Accessibility law

- Directive (EU) 2019/882 covers e-books and "dedicated software" and has applied since 28 June 2025 (V via W3C mapping note: w3.org/TR/2023/NOTE-epub-a11y-eaa-mapping-20231227/ for the date and Annex I structure; European Commission page lists e-books among covered products; EUR-Lex text itself failed to load, so microenterprise exemption in Article 4(5) is [unverified]).
- Annex I Section IV(f), e-books, mapped by W3C: (i) audio synchronised with text; (ii) files must not prevent assistive technology; (iii) navigation, layout and presentation must be flexible; (iv) alternative access; (v) accessibility metadata must make features discoverable; (vi) "DRM must not block accessibility features". Section III service requirements: information in more than one sensory channel, adequate font size, shape, contrast and adjustable spacing, alternatives to non-text content, consistent accessible information, accessible support services. The W3C note labels several of these as obligations on reading systems and distributors rather than the file.
- What the software must do: show or expose the publication's accessibility metadata (conformsTo, accessMode, accessibilityFeature, accessibilityHazard, accessibilitySummary, certifier); provide user-adjustable font size, face, contrast, spacing; support screen readers and keyboard-only operation on every platform; play media overlays; not disable text-to-speech or screen reader access on protected files.
- EN 301 549 and WCAG 2.1/2.2 AA are the harmonised yardstick [unverified; page not opened]. US position (ADA, Section 508, 21st Century CVAA) [unverified].
- The Apache app is presumably a "product" if distributed in the EU commercially; an unpaid hobby project's status is unclear. Aim for conformance anyway.

## 4. DRM and copyright

- US DMCA 1201 (V: law.cornell.edu/cfr/text/37/201.40; copyright.gov/1201/ and /1201/2027/):
  - (b)(6): circumvention to enable read-aloud or screen-reader access to literary works "distributed electronically", by an "eligible person" under 17 USC 121 who lawfully obtained the copy and, per the page, where the rights owner is paid the market price of the inaccessible copy, or by an authorised entity.
  - (b)(5) text and data mining: researchers at non-profit higher-education institutions only; copy lawfully acquired and owned or licensed without time limit; security measures; limited access.
  - No preservation exemption for literary works on that page.
  - Exemptions protect the act of circumvention only. Trafficking in tools is separate and not exempted; so the app must not ship, link to or describe tools.
  - Triennial cycle: tenth proceeding is underway; NPRM issued 28 Oct 2026; comments due 7 Dec 2026, 22 Feb 2027, 22 Mar 2027; renewed exemptions run October 2027 to October 2030; the Register intends to readopt all but one existing exemption (the medical-device repair one) (V: copyright.gov/1201/2027/). Which literary exemptions are renewed is not stated there.
- EU position [unverified: Information Society Directive Article 6 and Software Directive; not opened]. Several member states have accessibility exceptions that vary.
- Consequence for the manager: no circumvention code, no scripts, no instructions in the repo, README, issue tracker or plugin index; plugins that strip DRM must not be hosted or recommended by the project. When a user opens a protected file: show a clear "protected by <scheme>; this app cannot open it unless it supports that scheme", name the scheme, offer the supported licensed route, never offer a workaround. Keep the file in the library (metadata and cover can be read from unprotected parts) and flag it as locked.
- LCP (V: edrlab.org/readium-lcp/how-to-lcp-reading-system-provider/): reading app providers get a test module, then must sign Terms of Use and pay an annual fee; EDRLab supplies a production X509 root certificate and a precompiled module; test on EDRLab's sandbox server; submit for certification (test report plus "LCP Certified" logo); EDRLab checks conformity every year. The module is confidential. For this project: LCP can be an optional closed add-on, built only in official release builds; the open repo must still build without it. Whether EDRLab would sign for an open-source app and let the module ship in public binaries is open.
- Adobe ACS (Adept) not planned in the Readium Swift toolkit (V). Apple FairPlay, Kindle and Kobo DRM are not supportable [unverified].

## 5. Privacy

- Reading data can reveal religion, political opinions, sexual orientation and health, so it can fall under GDPR Article 9 special-category data even though no label is stored (V: gdpr-info.eu/art-9-gdpr/, "revealing" wording; applicability to reading data is my inference, not stated).
- US state "reader privacy" statutes exist (California has one) [unverified; page returned 403]. They restrict disclosing which books a person reads without a court order.
- COPPA (V: ftc.gov COPPA FAQ): applies to operators of apps directed to under-13s or with actual knowledge, covering personal information including persistent identifiers; an app that only handles data stored on the device is not collecting it. The rule was amended 22 April 2025 (changes not read). Apple Kids category bars third-party analytics and ads (V).
- "No account, local first" avoids: controller duties for hosted data, parental consent flows, account deletion rules (Apple 5.1.1(v)), cross-border transfer questions, breach notification for reading history. Any hosted sync, Open Library or metadata lookups, or crash reporting reintroduces them; lookups must send only the identifier needed, no library list or user identity.

## 6. Trademarks and naming

- Apple's guidelines (V: apple.com/legal/intellectual-property/guidelinesfor3rdparties.html): use word marks only referentially ("for", "runs on", "compatible with"), not as part of the product name, less prominent than the name, no Apple logo, no implied endorsement, include the attribution line. Kindle and Kobo terms were not opened; expect similar [unverified].
- Practical: feature lists and device profiles may name "Kindle", "Kobo", "EPUB" (EPUB is a W3C specification name, trademark status unverified) as compatibility labels with a trademark notice; never use their icons or device photos; avoid product names like "KindleSync".

## 7. Metadata and covers

- Open Library (V: openlibrary.org/developers/api): 1 request per second, 3 per second when identified with a User-Agent containing app name and contact; cache responses; no bulk download through the API; "Don't use Open Library as a backend for high-traffic commercial services". Data licence not stated on that page.
- Cover images, descriptions and reviews are generally copyrighted by publishers, retailers or reviewers [unverified]. Consequence: store them in the user's local cache as the book's own cover; do not redistribute through a public site; a household server serving the user's own library to household devices is personal use [unverified, jurisdiction dependent]; exports (OPDS, backups) should carry the user's data and flag provenance; do not scrape retailer pages or reviews; keep source and licence per field.
- Wikimedia text (V): CC BY-SA, attribution and share-alike.

## 8. Security

- EPUB Reading Systems 3.3 (V: w3.org/TR/epub-rs-33/): if scripting is supported, a unique origin per publication instance; container-constrained scripts must not modify the host DOM; scripts "should be treated as untrusted and potentially malicious"; systems MAY limit networking; MUST prevent data URLs in top-level contexts and access to file URLs; SHOULD load remote resources only over https; SHOULD notify the user of network activity and allow blocking globally or per publication; SHOULD get consent before opening external links, in a separate browser; MAY block persistent storage and should let users inspect or delete it. Sections 14 and 15 (accessibility and security) were truncated.
- EPUB 3.3 core (V: w3.org/TR/epub-33/): no external entity declarations or XInclude in XML; no file URLs; path limit 65535 bytes, file name limit 255 bytes. Zip-bomb and path-traversal handling are not written there; the parser must enforce its own limits [recommendation].
- Readium's architecture page gives no security guidance (V).
- Local server defaults [recommendation]: bind to loopback until the user enables LAN; per-device token; no directory listing; no CORS wildcard; serve book content from a separate origin to the app UI.
- Sandboxing options by platform: iOS/Android app sandbox; macOS App Sandbox; Linux Flatpak or seccomp; Windows AppContainer or MSIX [unverified]. Run parsers and converters in a child process with no network and a CPU/memory/time cap; treat conversion tools (Pandoc, Ghostscript, ffmpeg) the same.
- Classes of hostile files [unverified, general knowledge]: zip bombs, path traversal and symlinks in archives, oversized images, XML bombs, malicious scripts in EPUB, remote-resource beacons that leak reading, PDF JavaScript and launch actions, embedded fonts exploiting font parsers, malformed MOBI/CBR.

## Data the app must store
- Per file: protection scheme detected (none, LCP, Adobe, Kindle, Kobo, unknown), and whether this build can open it; origin of the file.
- Per book accessibility record: conformsTo, accessMode, accessModeSufficient, accessibilityFeature, accessibilityHazard, accessibilitySummary, certifier, certification date.
- Per metadata field: source, fetch time, licence or terms of that source, user-edited flag; per cover: source, size, whether user-supplied.
- Bookmark records for each watched folder (macOS security-scoped bookmarks, Android persisted URI grants, Flatpak portal grants) with a stale flag and last-verified time.
- Per-publication permission settings: allow scripts, allow network, remote-resource log.
- Consent log for any data leaving the device; per-profile age band flag when child profiles exist.
- Per-plugin: type (data or code), origin, signature, licence.
- Per-device profile: connection type (USB MTP, mass storage, Wi-Fi), trademark-safe display name.

## Requirements (the app must / must not)
- The app must not contain, ship, link to or describe DRM-removal methods; its plugin index must refuse them.
- The app must name the protection scheme and stop when a file is protected by an unsupported scheme; it must keep the book in the library, marked locked.
- The app must build and run without the LCP module; the module is added only in official release builds under EDRLab's Terms of Use.
- The app must not link AGPL or GPL libraries (MuPDF, Poppler, Pandoc, Piper) into the Apache-2.0 binary; it may call GPL tools only as user-installed separate executables on desktop.
- The app must use dynamic linking and ship notices and source for any LGPL library (FFmpeg, libmtp); it must not statically link LGPL on iOS.
- The app must use platform TTS and codecs on mobile.
- The app must read and show accessibility metadata, support screen readers and keyboard-only use, allow user control of font, size, contrast and spacing, and not block assistive technology on protected files.
- The app must not run executable downloaded code on iOS; plugins there are data only.
- The app must treat scripts in books as untrusted: unique origin, no file/data URL top-level access, https only for remote resources, per-book network switch, off by default.
- The app must parse files in a restricted child process with size, depth, time and memory limits.
- The app must bind its local server to loopback by default, and require a token for LAN use.
- The app must not require all-files access on Android by default; use SAF tree grants and keep MANAGE_EXTERNAL_STORAGE to a separate build.
- The app must request only the Flatpak permissions it uses and prefer portals.
- The app must not send library contents or reading history to any service; lookups send only identifiers; they must respect rate limits and a descriptive User-Agent without personal data.
- The app must not redistribute fetched covers, descriptions or reviews beyond the user's household; it must record each field's source.
- The app must attribute CC BY-SA data (Wiktionary) and keep share-alike.
- The app must use vendor names only referentially with a trademark notice, and must not use logos.

Constrained features by platform:
- iOS: code plugins, MTP/USB device transfer, background watched folders, background sync, persistent local server, unattended conversion, multicast discovery without entitlement.
- Android: broad folder watch (SAF or Play-approved all-files), long conversions and servers (foreground service type and notification), USB device access (per-device permission).
- macOS: Mac App Store build cannot read arbitrary folders or raw USB; direct build must be notarised.
- Linux: Flatpak needs portals or declared permissions for folders and USB.
- Windows: restricted capabilities for full file access [unverified].

## Open questions
- Whether EDRLab will certify an open-source reading app and permit distributing the module.
- Which literary-works exemptions the October 2026 US notice proposes to keep, and EU member-state exceptions.
- The text of EUR-Lex Article 4(5) microenterprise exemption and whether a free open-source app counts as placed on the market.
- Whether Google Play accepts a library manager for MANAGE_EXTERNAL_STORAGE.
- Whether Apple treats an OPDS catalogue browser as a store under 3.1.1.
- Atkinson Hyperlegible EULA terms for redistribution; per-language hyphenation and Tesseract data licences.
- Whether static LGPL linking on iOS is acceptable in practice.

## Source URLs opened
developer.apple.com/app-store/review/guidelines/ (2 reads); support.google.com/googleplay/android-developer/answer/10467955; copyright.gov/1201/2024/, /1201/, /1201/2027/; edrlab.org/projects/readium-lcp/, /readium-lcp/, /readium-lcp/how-to-lcp-reading-system-provider/; commission.europa.eu EAA page; apache.org/licenses/GPL-compatibility.html; gnu.org/licenses/license-list.html; w3.org/TR/2023/NOTE-epub-a11y-eaa-mapping-20231227/; artifex.com/licensing; ffmpeg.org/legal.html; github.com/readium/swift-toolkit, /readium/kotlin-toolkit, /mozilla/pdf.js, /OHF-Voice/piper1-gpl, /tesseract-ocr/tesseract; rarlab.com/rar_add.htm; openfontlicense.org/ofl-faq/; pandoc.org; docs.flatpak.org sandbox-permissions; ftc.gov COPPA FAQ; developer.android.com manage-all-files and fgs service-types; law.cornell.edu/cfr/text/37/201.40; readium.org/architecture/; w3.org/TR/epub-33/ and /epub-rs-33/; openlibrary.org/developers/api; apple.com trademark guidelines; gdpr-info.eu/art-9-gdpr/; sqlite.org/copyright.html; dumps.wikimedia.org/legal.html; libmtp.sourceforge.net; brailleinstitute.org/freefont/. Failed: eur-lex (3 attempts, empty), ecfr.gov and federalregister.gov (redirect block, not pursued), developer.apple.com sandbox and local network pages (no body text), learn.microsoft.com MSIX (404), readium.org/about (404), leginfo.legislature.ca.gov (403), accessibility-act.eu (DNS).
