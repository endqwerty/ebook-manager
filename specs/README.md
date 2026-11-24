# Specs

Each file here describes one area of Ebook Manager as it works today: what it does,
the rules it follows and why. Specs are the project's history. The Git log of
`specs/` shows how the behaviour changed, and each commit message says why.

## Implemented behaviour

- [Desktop shelf, import and reading](desktop-mvp.md)
- [macOS desktop library and reader](macos-desktop.md)
- [Library storage and recovery](library-mvp.md)
- [EPUB reader and helper isolation](epub-reader-mvp.md)
- [Mounted reader export](mounted-epub-export.md)

## Writing a spec

- A pull request that changes behaviour updates the spec for that area, or adds one,
  in the same pull request. Code and spec must agree on `main`.
- Write specs in your own words, from user needs, our own observations of
  how calibre behaves and public format standards. Never from calibre's source,
  manual or help text ([Rule 1](../AGENTS.md#rule-1-never-look-at-calibres-source-code)).
- Describe current behaviour. Don't keep a changelog inside the file; Git
  does that. When an area is removed, delete its spec.
- Link the GitHub issue that asked for a change in the commit or pull request,
  not in the spec.
- Name files by area in kebab case, for example `library-import.md` or
  `epub-to-mobi.md`.

A spec starts with a short summary, then lists the behaviour as testable
statements. Tests should name the spec statement they check.
