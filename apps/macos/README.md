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
adaptation to `reader.js` or `reader.css` was needed. During `swift test`,
`ReaderAssets` loads the same two files directly from `apps/desktop/src/reader/`.

## What callers and users can rely on

- **All bridge calls run off the main actor.** `LibraryActor` dispatches every
  synchronous `ebook-bridge` call on its own background executor.
- **Sandboxed helper beside the executable.** `build.sh` places
  `ebook-reader-helper` next to `EbookManager` inside `Contents/MacOS` and
  signs both ad hoc (`codesign -s -`).
- **Locked-down reader web view.** `ReaderWebController` uses a non-persistent
  data store, sets `allowsContentJavaScript = false` for book markup, injects
  `reader.js` in a separate `WKContentWorld`, compiles a `WKContentRuleList`
  that blocks every URL outside `ebook:`, enforces a strict
  `Content-Security-Policy` on `ebook:` `application/xhtml+xml` responses, and
  permits only the navigation URL just requested for the active load.

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
