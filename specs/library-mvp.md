# Offline library

The library stores the original EPUB bytes alongside a recoverable catalogue. Desktop supplies stable staged input and the isolated reader's package metadata. The checks below run against temporary libraries in the [crate integration suite](../crates/ebook-library/tests/library.rs).

- **Managed revisions.** Importing changed bytes at a known path or with a known package identifier requires a decision. Keeping a separate book gives it new identities. Replacing a revision keeps the work, edition and file IDs, retains the old bytes, and invalidates the older reading position. Check `exact_duplicate_and_review_preserve_both_revisions`.
- **Exact duplicates.** An exact current version at its known path keeps its identity and position, even when another edition shares its package identifier. Check `shared_identifier_does_not_hide_an_exact_version`.
- **Restore and browse.** After restart or reconstruction from the portable folder, search matches an edited title or author regardless of case. Original bytes, package values, catalogue IDs and the saved chapter and offset survive. Check `portable_only_rebuild_restores_bytes_metadata_and_position`.
- **Durability.** A local commit survives interruption before journal installation or before sidecar publication. Retrying finishes the original change once. Check `publication_interrupted_before_and_after_journal_install_retries_once` and `retry_reports_the_committed_book_while_publication_is_pending`.
- **Edited sidecars.** Replaying a revision preserves an externally edited snapshot as a proposal and restores its generated copy. Check `edited_sidecars_survive_revision_reuse_and_portable_restore`.
- **File integrity.** A symbolic link at a staged or managed file path is refused. Resolving changed managed bytes reports an error. Re-importing the verified original repairs the file. Check `refuses_symlinks_and_rejects_tampered_managed_bytes`.

See the [library contract](../crates/ebook-library/README.md) for the desktop API and recovery format.
