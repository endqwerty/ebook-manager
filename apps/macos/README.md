# macOS shell

SwiftUI and AppKit shell for macOS 14 or later, calling the shared Rust core
through [`crates/ebook-bridge`](../../crates/ebook-bridge/README.md) and
rendering EPUB chapters in an isolated `WKWebView`. See
[`docs/design/macos-app.md`](../../docs/design/macos-app.md).

## Entry point

Build the release app bundle from the repository root:

```sh
apps/macos/build.sh
open "apps/macos/build/Ebook Manager.app"
```

Pass `--library <dir>` or set `EBOOK_MANAGER_LIBRARY_DIR=<dir>` to point the app
at a scratch library folder instead of the default
`~/Library/Application Support/Ebook Manager`.

## Shared reader script and style

`apps/macos/build.sh` copies `apps/desktop/src/reader/reader.js` and
`apps/desktop/src/reader/reader.css` directly into
`Ebook Manager.app/Contents/Resources/` without modifying or forking either
file. Both WebKitGTK 6.0 and `WKWebView` use `window.webkit.messageHandlers.reader.postMessage`
and `callAsyncJavaScript` in a dedicated script world (`WKContentWorld`), so no
edits to `reader.js` or `reader.css` were needed. Two `WKWebView` differences
are handled in `ReaderWebController` around those unchanged files:

1. Setting `WKWebpagePreferences.allowsContentJavaScript = false` in `WKWebView`
   also disables `WKUserScript` in isolated `WKContentWorld`s (unlike WebKitGTK's
   `enable-javascript-markup = false`). Instead, `ReaderWebController` leaves
   `allowsContentJavaScript = true` so `reader.js` can run in its isolated
   `WKContentWorld`, while every `ebook:` chapter response carries a
   `Content-Security-Policy` (`default-src 'none'; img-src ebook:; style-src
   ebook: 'unsafe-inline'; form-action 'none'; base-uri 'none'`) in both its
   HTTP response header and `<meta http-equiv="Content-Security-Policy">`,
   blocking all inline `<script>` elements, event-handler attributes and external
   scripts in the page world.
2. `WKWebView` has no public `WKUserStyleSheet` API, so `/s/reader.css` is served
   on the per-view `ebook:` scheme and linked from the wrapper `<head>`. During
   `swift test` (outside an `.app` bundle), `ReaderAssets` reads `reader.js` and
   `reader.css` directly from `apps/desktop/src/reader/`. When a Mac runs
   headless or with its display asleep, WebKit pauses `requestAnimationFrame`; a
   small `.atDocumentStart` shim in the reader's `WKContentWorld` falls back to a
   32 ms timer when `requestAnimationFrame` does not fire, so `reader.js` never
   stalls on headless runners. `ReaderWindowViewModel` and `ReaderWebController`
   also pass `--dark-images`, `--line-focus` and `--focus-dim` and call
   `__reader.stepRuler` / `__reader.resetRuler` added on `main`.

## What callers and users can rely on

- **All bridge calls run off the main actor.** `BridgeService` and
  `OpenBookHandle` dispatch every synchronous `ebook-bridge` call onto dedicated
  background `DispatchQueue`s, separating catalogue reads from writes.
- **Sandboxed helper beside the executable.** `build.sh` places
  `ebook-reader-helper` next to `EbookManager` inside `Contents/MacOS` and
  signs both ad hoc (`codesign -s -`).
- **Locked-down reader web view.** `ReaderWebController` uses a non-persistent
  `WKWebsiteDataStore`, keeps `allowsContentJavaScript = true` solely so WebKit
  executes `WKUserScript`s in the isolated `ebook-reader` `WKContentWorld`,
  blocks all book-markup scripts and inline handlers via the `ebook:`
  `Content-Security-Policy` (`default-src 'none'`), compiles a
  `WKContentRuleList` that blocks every URL outside `ebook:`, serves resources
  only for the view's own random host token, and permits only the single
  navigation URL just requested for the active chapter load.

## Side effects

- Reads and writes the chosen library directory (`~/Library/Application Support/Ebook Manager`
  by default) through `ebook-bridge`.
- Compiles one `WKContentRuleList` in WebKit's local store and holds a
  non-persistent `WKWebsiteDataStore` per reader window.

## Checking it

```sh
apps/macos/build.sh test
apps/macos/build.sh
```

`build.sh test` builds `ebook-bridge` and `ebook-reader-helper` in debug mode,
generates the UniFFI Swift bindings, and runs `swift test` covering both
window-free logic (`EbookShellCoreTests`) and the `WKWebView` isolation and
bridge integration checks (`EbookManagerKitTests`).
