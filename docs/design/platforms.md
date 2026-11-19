# Platform and distribution design

Proposed for [the shared core and UI decision](https://github.com/endqwerty/ebook-manager/issues/3)
and [the distribution decision](https://github.com/endqwerty/ebook-manager/issues/8).
Linux is the first development target. macOS, iOS, and Android are planned, with
Windows to follow. The platform combinations and release packages need prototypes.

## Shared core and native shells

Use Rust for the library, search, jobs, and sync. Keep the domain model independent
of storage and OS APIs. Give the CLI and every shell the same typed commands,
paged queries, and change events. Pass batches across the language boundary and
run long jobs off the UI thread. Use OS-local SQLite in WAL mode and start with
FTS5 for catalogue search. Use keyset pagination and lazy native list and grid
models for the large-library targets in [LIB-70](../requirements/01-library.md#speed).
Measure search, startup, and scrolling at 100,000 books and check usability at
1,000,000 before committing to a more complex index.
[SQLite WAL](https://sqlite.org/wal.html) · [FTS5](https://sqlite.org/fts5.html)

| Target | Native shell | EPUB engine |
|---|---|---|
| Linux | GTK 4 and libadwaita through Rust bindings | WebKitGTK 6.0 |
| macOS | SwiftUI with AppKit collections where needed | WKWebView |
| iOS and iPadOS | SwiftUI with UIKit collections where needed | WKWebView |
| Android | Kotlin and Jetpack Compose with lazy lists and grids | Android WebView |
| Windows | C# and WinUI 3 | Evergreen WebView2 |

Use UniFFI for Swift and Kotlin, and a narrow C ABI for C#. Native shells own
accessibility, focus, and file grants. Shared code owns the behaviour behind their
controls. Maintaining separate shells costs UI work on each target, but gives
each shell the platform's controls and assistive technology APIs. Keep EPUB
layout in the WebView and test pagination and locator round-trips on each engine.
Use continuous text flow for screen readers.
[UniFFI](https://mozilla.github.io/uniffi-rs/) · [GTK](https://docs.gtk.org/gtk4/)
· [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
· [WebKitGTK 6.0](https://webkitgtk.org/reference/webkitgtk/stable/)
· [SwiftUI](https://developer.apple.com/documentation/swiftui)
· [WKWebView](https://developer.apple.com/documentation/webkit/wkwebview)
· [Compose lists and grids](https://developer.android.com/develop/ui/compose/lists)
· [Android WebView](https://developer.android.com/reference/android/webkit/WebView)
· [WebView2 in WinUI 3](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/webview2)

Add formats and device integrations through adapters around the shared operations.
The format adapters run inside workers and return bounded results. Data-only
extensions can describe metadata sources, device profiles, and automation through
the same operations. Keep executable extensions and user-installed command-line
tools as later direct-desktop options. Store and sandboxed packages omit those
options. See [extensions](../requirements/08-sync-sharing.md#extending-the-app).

## Rendering and isolation

- **EPUB.** Unpack and sanitise publications in a worker. Use a shared local
  reader module for pagination, locators, and highlights. Serve book assets on
  a publication-specific origin apart from the privileged app and keep book
  frames outside the native bridge. Disable publisher scripts and remote
  resources by default. If opt-in scripts pass isolation tests, run them in an
  opaque sandboxed frame with no direct network access. Any approved remote
  images or media must come through the app from static, user-approved HTTPS
  URLs. Keep scripting disabled on any engine where this boundary fails.
  [EPUB Reading Systems 3.3](https://www.w3.org/TR/epub-rs-33/)
- **PDF.** Use a pinned PDFium build through `pdfium-render`, with V8 and XFA
  disabled. Workers return bounded raster tiles and text. Use document tags
  for reading order when available and label geometric fallback as approximate.
  Keep launch actions and embedded files inactive. PDF.js is a candidate if
  PDFium packaging fails, subject to the same isolation and accessibility checks.
  [PDFium build options](https://github.com/chromium/pdfium)
  · [PDF.js licence](https://github.com/mozilla/pdf.js/blob/master/LICENSE)
- **Worker.** Keep untrusted parsing, conversion, and image rasterisation outside
  the app process. Pass only the inputs needed for each job, deny networking,
  and bound input, output, memory, and time. Candidates are Landlock and seccomp
  on Linux, signed sandboxed XPC on macOS, AppContainer with Job Objects on
  Windows, an isolated Binder service on Android, and an ExtensionFoundation
  helper on iOS and iPadOS. Treat the iOS helper as a candidate to test on signed
  iOS and iPadOS builds. Apple lists version 26 as the first version on both
  platforms with `AppExtensionProcess`. Deployment targets and extension
  entitlements depend on the helper tests.
  [Linux Landlock](https://www.kernel.org/doc/html/latest/userspace-api/landlock.html)
  · [Apple XPC](https://developer.apple.com/documentation/xpc)
  · [Apple AppExtensionProcess](https://developer.apple.com/documentation/extensionfoundation/appextensionprocess)
  · [Windows process isolation](https://learn.microsoft.com/en-us/windows/win32/secauthz/createprocessinsandbox)
  · [Android service manifest](https://developer.android.com/guide/topics/manifest/service-element)

Before enabling a format on a target, test the worker in the actual package.
Check file grants, denied network access, memory and timeout behaviour, and recovery
after worker termination. On mobile, verify that allocation bounds and OS process
limits are sufficient. If a worker cannot meet SEC-5, omit the affected capability
until the boundary works. Test EPUB origin separation, request blocking, bridge
access, and continuous screen-reader navigation on every shipping WebView.
Upstream API availability and browser-only results leave these checks open.
See [SEC-1 to SEC-9](../requirements/10-constraints.md#opening-untrusted-files).

## Distribution and feature limits

| Target | Planned channels | Channel limits |
|---|---|---|
| Linux | Native packages first. Flatpak as a secondary channel. Defer AppImage. | Flatpak uses portal grants for folders and supported USB devices, and explicit network permission for LAN features. It omits host tools and code extensions. Test USB and worker confinement in the Flatpak runtime. [Flatpak permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html) · [USB portal](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Usb.html) |
| macOS | Developer ID signed, hardened, notarised DMG or ZIP. Consider Homebrew Cask after direct packaging. Defer the Mac App Store. | A future Store build needs App Sandbox grants and entitlements. Omit host tools and code extensions in that build. Treat USB MTP as unverified in both builds. Apple documents a sandbox USB entitlement, so sandboxing alone does not settle MTP access. LAN discovery needs local network privacy checks. [Notarisation](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution) · [App Sandbox](https://developer.apple.com/documentation/xcode/configuring-the-macos-app-sandbox) · [Local network privacy](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy) |
| iOS and iPadOS | App Store and TestFlight. | Use Files folder grants and bookmark recovery. Scan watched folders while open. Rely on scheduled work and background transfers where supported. Omit the persistent LAN server, cable transfer, and executable extensions. Offer sync and Wi-Fi delivery. Ship a native library manager and reader with OPDS for browsing and acquisition, without external checkout steering. [Directory access](https://developer.apple.com/documentation/uikit/providing-access-to-directories) · [Background Tasks](https://developer.apple.com/documentation/backgroundtasks) · [App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/) |
| Android | Google Play first. Consider a direct APK later. Defer F-Droid until the PDFium build can be reproduced from source. | Persist Storage Access Framework grants. Android 11 and later restrict tree grants for locations such as storage roots and Download. The Play build uses folder grants alone. Long jobs must handle foreground service limits. USB MTP needs host support and device permission. Omit executable extensions in both builds. [Shared documents](https://developer.android.com/training/data-storage/shared/documents-files) · [All files policy](https://support.google.com/googleplay/android-developer/answer/10467955) · [Foreground timeouts](https://developer.android.com/develop/background-work/services/fgs/timeout) · [USB host](https://developer.android.com/develop/connectivity/usb/host) · [Play code policy](https://support.google.com/googleplay/android-developer/answer/16559646) · [F-Droid inclusion policy](https://f-droid.org/docs/Inclusion_Policy/) |
| Windows | Signed desktop installer first. Evaluate a portable ZIP and full-trust Store MSIX later. | Check runtime installation and updates, file access, local hosting, and device transfer in the packaged build. Keep host tools and executable extensions in the direct build. [Windows distribution paths](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/choose-distribution-path) · [WebView2 runtime distribution](https://learn.microsoft.com/microsoft-edge/webview2/concepts/distribution) |

Expose channel capabilities in the shell so users only see actions their package
supports. Store acceptance, signing, and registration remain release work. Apply
the keyboard, scaling, and screen-reader requirements in
[READ-70 to READ-76](../requirements/04-reading.md#access) to every channel.

## Licence checks and protected books

The app's own code uses Apache 2.0. The upstream PDFium licence is BSD-style and
`pdfium-render` offers MIT or Apache 2.0. The PDFium binary still needs an audit of
its pinned revision, build flags, and third-party code. SQLite declares its code
public domain. Use shared libraries supplied by the Linux distribution or
Flatpak runtime for GTK, libadwaita, and WebKitGTK. Audit any copies the package
redistributes. Use WKWebView and Android WebView via platform APIs. On Windows,
depend on the Evergreen Runtime through Microsoft's documented installer path.
[PDFium licence](https://pdfium.googlesource.com/pdfium/+/refs/heads/main/LICENSE)
· [pdfium-render manifest](https://github.com/ajrcarey/pdfium-render/blob/master/Cargo.toml)
· [SQLite copyright](https://sqlite.org/copyright.html)
· [GTK licence](https://github.com/GNOME/gtk/blob/main/COPYING)
· [libadwaita licence](https://github.com/GNOME/libadwaita/blob/main/COPYING)
· [WebKit licensing](https://webkit.org/licensing-webkit/)

UniFFI uses MPL 2.0. Check which generated files are covered. Preserve notices
and make the corresponding covered source available to recipients of executable
builds. Keep the C ABI as a fallback if the binding or its terms block a channel.
[UniFFI licence](https://github.com/mozilla/uniffi-rs/blob/main/LICENSE)
· [MPL 2.0 sections 3.2 and 3.3](https://www.mozilla.org/en-US/MPL/2.0/)

Before release, audit resolved dependencies and actual package contents,
including native libraries, generated bindings, reader JavaScript, fonts, models,
and data. The release manifest must list their exact versions and licences and
carry the required notices and source offers or access instructions. Apply
LIC-1 to linked and bundled components. The component choices above identify
candidates and obligations. Package-level clearance remains open.
[Project licence requirements](../requirements/10-constraints.md#licences-of-parts)

Defer Readium LCP. EDRLab documents a production module, confidential certificate
material, signed terms, and annual conformity checks and fees. Thorium shows that
an open-source app can ship LCP with a closed module. Confirm Ebook Manager's
eligibility and terms before planning a certified build. The public build must
work without the module or production material and retain unsupported books as
described in LAW-2. Any future protected-book adapter must preserve the access
required by READ-74.
[EDRLab provider process](https://www.edrlab.org/readium-lcp/how-to-lcp-reading-system-provider/)
· [EDRLab LCP FAQ](https://www.edrlab.org/readium-lcp/faq/)
· [Thorium Reader](https://www.edrlab.org/software/thorium-reader/)
