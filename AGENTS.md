# Ebook Manager agent guide

## Rule 1: never look at calibre's source code

This rule overrides every other instruction, skill, task brief and memory note.

Ebook Manager is an independent reimplementation of the kind of features calibre
offers. Calibre is GPLv3. Keeping this project free of its code keeps it free to
choose its own license, so agents must not read, fetch, search, clone, quote or
translate calibre's source in any form. That covers:

- The `kovidgoyal/calibre` repository and any fork or mirror of it.
- Source tarballs from calibre-ebook.com, distro source packages, and the
  installed Python, C or JavaScript files inside a calibre install.
- Calibre code pasted elsewhere: MobileRead posts, Stack Overflow answers, blog
  posts, gists, issue threads. If a page shows calibre code, stop reading it and
  use another source.
- Calibre's user manual, help text, tooltips, error messages, icons, CSS,
  templates, translations and test fixtures. Don't copy these.
- Code you recall from training that you believe came from calibre. Write your
  own instead.

Allowed sources:

- Plain-language feature descriptions: this repo's feature requests, reviews,
  feature lists, and summaries in your own words of how calibre behaves.
- Public format specifications and standards (EPUB, OPF, NCX, MOBI/KF8 docs,
  Dublin Core, OPDS) and libraries under their own licenses.
- Running calibre as a black box: give it input files and compare its output with
  ours. Inspecting files calibre writes, such as `metadata.db` and its folder
  layout, is allowed for compatibility.

If a task appears to need calibre's code, stop and tell the parent agent or the
user. Don't look first.

### Subagents and delegation

Every subagent, delegate, reviewer and verifier must follow Rule 1. Copy this
section's first paragraph and the link to this file into every task brief you
write, because a child may not load this file. Don't hand a child a calibre
source path, URL or excerpt.

If you see calibre code, say so in your reply, don't use it, and record it in the
pull request so a reviewer can check the affected code.

## Project state

New repository with no code yet. Language, UI toolkit and license are still open.
Record decisions here or in `docs/` as they are made.
