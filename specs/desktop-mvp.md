# Linux desktop library

## Acceptance walk-through

Use an empty XDG data directory and generated EPUBs. Start the app, choose a folder containing a multi-chapter EPUB, an exact duplicate, a revised copy and an invalid file. Check each result and resolve the revision. Search by author, edit a title and author, and find the new values. Read chapter two, close the application, reopen the same book and check its reading position. Choose an export directory twice and compare the exported bytes to the imported EPUB. Keep the source files intact throughout.

## Shelf and import

- The file picker accepts multiple EPUBs. Folder import finds EPUBs recursively, follows links to EPUB files, skips directory links, and reports unreadable directories. Staging, inspection and catalogue writes run off the GTK thread. The report lists the result beside each filename and lets the reader stop after the current book.
- Exact duplicates keep a single shelf entry. If the library identifies a possible revision, the reader can skip it, keep it separately or select a matching book to replace. A failed or skipped file leaves the remaining queue available.
- Search matches shelf metadata. Each row opens its own reader, edit window or export picker. Edits accept a title and one author per line, survive restart and leave the EPUB unchanged.

## Read and export

- Open a book to read its text in spine order. Choose a chapter from the list or use Previous and Next. Each paragraph begins on a new line. External links remain inert.
- When scrolling settles and on leaving the reader, the library records the first visible block and chapter for that file version. Reopening it after restart returns to that place. The chapter indicator appears once the text viewport reaches the saved block or the last page containing it. A different version begins at its own saved position or the first chapter.
- Export writes the managed EPUB into an existing user-selected folder. Choose a mounted SD-card directory for an Xteink X4 Pro in CrossPoint USB Drive mode. The result shows the written path or reports that a matching copy already exists. The smoke check uses a local temporary destination. Physical-reader behaviour remains untested.
