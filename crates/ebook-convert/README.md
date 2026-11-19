# Ebook conversion on Linux

Start a job with `ebook_convert::start(request, config)`. The [runnable caller](examples/convert-book.rs) converts one managed source path to a fresh EPUB and prints its JSON report. The [design](../../docs/design/mobi-to-epub.md) explains the helper boundary and catalogue integration. The [behaviour spec](../../specs/mobi-to-epub.md) lists acceptance checks.

The caller owns the managed version and chooses a destination in a trusted directory. Put the library book ID, version ID and SHA-256 into any string representation and pass it as `Request::identity`. Only the caller interprets it. Supply optional title, author list, language and a JPEG or PNG cover in memory. Show `LOSS_DESCRIPTION` before starting. Supply a trusted absolute path to the built converter helper and the reader configuration.

Poll `job.stage()`, call `job.cancel()` to request cancellation and call `job.wait()` for the terminal result. On success, compare the identity in the report to the intended version before attaching it. Another job may already own the requested destination. `Error::Exists` means this job published nothing. Retry an unsuccessful request by calling `start` again. Dropping the handle detaches the running thread. The coordinator owns the catalogue transaction, desktop queue and helper installation.

The host copies the input into private scratch space. The read-only converter sandbox returns bytes over stdout. A second sandbox opens the staged EPUB and reads each chapter. Hidden staging files have no EPUB extension. Normal exits clean up staging and scratch. If the host process dies, its caller may remove orphaned staging files in its own output directory.

Build the helpers and run an independent conformance check on a converted book:

```sh
cargo build --release --locked --workspace --bins
cargo run --release --locked -p ebook-convert --example convert-book -- \
  book.mobi new.epub target/release/ebook-convert-helper target/release/ebook-reader-helper
java -jar "$EPUBCHECK_JAR" new.epub
```

EPUBCheck is a standalone acceptance tool. Runtime jobs use the repository's reader. The package and navigation follow [EPUB 3.3](https://www.w3.org/TR/epub-33/).

The new lockfile dependencies chrono, num-traits and language-tags each offer an MIT or Apache-2.0 licence. The format reader is part of this project.
