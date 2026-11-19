# Ebook Manager for Linux

Run `ebook-manager` to open your shelf. Choose **Add files** or **Add folder** to import DRM-free EPUBs. Use **Read** to open the chapter reader, **Edit** to correct a shelf entry, and **Export** to copy its EPUB to a folder. On an Xteink X4 Pro running the CrossPoint fork, switch to USB Drive mode and choose a folder on the mounted SD card.

The GTK4 application uses GTK 4.12 or later and libadwaita 1.5 or later. Install the GTK and libadwaita development packages and bubblewrap. Imports and reading require a working bubblewrap sandbox. From the repository root, run `make install` to build the optimized desktop binary and helper together and install them in `~/.local/bin`, with the launcher in `~/.local/share/applications`. Put `~/.local/bin` on PATH. The helper must be discoverable by the reader crate, normally next to the desktop binary. The launcher in `data/` invokes `ebook-manager` on PATH.

`main.rs` opens the shelf. A single background worker owns the library in `$XDG_DATA_HOME/ebook-manager` (by default `~/.local/share/ebook-manager`). Imports parse a stable temporary copy through the reader helper before submitting it to that worker. Opening a chapter uses the same helper. Metadata edits and reading positions belong to the library. Export copies managed EPUB bytes to the directory chosen in the file picker.

## Exercise the desktop

From the repository root containing the workspace Cargo.toml, build and run the smoke driver in a private graphical session. It requires Xvfb, D-Bus, xdotool, ImageMagick, Tesseract with English data, Python with PyGObject, and AT-SPI 2. The driver creates a temporary home directory, generates all its books, exercises the GTK controls, restarts the process and compares the exported bytes. Its two arguments are the desktop binary and a directory for screenshots, including the scrolled and restored pages.

```sh
cargo build --release --workspace --bins
python3 apps/desktop/tests/smoke.py \
  target/release/ebook-manager /tmp/ebook-desktop-smoke
```

See [desktop behaviour](../../specs/desktop-mvp.md) for the acceptance cases.

The [empty and populated shelf](tests/evidence/shelf.png), [folder import report](tests/evidence/import.png), [chapter reader](tests/evidence/reader.png) and [reading position across restart](tests/evidence/resume.png) show the synthetic smoke library.
