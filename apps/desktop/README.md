# Ebook Manager for Linux

Run `ebook-manager` to open your shelf. Choose **Add files** or **Add folder** to import DRM-free EPUBs. Use **Read** to open the reader, where Left and Right turn pages and F11 fills the screen, **Edit** to correct a shelf entry, and **Export** to copy its EPUB to a folder. On an Xteink X4 Pro running the CrossPoint fork, switch to USB Drive mode and choose a folder on the mounted SD card.

The GTK4 application uses GTK 4.12 or later, libadwaita 1.5 or later and WebKitGTK 6.0 (2.44 or later), which draws the book's pages. Install the GTK, libadwaita and WebKitGTK 6.0 development packages and bubblewrap. Imports and reading require a working bubblewrap sandbox. From the repository root, run `make install` to build the optimized desktop binary and helper together and install them in `~/.local/bin`, with the launcher in `~/.local/share/applications`. The helper must be discoverable by the reader crate, normally next to the desktop binary. The installer writes the absolute executable path into the application menu entry.

`main.rs` opens the shelf. A single background worker owns the library in `$XDG_DATA_HOME/ebook-manager` (by default `~/.local/share/ebook-manager`). Imports parse a stable temporary copy through the reader helper before submitting it to that worker. Opening a chapter uses the same helper. [`reader.rs`](src/reader.rs) owns the reader's window and [`reader/view.rs`](src/reader/README.md) owns the web view and everything that keeps book content inside it. Metadata edits and reading positions belong to the library. Export copies managed EPUB bytes to the directory chosen in the file picker.

## Exercise the desktop

From the repository root containing the workspace Cargo.toml, build and run the smoke driver in a private graphical session. It requires Xvfb, Openbox, D-Bus, gdbus, xdotool, ImageMagick, Tesseract with English data, Python with PyGObject, and AT-SPI 2. The driver creates a temporary home directory, generates all its books, drives the window and the reader from the keyboard, restarts the process and compares the exported bytes. It then imports a gallery of generated covers, shows it as a list and a table, sorts by every field and by three keys through the sort editor and the table's headers, hides, moves and resizes columns, changes the cover size, and checks that all of it is as left after a restart. It checks what is drawn in light, dark and More contrast appearance. The driver starts the AT-SPI registry explicitly and asks the private bus for the registry root before every app launch. The bus has no auto-activated services, and the driver clears the desktop's identity variables, so the host's portals and file-system daemons never start inside it. Those would crash when the private bus closes. Its two arguments are the desktop binary and a directory for screenshots. Both drivers check app stderr across every start. The sole exception is GDK's duplicate frame timing warning on X11, also reproduced on `main` by resizing the reader with its chapter drop-down open.

```sh
cargo build --release --workspace --bins
python3 apps/desktop/tests/smoke.py \
  target/release/ebook-manager /tmp/ebook-desktop-smoke
```

The reader's web view has tests of its own, which serve it hostile chapters on a real WebKitGTK. They need a display and a session bus, and [`tests/private-session.sh`](tests/private-session.sh) gives them private ones that start none of the desktop's services:

```sh
apps/desktop/tests/private-session.sh cargo nextest run -p ebook-manager --test view
```

Where `coredumpctl` exists, that test program fails if any core dump was recorded while it ran.

The smoke driver can also end the app one way with a reader open and report what was left: `--ending term`, `int`, `kill`, `abrt` or `display` after the two usual arguments. It is a check made by hand, once per way, when the way the reader ends has changed. It is never run in CI or in a loop, because an ending that crashes something leaves a record the desktop shows its user, and `abrt` always leaves the app's own.

See [desktop behaviour](../../specs/desktop-mvp.md) for the acceptance cases.

```sh
cargo build --release -p ebook-library --features fixture --bin fixture
python3 apps/desktop/tests/scale.py \
  target/release/ebook-manager target/release/fixture
```

The first check fails when the median launch is over 2 seconds plus the time the driver takes to look. A launch here takes about 0.26 seconds: 0.1 for GTK to start, 0.06 for the catalogue to put 100,000 books in order, and the rest to build and draw the first rows.

The screenshots show the synthetic smoke library: the cover grid in [light](tests/evidence/grid-light.png), [dark](tests/evidence/grid-dark.png) and [More contrast](tests/evidence/grid-contrast.png) appearance, the list in [light](tests/evidence/list-light.png) and [dark](tests/evidence/list-dark.png), the table in [light](tests/evidence/table-light.png) and [dark](tests/evidence/table-dark.png), the [sort editor](tests/evidence/sort-editor.png), the [empty library](tests/evidence/empty.png), a [search with no match](tests/evidence/no-match.png), the [folder import report](tests/evidence/import.png), the [chapter reader](tests/evidence/reader.png), the [full-screen reader](tests/evidence/full-screen.png) and the [reading position across restart](tests/evidence/resume.png).
