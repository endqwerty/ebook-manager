# MOBI reading copies

The Linux [conversion crate](../crates/ebook-convert/README.md) creates a reflowable EPUB from a supported unprotected older Kindle book. The caller supplies source and destination paths. The library catalogue and desktop do not yet expose the job.

- **E1** The converter reads the managed source through a private snapshot, inside a short-lived sandbox. Protected, unsupported, malformed and over-limit inputs have distinct outcomes. A failed sandbox stops conversion.
- **E2** An EPUB has a new publication identifier, package metadata, ordered spine and navigation. Headings, paragraphs and supported images retain their order. Text is escaped. XML-forbidden characters are replaced with � and reported. An invalid source language becomes undetermined and is reported. An invalid caller-supplied language returns an error. Optional catalogue metadata and cover take precedence. The source retains its original identifiers.
- **E3** The job exposes stage progress and cancellation. Its terminal result distinguishes cancellation, failure and publication. Retry starts a fresh job. Caller-provided version identity, output size, converter version, direction and losses are available in a serialisable success report. A direction-level loss description is available before start.
- **E4** The job checks each staged chapter through the independent EPUB reader before publishing a new file. Errors and cancellation discard staged output. Publication never overwrites an existing file. Concurrent successes cannot claim the same destination. The source bytes remain unchanged.
- **E5** Acceptance includes an EPUBCheck pass on converter output and checks of actual text and metadata against expectations. No physical device is required.

- **E6** Before opening any input or setting resource limits, the single-threaded helper installs fatal signal handlers on an alternate stack. A fatal signal ends the helper with exit code 4. The host returns `Error::ConversionStopped` and discards staging. This implements SEC-10 for the converter process.

The caller owns catalogue links and history. Source locations and annotation identifiers are not mapped across formats.
