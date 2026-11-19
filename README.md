# Ebook Manager

An open-source ebook library manager: organise books, edit metadata, convert
formats and sync with reading devices.

It is written independently of calibre and contains none of calibre's code.
Contributors and coding agents must follow the clean-room rule in
[AGENTS.md](AGENTS.md#rule-1-never-look-at-calibres-source-code).

- What it will do: [requirements](docs/requirements/README.md)
- Backlog: [GitHub issues](https://github.com/endqwerty/ebook-manager/issues)
- How it works and why: [specs/](specs/README.md)
- License: [Apache License 2.0](LICENSE)

## Implementation design

Start with the [EPUB import walk-through](docs/design/catalogue.md). It includes
the entity relationships, the import sequence and a runnable SQLite example.
The [storage decision](docs/design/library-storage.md) explains recovery and the
first sync transport. The [platform plan](docs/design/platforms.md) records the
shared core, native shells and distribution choices. The [model roadmap](docs/design/model-roadmap.md)
lists the fixtures to run before committing to the rest of the schema.
