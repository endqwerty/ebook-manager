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

A public open-source project under the Apache License 2.0 ([LICENSE](LICENSE)).
There's no code yet, and the language and UI toolkit haven't been chosen. Everything
committed here is public: never commit secrets, tokens, private addresses or
personal data.

## Backlog: GitHub issues

The backlog lives in [GitHub issues](https://github.com/endqwerty/ebook-manager/issues).
Before starting work, find or open the issue for it, and link it from the pull
request (`Fixes #N`). Write feature requests in plain language about what a user
needs. A request may describe calibre's behaviour as seen from using it, but never
its code.

## History: spec files

[`specs/`](specs/README.md) describes how each area works today. A pull request that
changes behaviour updates its spec in the same pull request. The Git history of
`specs/` is the project's history. Don't add changelogs or decision logs beside it.

## Tests and CI: Mainworthy

Every push to `main` and every pull request runs [check.yml](.github/workflows/check.yml)
on GitHub's hosted runners. It reports results to the production Mainworthy at
<https://mainworthy.com>, project `ebook-manager`. Mainworthy's gate (`mainworthy gate`)
decides whether the commit passes. [change.yml](.github/workflows/change.yml) tells
Mainworthy when a pull request ends.

- Repository variables `MAINWORTHY_URL` and `MAINWORTHY_PROJECT`, and secret
  `MAINWORTHY_TOKEN`. The secret is a pipeline token for the actor
  `ebook-manager-github-actions` (`record` only), expiring 2027-11-17. To renew it
  before then, issue a new one with `mainworthy token new --ci ebook-manager-github-actions
  --project ebook-manager --expires 365d` using an instance admin token, set the
  secret, revoke the old token, and update the date here. The procedure for
  Mainworthy production access is in the homelab repository.
- The CLI comes from the public image `ghcr.io/endqwerty/mainworthy:main`.
- Until the first tests exist, CI reports `--nothing-required`, which the project's
  policy accepts (`policy empty nothing-required`). When the first test suite lands,
  have it write JUnit XML and change the step to `mainworthy report results.xml`.
  Then set the policy back with `mainworthy policy empty pending`.
