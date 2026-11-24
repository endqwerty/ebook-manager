# ebook-bridge

Joins `ebook-library`, `ebook-reader` and `ebook-device` behind one synchronous
UniFFI API (`lib` and `staticlib`) for native shells such as `apps/macos`. The
design is in [`docs/design/macos-app.md`](../../docs/design/macos-app.md#the-bridge).

## Entry point

```rust
use ebook_bridge::{Library, ReviewChoice, SortField, SortRule};

let library = Library::open(app_data_dir)?;
let results = library.import_paths(vec![folder_or_file]);
let snapshot = library.browse(
    String::new(),
    vec![SortRule { field: SortField::Title, descending: false }],
)?;
let rows = library.browse_window(snapshot.view_id, 0, 128)?;
let png = library.cover_thumbnail(rows[0].id.clone())?;
let book = library.clone().open_book(rows[0].id.clone())?;
let chapter = book.read_content(0)?;
book.save_place(0, 120, 0, 4)?;
```

Generate Swift bindings from the compiled library with:

```sh
cargo run --release -p ebook-bridge --features cli --bin uniffi-bindgen -- \
    generate --library target/release/libebook_bridge.a --language swift --out-dir out
```

## Survey policy

Survey runs in both places, matching the Linux desktop shell:

1. **During import staging.** `import_paths` inspects and surveys each staged
   file in the sandboxed helper before committing the book, writing its cover
   thumbnail to `<library>/cache/thumbs/<digest>.png` so newly imported books
   appear immediately with their word count, cover colours and thumbnail. A
   survey failure during staging leaves the book unsurveyed rather than failing
   the import.
2. **In the background and on demand for unsurveyed or restored books.** The
   shell calls `Library::survey_unsurveyed(limit, skip)` in batches at startup
   and after Refresh or Try Again. Pass failed IDs into subsequent batches so
   other books can progress. A new survey retries those IDs. This fills in
   books that lack survey version 1. `Library::cover_thumbnail(book_id)` also
   remakes any missing or tampered file in `cache/thumbs` from the managed EPUB.

## What callers can rely on

- **Every call is synchronous and blocking.** Call from a background executor,
  never from the UI main thread. `Library` separates its write connection from
  its read-only SQLite `BrowseReader` so browsing does not wait behind writes.
- **No book byte is parsed in the app process.** Metadata inspection, survey,
  chapter markup (`read_content`) and pictures (`read_resource`) all run in
  `ebook-reader-helper` inside its sandbox.
- **One error type with user-facing text.** Every failure crosses as
  `BridgeError`, whose `Display` text and `message` field can be shown directly
  to a user. `BridgeError::Locked` distinguishes a protected book so the shell
  can show the locked-book alert.
- **A saved change is never reported as a failure.** When the catalogue commits
  an import, review settlement or edit but the portable record cannot be written,
  the result carries `publication_pending: true` instead of an error. `refresh`
  retries publication, and so does saving the same edit or importing the same
  file again; neither records a second change.
- **Edits are one change.** `edit_book` saves the title, authors, sort title,
  author sort names and series in a single journal entry, so a failure leaves
  either all of them or none.
- **Review tokens are spent once.** `settle_review` keeps its token after a
  failure so the choice can be retried, and spends it once the choice is saved
  or skipped. It returns the settled `ImportItemResult`, with `Skipped` for a
  skip. `discard_reviews` releases tokens the shell will not settle, such as
  those in a dismissed or replaced import report.
- **No internal library paths cross the boundary.** Records identify books and
  resources by ID; only `export_book` returns the chosen destination path.
- **Checked cover PNGs.** Files in `cache/thumbs` are read with `O_NOFOLLOW`,
  capped at 512 KiB, verified to be non-interlaced 8-bit RGB PNGs within
  256x384 containing only `IHDR`, `IDAT` and `IEND`, and hashed against the
  digest in the book's record before bytes are returned.

## Side effects

- Creates and locks `<directory>/library.lock`, `<directory>/library.sqlite`,
  `<directory>/portable/` and `<directory>/cache/thumbs/`.
- Keeps staged files on disk in the system temporary directory while an import
  awaits review. The in-memory token table owns the files until the review is
  settled, skipped, discarded or dropped.
- Spawns sandboxed `ebook-reader-helper` processes through `ebook-reader` for
  import, survey and `OpenBook` sessions; dropping `OpenBook` ends its helper.

## Checking it

```sh
cargo test -p ebook-bridge
cargo clippy -p ebook-bridge --all-targets -- -D warnings
```

The integration tests in `tests/bridge.rs` generate synthetic EPUB fixtures in
temporary directories and run against the real `ebook-reader-helper` sandbox.
