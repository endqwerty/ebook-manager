# Linux desktop library

## Acceptance walk-through

Use an empty XDG data directory and generated EPUBs. Start the app, add a folder containing a multi-chapter EPUB, an exact duplicate, a revised copy and an invalid file. Check each result and resolve the revision. Switch between the grid, the list and the table and sort each. Search by typing, then edit the title, author, sort title, author sort name, series and position from the keyboard and find the new values. Read chapter two, close the application, reopen the same book and check its reading position. Send the book to a folder twice and compare the exported bytes to the imported EPUB. Keep the source files intact throughout. [`smoke.py`](../apps/desktop/tests/smoke.py) exercises the keyboard commands and window resizes, and [`scale.py`](../apps/desktop/tests/scale.py) uses 100,000 generated books to check launch, scroll position when switching views, search, jumping to the last row, and background imports.

## The library window

- The window shows one place, All books, with the number of books beside its name, as a grid of covers, a list or a table. A place with no sort chosen is in title order: a leading article is skipped by the book's language, and numbers sort by value.
- Every cover is a 2:3 frame of the same size. A book with a cover image shows it with an edge line, a shade on the side it is bound and a shadow. A book without one shows a binding set in type: its title and author on one of eight colours chosen from the book's ID. While a cover's image is being read its frame is the cover's average colour. Under More contrast the shade and shadows go and an outline is drawn around the cover. The app follows the system's light or dark appearance.
- A partly read book has a line under its cover, as long as the part read. A finished book has a check mark after its author. A screen reader hears both in words with the title and author.
- The grid's number of columns follows the window's width, the cover size and the system's text size. Covers grow to share a row, up to a quarter over the chosen width: 96 for small covers, 128 for medium and 160 for large.
- With no books the window says so and offers Add. When a search matches nothing it names the text searched for and offers Clear Search. When the books asked for cannot be opened it says why and offers Try Again, and shows no books of an earlier search. When the books on screen can no longer be read, a command on a book whose row had not loaded says so, and the window opens the books again by itself.
- A book the reader was on stays selected, focused and in place when the books shown are replaced by a search or by a change to the library. The selection is kept as the books themselves and not their places, so new books never arrive with an old selection. If the reader scrolls while their place is being found, the new books are shown and the place they moved to is found once more, and if they move again the view stays where they put it. One case is not caught: a drag or a flick that was already under way when a refresh placed the view sends no new press, so the view can be put back once to where the drag had it at that moment. A view that cannot be read when it is about to be shown is an error with Try Again, and showing an error clears the selection. During an import the list is refreshed at most once a second.
- Closing the window first closes the windows opened from it, so each saves what it holds.

## List and table

- A list row is a small cover, the title, the author, and the series with the book's place in it. At its trailing edge is the percentage read, a check mark for a finished book, or, when the list is sorted by a date, that date. The cover is 40, 48 or 60 wide with the cover size.
- The table has a column for each fact: Title, Author, Series, Date Added and Progress to start with, and Date Published and Last Read to add. Title has a small cover and cannot be removed. A column's width can be dragged, and a header dragged to another place.
- The table is offered when the window content is at least 640 wide. Under that the view switch is a menu without it, and a window set to the table shows the list. When the system's text size is one and a half times or more, a window set to the grid shows the list. Neither changes the choice.
- From the first row, Up reaches the table's headers. On a header, Left and Right move along them, Return sorts by that column and again turns the sort round, Control+Return adds the column as the next sort key or turns that key round, and the Menu key or Shift+F10 opens the header's menu. Down returns to the books. A click and a Control+click do what Return and Control+Return do. A second and third key show a small 2 and 3 in their headers.
- A header's menu shows and hides columns, moves its column toward the start or the end, and offers **Make Wider** and **Make Narrower** in steps of 24.

## Sorting

- The Sort menu sorts by title, author, series, date added, date published or last read, ascending or descending. More opens the sort editor: three rows, each a field and a direction, for a second and third key. Progress is a field there too.
- Each view has its own sort. A sort the reader chooses starts at the book they were on if it is selected or focused, and at the top otherwise.
- A book with no value for a field sorts after those that have one, in both directions.

## Remembered choices

- The view, each view's sort, the cover size, and the table's columns with their order and widths are kept in `settings.json` in the app's config folder and are as they were left after a restart. Each view's sort falls back independently and unknown column entries are skipped. Fractional saved widths round to the nearest pixel. Closing the window leaves the settings file alone when the saved choices match the current ones.

## Search

