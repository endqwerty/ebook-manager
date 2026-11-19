# Ebook Manager

Ebook Manager is an offline Linux desktop library for DRM-free EPUBs. Import
your books, find them by title or author, read a chapter, and send a copy to a
mounted e-reader. Conversion, wireless sync and mobile apps are in the backlog.

## Run on Linux

Install a recent stable Rust toolchain, a C compiler, pkg-config, GTK 4.12 or
later, libadwaita 1.5 or later development files, Bubblewrap and Make. On Arch the system
packages are `base-devel gtk4 libadwaita bubblewrap`. Ubuntu 24.04 uses
`build-essential pkg-config libgtk-4-dev libadwaita-1-dev bubblewrap`.

From the repository root:

```sh
make install
~/.local/bin/ebook-manager
```

The installer puts both the desktop app and its EPUB helper in `~/.local/bin`.
The application menu entry uses `ebook-manager` on the session's PATH. For a
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

The [library guide](crates/ebook-library/README.md) describes how to back up your books, edits and reading positions and restore them on a fresh installation.

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
