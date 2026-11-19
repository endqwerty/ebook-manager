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
The first implementation targets a local Linux desktop library for DRM-free EPUBs,
with a shared Rust core and a mounted-folder export for e-readers. The [design documents](README.md#implementation-design)
record the selected stack, storage decisions and executable catalogue experiment.
Everything committed here is public: never commit secrets, tokens,
private addresses or personal data.

## Plans: requirements

[`docs/requirements/`](docs/requirements/README.md) lists what the app has to do,
with an identifier for each requirement, and
[what that asks of the data model](docs/requirements/data-model-inputs.md). Read the
page for an area before designing or building in it. An issue names the requirement
identifiers it covers. When a requirement is built, describe the behaviour in a spec
and mark the requirement done. If research or a decision changes a requirement, edit
its page in the same pull request.

## Running design and component reviews in T3

The agent owning a feature maintains its documents as part of doing the work.
Before implementing a change that affects an interface, ownership or recovery,
write or update the relevant design in `docs/design/`. Link the requirement,
the decision and a concrete case that could show the decision is wrong. When
a component exists, keep its README beside the code. Explain its entry point,
what callers can rely on, its side effects and how to check them. Link it from
the nearest parent README. When behaviour changes, update that contract and the
current-behaviour spec in the same PR. An existing README that already answers
these questions is sufficient. When an interface changes, find the designs and
callers that depend on it, tell their owners which assumption changed, and check
that assumption on the new revision.

### Start a fresh reader automatically

When a design is ready to implement, its plan materially changes, or a component
README gains or changes a contract, the owning agent starts a goldfish review.
Run the relevant example or behaviour check first. In T3 use a new
`delegate_task` with a standalone, read-only brief. Give it the revision, item
path, a realistic reader or implementer task, and the Rule 1 instructions above.
Let it follow the document's links. Keep the author's conversation, preferred
answer and earlier verdicts out of the brief. Ask what it would do, what evidence
supports that answer and where it had to guess. Fix consequential gaps in the
repo. A passing fresh verifier on the same revision can cover this read when
its brief and evidence meet both jobs. Wording edits alone can finish locally.

Choose a reviewer from the live catalogue and check its allowance. GPT-6.2 Sol
and Opus 5.5 are the usual choices for design and behaviour. GPT-6.1 Terra and
Sonnet 5.5 can take bounded reading or navigation tasks. For an occasional
defensive security review, use Daybreak when the change touches a trust boundary
such as book parsing, device pairing or access to personal data. Give it the
specific boundary and the protection the implementation should provide. When a
design trade-off needs another decision-maker, consult Fable with the options,
constraints and evidence. A Fable lead can ask Astra for an independent view.
The item owner records the choice in the design. For a choice that binds two
milestones, the coordinator decides within the agreed scope, records it on the
shared issue, links it from the affected designs and tells both leads.

### Count reviews per work item

Count cold reads and targeted rechecks as rounds on the item they examine.
Several reviewers in one pass count once. The parent coordinates dispatch
when a delegated worker cannot launch reviewers. Carry the count through
revisions, renames, worker changes and handoffs.

Use the linked issue for handoff. Keep a short entry per item with its name,
revision, round, reviewer, finding and next action. Before dispatch, mark the
round in flight and give the T3 task a unique public-safe title containing the
issue number, item and round. Put that exact title on the issue. On takeover,
find it with `t3_thread_list` (include subagents, filter by title) and read
the matching thread. Its `parentThreadId` identifies the dispatching session.
If that session is unavailable, keep a live reader in flight. If the reader
terminated without a result, record the failed attempt before arranging a new pass.
If the API never accepted a dispatch, record that and recover the same round.
Record the result, deciding check or failed attempt. A pass against a superseded
interface still counts. Use its findings only where their evidence still holds,
and target any recheck at the changed dependency.

Before rounds 4, 7, 10 and so on, pause that item. Record what another pass
could change for a reader or implementer and how to check it. Continue if that
check would guide a product decision. If only wording remains, finish the edit.
Other items continue on their own counts.

### Delegate a major milestone

When a milestone has its own outcome, boundaries and acceptance check, the
coordinator can assign an Astra or Fable lead to own it through verification.
Use `delegate_task` for a child of the current session. When the user authorizes
separate T3 sessions, launch independent milestones with `t3_thread_launch`
and a worktree workspace strategy so each has its own checkout. Supply the
chosen base and return address in the brief. Check model availability and
allowance first.

Give each lead its issue, base revision, owned files, dependencies, completion
check, decision authority and where to report back. Include Rule 1 in every
brief. The lead can organise its own workers and fresh readers. Keep publication
and integration with the coordinator. Before relying on a handoff, check the
lead's revision and reproduce the deciding result. The coordinator owns the
PR, its independent verification and CI. Merge when the user authorizes it.

Agree on check-ins in the brief. Report the first working result, decisions
that affect another milestone, blockers and the verified handoff. For longer
gaps, set a check-in interval suited to the work. Send the coordinator a short
T3 update with the revision, evidence and next step, and leave the durable
state on the issue so another lead can resume.

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

## Look and feel: interface design

[`docs/design/interface.md`](docs/design/interface.md) says how the app should
look: what the platform's standard parts own, and how the app draws covers,
type, shadows and motion. Its three companion pages cover
[the ways to show books](docs/design/interface-views.md),
[the other screens](docs/design/interface-screens.md) and
[the reader](docs/design/interface-reader.md). Read the base page and the page
for your screen before designing or building it in any shell. If a screen needs to break one of its rules, change the page in the same
pull request and say why.

## Crashes: users never see one

A crash is one of the worst things a user can experience, so no part of the app
may crash: not the window, not a helper process, not a converter. A crash here
means a process that ends by a signal that dumps core, such as an abort, a
segfault, a stack overflow or a CPU or file-size limit. The system records it
and the desktop shows a crash notice. A process stopped deliberately with
`SIGKILL` or `SIGTERM` leaves no record. Requirement `SEC-10` covers files that
break a parser.

- Expected failures, such as a limit being hit or a broken file, end with an
  error the caller can show, never a signal. A process that runs untrusted input
  must also turn a fatal signal into an ordinary exit. The reader helper does
  both: [helper.rs](crates/ebook-reader/src/bin/helper.rs) installs handlers
  as its first step and from then on exits with status 4 on any such signal.
  A test raises each one.
- Agents run builds and tests on the desktop Daniel uses, so a test that crashes
  a process sends him one crash notice per run, from every agent that runs it.
  When a change adds or alters a failure test, count `coredumpctl list
  --no-pager -q` lines before and after one run. A new record is a bug in the
  code or the test. Fix it before pushing.
- Run the GUI under a private X server and a private D-Bus session that starts
  no services, as [smoke.py](apps/desktop/tests/smoke.py) does. Clear
  `XDG_CURRENT_DESKTOP` and `HYPRLAND_INSTANCE_SIGNATURE` as well. A private bus
  that may start services launches the host's portals, which crash when it
  closes.
- An exploratory probe that may crash, such as a native call from a script or a
  headless browser, can run once when the crash is what you're studying. Don't
  put it in a loop or a test, and mention the crash in your reply.

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
- CI runs the Rust workspace with the nextest `ci` profile and reports
  `target/nextest/ci/junit.xml` along with the desktop and mounted FAT32
  acceptance results. When this workflow reaches the default branch,
  set the project policy to `mainworthy policy empty pending` to retire the
  pre-implementation allowance for empty reports.
