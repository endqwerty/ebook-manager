# Ebook Manager for Linux

Run `ebook-manager` to open your library. Choose **Add** to import DRM-free EPUBs from files or a folder. Type to search. Select a book and press Return to read it, F2 to edit how it is named and filed, or Ctrl+D to copy its EPUB to a folder. In the reader, Left and Right turn pages and F11 fills the screen. On an Xteink X4 Pro running the CrossPoint fork, switch to USB Drive mode and choose a folder on the mounted SD card. [Desktop behaviour](../../specs/desktop-mvp.md) lists every command and key.

The GTK4 application uses GTK 4.22 or later and libadwaita 1.9 or later. Install the GTK and libadwaita development packages and bubblewrap. Imports and reading require a working bubblewrap sandbox. From the repository root, run `make install` to build the optimized desktop binary and helper together and install them in `~/.local/bin`, with the launcher in `~/.local/share/applications`. The helper must be discoverable by the reader crate, normally next to the desktop binary. The installer writes the absolute executable path into the application menu entry.

## How it is put together

The app keeps its library in `$XDG_DATA_HOME/ebook-manager` (by default `~/.local/share/ebook-manager`). Besides the GTK thread there are four: the write thread, the read thread, the thumbnail loader and the survey thread. The window waits on none of them.

| File | What it owns | What callers can rely on |
|---|---|---|
| `main.rs` | The application and the helpers the windows share. | |
| `window.rs` | The library window: header bar, search, screen title, actions and keys. | Every command is a `GAction` on the window. A command on a book takes the book's ID. |
| `store.rs` | The write thread, the one owner of the `Library`. | `run` and `call` queue a job and return its result. After every job the window is told, and asks the catalogue whether anything changed. |
| `browse.rs` | The read thread, which owns a `BrowseReader`, and `BookListModel`, the list model, selection model and section model every view shares. | `item(rank)` returns at once and its book arrives with its page of 128. A new search or catalogue change replaces the view with one removal and one addition. The selection is a set of book IDs, so a new view needs nothing found to show it, and a place selected before its row loads is held by rank until it does. `browse/core.rs` is the model's state as one enum and its handling of every event as one function with no GTK in it. The GObject performs the effects that function returns. `browse/worker.rs` is the read thread, which answers every request with what was asked or a refusal, and says `Unchanged` only of the view the model names as shown. A view on screen that can no longer be read is opened again, a place whose row will not come reports that it is gone, a failed open shows the error and no older books, and Try Again starts a new read thread if the last stopped. When the catalogue file is replaced the read thread opens a new reader. |
| `cover.rs` | The `Cover` widget. | It draws the cover table in [the interface design](../../docs/design/interface.md#covers) from a row of the model, and asks `thumbs.rs` for the image. |
| `thumbs.rs` | The thumbnail cache in `cache/thumbs`, its loader thread and the survey thread. | A file is shown only if it is the kind of PNG the app writes, with a header, image data and an end and no other chunk, and its pixels hash to the digest it is named by, and what is shown is made from those pixels. A missing or wrong one is made again from any book that has that cover, and only a thumbnail whose digest matches the record is installed. |
| `views/` | `grid.rs`, `list.rs` and `table.rs`, and in `mod.rs` the cell every view's rows are built on. | One view is on screen at a time, built over the same model. The table never sorts rows: its headers report which was activated and the window changes the model's sort. |
| `window/controls.rs` | The view switch, the Sort menu, the sort editor, cover sizes and the table's column actions. | Each control is an action whose state is the remembered choice. |
| `settings.rs` | The remembered choices in `settings.json` under the config folder. | A missing or damaged file gives defaults. Each view's sort falls back independently and each unknown column is skipped. Saving changed choices can discard unknown top-level fields. Closing with unchanged choices leaves the file intact. |
| `edit.rs`, `imports.rs`, `reader.rs` | The Edit window, the import report and the reader. | |

The serif roles use the system's serif family. No serif is bundled yet.

## Exercise the desktop

From the repository root containing the workspace Cargo.toml, build and run the smoke driver in a private graphical session. It requires Xvfb, Openbox, D-Bus, gdbus, xdotool, ImageMagick, Tesseract with English data, Python with PyGObject, and AT-SPI 2. The driver creates a temporary home directory, generates all its books, drives the window and the reader from the keyboard, restarts the process and compares the exported bytes. It then imports a gallery of generated covers, shows it as a list and a table, sorts by every field and by three keys through the sort editor and the table's headers, hides, moves and resizes columns, changes the cover size, and checks that all of it is as left after a restart. It checks what is drawn in light, dark and More contrast appearance. The driver starts the AT-SPI registry explicitly and asks the private bus for the registry root before every app launch. The bus has no auto-activated services, and the driver clears the desktop's identity variables, so the host's portals and file-system daemons never start inside it. Those would crash when the private bus closes. Its two arguments are the desktop binary and a directory for screenshots. Both drivers check app stderr across every start. The sole exception is GDK's duplicate frame timing warning on X11, also reproduced on `main` by resizing the reader with its chapter drop-down open.

```sh
cargo build --release --workspace --bins
python3 apps/desktop/tests/smoke.py \
  target/release/ebook-manager /tmp/ebook-desktop-smoke
```

The scale check starts the app against 100,000 generated books. It measures the time to the first readable titles over five starts, checks scroll position and visible keyboard focus when switching between views, searches, jumps to the last row, and imports 400 generated EPUBs while checking that the list refreshes at most once a second and keeps its place.

```sh
cargo build --release -p ebook-library --features fixture --bin fixture
python3 apps/desktop/tests/scale.py \
  target/release/ebook-manager target/release/fixture
```

The first check fails when the median launch is over 2 seconds plus the time the driver takes to look. A launch here takes about 0.26 seconds: 0.1 for GTK to start, 0.06 for the catalogue to put 100,000 books in order, and the rest to build and draw the first rows.

The screenshots show the synthetic smoke library: the cover grid in [light](tests/evidence/grid-light.png), [dark](tests/evidence/grid-dark.png) and [More contrast](tests/evidence/grid-contrast.png) appearance, the list in [light](tests/evidence/list-light.png) and [dark](tests/evidence/list-dark.png), the table in [light](tests/evidence/table-light.png) and [dark](tests/evidence/table-dark.png), the [sort editor](tests/evidence/sort-editor.png), the [empty library](tests/evidence/empty.png), a [search with no match](tests/evidence/no-match.png), the [folder import report](tests/evidence/import.png), the [chapter reader](tests/evidence/reader.png), the [full-screen reader](tests/evidence/full-screen.png) and the [reading position across restart](tests/evidence/resume.png).