- Typing anywhere in the window but in a text field opens the search with the typed text. No key typed while it opens is lost, and the keys go through the search field's input method: a dead acute and then `e` searches for `é`, and Backspace or Escape after the dead key cancels it. The books shown are those whose title, an author or a series name contains every word, whatever its case or accents.
- Down moves from the search field to the books it found and selects the first. Typing with a book focused goes on to the search field. Escape closes the search from anywhere in the window.

## Commands and keys

| Command | Keys |
|---|---|
| Add files, add a folder | Ctrl+O, Ctrl+Shift+O, or the Add menu |
| Grid, list, table | Ctrl+1, Ctrl+2, Ctrl+3, or the view switch |
| Sort | The Sort menu, a table header, or Ctrl+Shift+S for the sort editor |
| Small, medium or large covers | The main menu, F10 |
| Search | Typing, or Ctrl+F |
| Read | Return or a double click |
| Edit | F2 |
| Send to device | Ctrl+D |
| A book's menu: Read, Edit, Send to Device | The Menu key, Shift+F10 or a secondary click |
| Refresh library | F5, Ctrl+R, or the main menu |
| Close | Ctrl+W, Ctrl+Q |

- A command acts on the books selected when it is given, whatever is selected by the time it runs. Every key acts on the selection, Return included: when the focused book is not the selected one, Return reads the selected one. Read and Edit need exactly one selected book, and Send to device takes any number. A book's menu acts on that book and leaves the selection alone. Book and column menus close before a view is replaced or when the window width changes.
- Refresh library checks every journal file against the catalogue and finishes any change that was saved but not published.

## Import

- The file picker accepts multiple EPUBs. Folder import finds EPUBs recursively, follows links to EPUB files, skips directory links, and reports unreadable directories. Staging, inspection and catalogue writes run off the GTK thread. The report lists the result beside each filename and lets the reader stop after the current book.
- Exact duplicates keep a single entry. If the library identifies a possible revision, the reader can skip it, keep it separately or select a matching book to replace. A failed or skipped file leaves the remaining queue available.
- Import keeps what the file says about each author's sort name, its series and position, its date, its first language and its page direction. It counts the book's words and reads its cover in the reader's sandbox. A cover that cannot be used leaves the book with a binding set in type.
- Cover thumbnails are PNG files in `cache/thumbs` under the app's data folder, named by the digest in the book's record. The folder is a cache. A file there is shown only if it is the kind the app writes: a regular file of at most 512 KiB, a PNG holding a header, image data and an end and nothing else, of eight-bit red, green and blue with no alpha or interlacing, no larger than 256 by 384, whose pixels hash to the digest it is named by. What is shown is made from those pixels. A file that is missing or fails that check is made again in the background from the managed EPUB of any book that has that cover, and so are the covers of books imported before covers were read.

## Edit

- The Edit window holds the title, the authors one to a line, the sort title, each author's sort name, and the series with the book's position in it. A position is text, such as `2`, `1.5` or `TPB1`.
- A sort title, sort name or series left as the file gives it follows the file. An empty sort title or sort name is worked out by the library: the title without its leading article, and the author's name with its last word first, as `Melville, Herman`. A book in more than one series keeps the others when the first is edited.
- Edits survive restart and leave the EPUB unchanged. Tab moves between the fields, Return in a one-line field saves and Escape cancels.

## Read and export

