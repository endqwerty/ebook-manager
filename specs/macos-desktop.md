# macOS desktop library and reader

The macOS app keeps a local EPUB library in
`~/Library/Application Support/Ebook Manager`. Launching with
`--library <directory>` selects a different library.

- Add Files and Add Folder show an outcome for each imported EPUB. An exact
  duplicate leaves the existing book in place. A file that needs review can be
  skipped, kept as a separate book or used to replace the selected book's file.
- The library window shows books in a grid or list. Search finds a book by
  title or author. An edit to the catalogue title or authors survives reopening
  the library. Go to searches across books, authors and series. Show Details
  displays the selected book and offers Read and Edit. The managed EPUB
  retains its original bytes.
- Refreshing the same search or sort keeps the visible book in view. Cover
  rebuilds have a separate bounded queue. Background surveys continue past
  unreadable books and retry them after Refresh or Try Again.
- Read opens a reader window. Its contents list jumps to a chapter, and the
  reader saves the chapter and position for the next session. The isolated
  helper reads EPUB data. The web view renders chapter markup, blocks book
  scripts and restricts resource requests to the reader's own scheme.
- Send to Device Folder copies the managed EPUB under a name derived from
  the catalogue title. Repeating the export recognises an identical copy.
  See [mounted reader export](mounted-epub-export.md) for collisions, space
  requirements and interrupted transfers.

- Opening a library restored from its portable folder and choosing Refresh
  Library reconstructs the catalogue, including edits and reading position.

[Library storage and recovery](library-mvp.md) describes the shared library.
[Building and testing the macOS app](../apps/macos/README.md) gives the bundle
and Swift test commands.
