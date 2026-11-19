# Linux desktop library

## Acceptance walk-through

Use an empty XDG data directory and generated EPUBs. Start the app, add a folder containing a multi-chapter EPUB, an exact duplicate, a revised copy and an invalid file. Check each result and resolve the revision. Search by typing, then edit the title, author, sort title, author sort name, series and position from the keyboard and find the new values. Read chapter two, close the application, reopen the same book and check its reading position. Send the book to a folder twice and compare the exported bytes to the imported EPUB. Keep the source files intact throughout. [`smoke.py`](../apps/desktop/tests/smoke.py) does this with the keyboard, and [`scale.py`](../apps/desktop/tests/scale.py) repeats the library steps with 100,000 generated books.

## The library window

- The window shows one place, All books, with the number of books beside its name. The books are a grid of covers in title order: a leading article is skipped by the book's language, and numbers sort by value.
- Every cover is a 2:3 frame of the same size. A book with a cover image shows it with an edge line, a shade on the side it is bound and a shadow. A book without one shows a binding set in type: its title and author on one of eight colours chosen from the book's ID. While a cover's image is being read its frame is the cover's average colour. Under More contrast the shade and shadows go and an outline is drawn around the cover. The app follows the system's light or dark appearance.
- A partly read book has a line under its cover, as long as the part read. A finished book has a check mark after its author. A screen reader hears both in words with the title and author.
- The number of columns follows the window's width and the system's text size. Covers grow to share a row, up to a quarter over the medium width of 128.
- With no books the window says so and offers Add. When a search matches nothing it names the text searched for and offers Clear Search.
- A book the reader was on stays selected, focused and in place when the books shown are replaced by a search or by a change to the library. During an import the list is refreshed at most once a second.
- Closing the window first closes the windows opened from it, so each saves what it holds.

## Search

- Typing anywhere in the window opens the search with the typed text. The books shown are those whose title, an author or a series name contains every word, whatever its case or accents.
- Down moves from the search field to the books it found and selects the first. Typing with a book focused goes on to the search field. Escape closes the search from anywhere in the window.

## Commands and keys

| Command | Keys |
|---|---|
| Add files, add a folder | Ctrl+O, Ctrl+Shift+O, or the Add menu |
| Search | Typing, or Ctrl+F |
| Read | Return or a double click |
| Edit | F2 |
| Send to device | Ctrl+D |
| A book's menu: Read, Edit, Send to Device | The Menu key, Shift+F10 or a secondary click |
| Refresh library | F5, Ctrl+R, or the main menu |
| Close | Ctrl+W, Ctrl+Q |

- Read and Edit act on the one selected book, and Send to device on every selected book. A book's menu acts on that book and leaves the selection alone.
- Refresh library checks every journal file against the catalogue and finishes any change that was saved but not published.

## Import

- The file picker accepts multiple EPUBs. Folder import finds EPUBs recursively, follows links to EPUB files, skips directory links, and reports unreadable directories. Staging, inspection and catalogue writes run off the GTK thread. The report lists the result beside each filename and lets the reader stop after the current book.
- Exact duplicates keep a single entry. If the library identifies a possible revision, the reader can skip it, keep it separately or select a matching book to replace. A failed or skipped file leaves the remaining queue available.
- Import keeps what the file says about each author's sort name, its series and position, its date, its first language and its page direction. It counts the book's words and reads its cover in the reader's sandbox. A cover that cannot be used leaves the book with a binding set in type.
- Cover thumbnails are PNG files in `cache/thumbs` under the app's data folder, named by the digest in the book's record. The folder is a cache. A thumbnail that is missing when its cover is shown is made again from the managed EPUB in the background, and so are the covers of books imported before covers were read.

## Edit

- The Edit window holds the title, the authors one to a line, the sort title, each author's sort name, and the series with the book's position in it. A position is text, such as `2`, `1.5` or `TPB1`.
- A sort title, sort name or series left as the file gives it follows the file. An empty sort title files the book by its title, and an empty sort name files that author by the name as written. A book in more than one series keeps the others when the first is edited.
- Edits survive restart and leave the EPUB unchanged. Tab moves between the fields, Return in a one-line field saves and Escape cancels.

## Read and export

- Open a book to read its text in spine order. Choose a chapter from the list or use Previous and Next. Each paragraph begins on a new line. External links remain inert.
- Right, Page Down and Space turn one page forward. Left, Page Up and Shift+Space turn one page back. A page starts with the first line the previous page could not show whole, so a turn skips no line and cuts none at the top. A chapter's last page ends where the chapter ends, so its first line can be cut. That line was whole on the page before. A turn back shows that line whole, so a chapter's first page can open without the margin above its first line. A turn forward on the last page opens the next chapter at its start. A turn back on a chapter's first page opens the previous chapter on its last page. Up and Down scroll by a few lines. While the chapter list has keyboard focus it keeps the arrow keys. Space presses a focused button.
- F11 or **Full screen** fills the screen with the reader. F11, Escape or **Leave full screen** returns to the window. While the chapter list is open, Escape closes the list. The text column keeps a readable width on a wide screen, and the chapter controls stay visible.
- When scrolling settles and on leaving the reader, the library records the first visible block and chapter for that file version, the time, and how much of the book has been read. Reopening it after restart returns to that place. The chapter indicator appears once the text viewport reaches the saved block or the last page containing it. A different version begins at its own saved position or the first chapter.
- The part read counts every chapter in the reading line the same: the chapters before the one on screen plus the part of it scrolled past. A book is finished when the end of its last chapter has been shown.
- Send to device writes the managed EPUB into an existing folder the reader chooses. Choose a mounted SD-card directory for an Xteink X4 Pro in CrossPoint USB Drive mode. The result shows the written path or reports that a matching copy already exists. The smoke check uses a local temporary destination. Physical-reader behaviour remains untested.

## Large libraries

- The window asks for books in pages of 128 and creates rows only for what is on screen, so its speed does not depend on the number of books.
- With 100,000 generated books the first titles can be read through the accessibility tree a quarter of a second after the process starts, against a limit of 2 seconds. A search is answered as the reader types, and Ctrl+End shows the last rows. `scale.py` checks all three and the import behaviour above.