- Open a book to read it in spine order, a page at a time. The page shows the book's headings, lists, tables, emphasis and pictures in the reader's own type. The book's style sheets are not applied yet. Choose a chapter from the list or use Previous and Next.
- Until the reader chooses a page colour, the page is Paper (`#f4ecd8` with `#3b2f21` text) when the window is light and Grey (`#2b2b2e` with `#d6d6d8` text) when it is dark, and changes with the window. The window around the page takes the page's colour. Links are the text colour, underlined.
- **Aa** opens the text settings. Its first level has the page colour (White, Paper, Grey, Black, and a colour of the reader's own), the font, and the size in twelve steps from 0.8 to 2.5 times the default. **More** has weight, line, letter and word spacing, margins (narrow, medium, wide), justification, which turns hyphenation on with it, hyphenation, two columns, continuous scrolling, the page turn (slide, cross-fade, none, and none whenever the system asks for less motion), whether the page colour follows light and dark, the reader's own page and text colours with their contrast as a number and a warning under 4.5:1, and Reset. Every change shows on the page at once and keeps the place. The settings apply to every book, are kept in `reader.conf` in the app's configuration folder, and are in force the next time a book opens. Under the system's More contrast setting White and Black use pure black and white text and Paper and Grey give way to them.
- The text column is centred and at most about 68 characters wide, in a window of any width. Two columns show when they are turned on and the window fits two of 60 characters. In continuous scrolling the chapter is one column to scroll, the page keys move a screen less a line, so the last line stays in sight, and the line under the page counts screens.
- Right, Page Down and Space turn one page forward. Left, Page Up and Shift+Space turn one page back. A click in the trailing quarter of the page turns forward and one in the leading quarter turns back. In a chapter the book marks as right to left, Left and Right and the two quarters swap. A page holds whole lines, and a turn skips none and repeats none. A turn forward on a chapter's last page opens the next chapter at its start. A turn back on a chapter's first page opens the previous chapter on its last page. While the chapter list has keyboard focus it keeps the arrow keys. Space presses a focused button.
- The line under the page says the chapter and the page, as in "Chapter 2 of 14 · Page 3 of 20", and says so when some of the chapter could not be displayed.
- A link to a place in the book goes there. A link out of the book shows its address and opens it in the system's browser or mail program only when the reader chooses Open.
- F11 or **Full screen** fills the screen with the reader. F11, Escape or **Leave full screen** returns to the window. While the chapter list is open, Escape closes the list. The chapter controls stay visible.
- The reader always holds the place it is at: every page turn, every chapter opened at its landing place and, in scrolling, every scroll that has settled tells it where the reader now is. A change of window size or of a text setting changes the page numbers shown and nothing that is recorded: not the place, not how far through the book it counts as, and it writes nothing to the library. A page turned in one chapter just before another is chosen is still the place recorded until the new chapter has opened. The library records the place, with the chapter and the first character of the page, for that file version, a third of a second after the last move and on leaving, always in the order the places were reached, so an earlier place is never written over a later one. Leaving is the same whoever asks: **Back to library**, the reader's close button, the window the reader was opened from being destroyed, or the app quitting. When the window the reader was opened from is destroyed from outside, as when a window manager ends, the place is saved and the app ends, with no window or process left behind. That is not checked for the moment between choosing a book and its reader being ready, on GTK before 4.24 ([#108](https://github.com/endqwerty/ebook-manager/issues/108)). Each writes the place the reader holds, waits up to two seconds for it, and ends the process that drew the pages. While the reader is open the library window cannot be closed by its own close button. Two things are not saved. A move the page has made but not yet told the reader about when the process ends is lost: at most the last move, a few milliseconds old. And an app ended by a signal, a crash or the loss of its display saves nothing after its last recorded place, so the moves of the last third of a second can be lost. Reopening it after restart shows the page holding that character, at any window size. A page with no text on it, such as a full-page picture, is recorded as the first text after it, or as the chapter's end when no text follows. So after a restart it reopens on the next page that has text, or on the chapter's last page. A book never opened starts on its first page whatever is on it, and a chapter's first or last page stays showing through a change of window size even when it has no text. A page with no text in the middle of a chapter stays showing while the chapter keeps its number of pages. Closing the reader while the next chapter is still loading keeps the last place reached, which can be a page turned just before the chapter was chosen. A different version begins at its own saved position or the first chapter. A position saved by an earlier build, which counted paragraphs, opens near the top of its chapter.
- Chapters in the reading line are weighed by their size in counting the part read, and a book is finished when the end of its last such chapter has been shown.
- Nothing in a book runs or loads from outside it. The page is drawn by a web view that is given only markup the sandboxed helper wrote ([C1 to C8](epub-reader-mvp.md#content-for-a-web-view)), runs no script from the book, refuses every request that is not for the book's own chapter or pictures, and goes nowhere but the chapter the reader asked for. A chapter that takes more than 15 seconds to display, or more memory than the reader allows, is stopped and the reader says so. The next chapter still opens. A chapter the helper cannot read says why at once. Closing the reader ends the process that drew its pages.
- Send to device writes the managed EPUB into an existing folder the reader chooses. Choose a mounted SD-card directory for an Xteink X4 Pro in CrossPoint USB Drive mode. The result shows the written path or reports that a matching copy already exists. The smoke check uses a local temporary destination. Physical-reader behaviour remains untested.

## Large libraries

- The window asks for books in pages of 128 and creates rows only for what is on screen, so scrolling and drawing cost the same in a library of any size. Opening a view still puts every book in order, and a search reads every book's text. At 100,000 books those take about 0.06 and 0.03 seconds, off the window's thread.
- With 100,000 generated books the first titles can be read through the accessibility tree a quarter of a second after the process starts, against a limit of 2 seconds. All six switches between Grid, List and Table keep the focused book visible, or keep the previous top book in the first visible row when focus is outside the books. A search is answered as the reader types, and Ctrl+End shows the last rows. `scale.py` checks these and the import behaviour above.
