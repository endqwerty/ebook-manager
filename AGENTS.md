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
- Code or design details you may recall from calibre through your training. See
  [Recall from training](#recall-from-training).

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

### Recall from training

If you think you may know how calibre implements the subject at hand from your
training data, such as its code, function names, internal structure or particular
algorithms, stop working on that subject. Don't write it down, describe it, or put
it in a brief.

Hand the work to an agent from a different model provider (Claude to Codex, Codex
to Claude). Its brief must:

- State the root problem in plain terms: what a user needs and what the inputs and
  outputs are. For example, "read the title, authors and cover from an EPUB file",
  not "port calibre's OPF reader".
- Name only allowed sources: format specifications, sample files and this repo's
  specs and issues.
- Tell it to start from basics and design the solution from the root problem,
  without trying to reproduce calibre's or any other existing implementation.
- Include this file's Rule 1 as described below.

Keep the reason for the handoff short and free of the recalled detail, for example
"possible training recall of calibre's EPUB parsing". The receiving agent follows
this same section. If it also thinks it recalls calibre's approach, it stops and
reports back instead of writing code, and you raise the subject with the user.

### Subagents and delegation

Every subagent, delegate, reviewer and verifier must follow Rule 1. Copy this
section's first paragraph and the link to this file into every task brief you
write, because a child may not load this file. Don't hand a child a calibre
source path, URL or excerpt.

If you see calibre code, say so in your reply, don't use it, and record it in the
pull request so a reviewer can check the affected code.

## Project state

A public open-source project under the Apache License 2.0 ([LICENSE](LICENSE)).
There's no code yet, and the language and UI toolkit haven't been chosen. The aim is
a native desktop app for macOS and Linux, probably Windows too, with native iOS and
Android apps, fast and smooth with large libraries. Rust is under consideration for
the shared core. Everything committed here is public: never commit secrets, tokens,
private addresses or personal data.

## Plans: requirements

[`docs/requirements/`](docs/requirements/README.md) lists what the app has to do,
with an identifier for each requirement, and
[what that asks of the data model](docs/requirements/data-model-inputs.md). Read the
page for an area before designing or building in it. An issue names the requirement
identifiers it covers. When a requirement is built, describe the behaviour in a spec
and mark the requirement done. If research or a decision changes a requirement, edit
its page in the same pull request.

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
