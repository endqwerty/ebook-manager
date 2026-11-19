# Ebook Manager

Ebook Manager is an offline Linux desktop library for DRM-free EPUBs. Import
your books, find them by title or author, read a chapter, and send a copy to a
mounted e-reader. Conversion, wireless sync and mobile apps are in the backlog.

## Run on Linux

Install a recent stable Rust toolchain, a C compiler, pkg-config, GTK 4.12 or
later, libadwaita 1.5 or later development files, Bubblewrap, Python 3 and Make. On Arch the system
packages are `base-devel gtk4 libadwaita bubblewrap python`. CI on Ubuntu 26.04 uses
`build-essential pkg-config libgtk-4-dev libadwaita-1-dev bubblewrap python3`.

From the repository root:

```sh
make install
~/.local/bin/ebook-manager
```

The installer puts both the desktop app and its EPUB helper in `~/.local/bin`.
The **Ebook Manager** menu entry opens the installed executable directly. For a
development run, build all binaries together with
`cargo build --workspace --bins --locked`, then run `target/debug/ebook-manager`.

Choose **Add files** or **Add folder** to import. The app keeps managed copies
under the XDG data directory (`~/.local/share/ebook-manager` by default).
If an import needs a decision, review the proposed match before replacing an
existing book. Edits change your library metadata. Original EPUBs stay intact.

For the Xteink X4 Pro with CrossPoint, enable USB Drive mode and mount the SD
card, or use a card reader. Export a book to a folder on the card and eject it
through your desktop before unplugging. CrossPoint can display the EPUB's
embedded title even when its exported filename uses your edited library title.
[Mounted EPUB export](specs/mounted-epub-export.md)

Read the [desktop guide](apps/desktop/README.md) for the controls and the
[library guide](crates/ebook-library/README.md) to back up and restore your
books, edits and reading positions. The [EPUB reader guide](crates/ebook-reader/README.md)
describes format support and the Linux sandbox.

## Back up and restore

Quit Ebook Manager, then copy the entire `portable` folder from
`~/.local/share/ebook-manager/` to your backup. If you set `XDG_DATA_HOME`,
the library is under `ebook-manager/` in that directory.

To restore, quit the app. Move any existing `ebook-manager` data directory
aside so you keep the old copy. Create a fresh directory of the same name
and copy the backup's `portable` folder into it. Check that the books are at
`ebook-manager/portable/books/`. Launch Ebook Manager to rebuild the shelf,
then open a book to check your edits and reading position.

## Project

It is written independently of calibre and contains none of calibre's code.
Contributors and coding agents must follow the clean-room rule in
[AGENTS.md](AGENTS.md#rule-1-never-look-at-calibres-source-code).

- What it will do: [requirements](docs/requirements/README.md)
- Backlog: [GitHub issues](https://github.com/endqwerty/ebook-manager/issues)
- How it works and why: [specs/](specs/README.md)
- License: [Apache License 2.0](LICENSE)

## Implementation design

The [desktop MVP](docs/design/desktop-mvp.md) follows a reader from folder import to reading on an X4 Pro.
Start with the [EPUB import walk-through](docs/design/catalogue.md). It includes
the entity relationships, the import sequence and a runnable SQLite example.
The [storage decision](docs/design/library-storage.md) explains recovery and the
first sync transport. The [platform plan](docs/design/platforms.md) records the
shared core, native shells and distribution choices. The [model roadmap](docs/design/model-roadmap.md)
lists the fixtures to run before committing to the rest of the schema.
