# The reader's web view

`view.rs` draws one chapter in a WebKitGTK 6.0 web view and keeps the book's
content inside it. `reader.js` and `reader.css` are the app's own script and
style sheet for the page. `progress.rs` works out how far through the book a
place is. `place.rs` is the reader's place-keeping: one function over an
explicit state that takes what happened and answers what to do, with no
WebKit and no GTK in it. `settings.rs` holds the reader's text settings,
reads and writes their file and turns them into the page's custom
properties, and `text_panel.rs` is the popover that changes them. The
window and the keys are in
[`../reader.rs`](../reader.rs), which tells that function every event and
does what it answers. The design is
[Reading an EPUB in the app](../../../../docs/design/epub-reading.md), and what
a reader sees is in [the desktop spec](../../../../specs/desktop-mvp.md#read-and-export).

## Entry point

```rust
view::View::open(
    Config::new(filter_dir),        // where the compiled content filter is kept
    source,                         // Rc<dyn Source>: chapters and pictures
    |event| { /* Ready, Place, Jump, External, Tap, Failed */ },
    |view: Result<View, String>| { /* put view.widget() in a window */ },
);
view.load(chapter, Landing::Start);       // or Landing::Offset(n), Landing::Anchor(..), Landing::End
view.turn(1, true, |moved| { /* false at the chapter's edge; true animates as `--turn` says */ });
view.set_style(&[("--page", "#2b2b2e")]);   // now, and for chapters opened later
view.shut();                               // before the window that holds it goes
```

A `Source` gives the view a `Document` (the children of an XHTML `body`) and
`Picture`s by number. The reader's source is the sandboxed helper's
`read_content` and `read_resource`. `view.rs` uses nothing else from the app,
so [the view tests](../../tests/view.rs) include it directly.

## What callers can rely on

- **No script from the document runs.** `enable-javascript-markup` is off and
  the chapter's content security policy allows no script. The app's script
  runs in its own script world, which the page cannot reach.
- **No request leaves the machine.** Three layers each block alone: the
  content security policy, a content filter that blocks every address not
  starting with `ebook:`, and a proxy setting that points at a closed local
  port. The web process itself has no network: WebKit runs it in its own
  network namespace.
- **The view stays on the chapter.** Only the load `View::load` just asked for
  may navigate. Links, redirects, new windows, downloads and permission
  requests are refused.
- **The `ebook:` scheme serves one book to one view.** An address carries a
  random token made for that view, and the scheme answers it only to that
  view: another view asking with the same token gets a 404, and so does
  anyone once the view is gone. The rest is `/c/<number>` or `/r/<number>`,
  and anything else is a 404. No part of an address becomes a file path. The
  scheme is also registered as no-access, which WebKit documents as keeping
  its pages from reaching pages of other schemes. That is an extra, and not
  what blocks requests: the three layers above do that.
- **Events are checked, not trusted.** A message from the page does something
  only if it has a known kind and values in range. A place must name a load
  that became ready, and hold a page inside the page count and an offset
  inside the length of the chapter that load showed; it is then told with
  that load's record, whichever load is showing. A jump, an outside link and
  a tap act on what is showing, so they must name the load asked for last.
  `Event::External` carries an `http`, `https` or `mailto` address and has
  opened nothing. Ask the reader first.
- **A place survives reflow.** `Event::Place` gives the UTF-16 offset of the
  page's first character. `Landing::Offset` shows the page holding an offset
  at any window size or type size. A page with no text on it, such as one
  full-page picture, reports the offset of the first text after it, or the
  chapter's length when no text follows, which `Landing::Offset` shows as the
  last page. An offset cannot tell text-less pages apart, so one in the
  middle of a chapter, or any but the last of several at its end, comes back
  as the next page with text, or the last page. While the view stays open, a
  text-less first or last page is held as that end and survives a reflow,
  and one in the middle keeps its number while the page count is unchanged.
  No `Event::Place` is sent before the landing `load` asked for.
- **A move and its place arrive together.** A turn that moved has sent
  `Event::Place` before its `done` is called, because the page's answer to
  the turn is the place. A landing and a settled scroll send it as their own
  message. A turn's place is where it leads, also when the page animates
  it: the page reads the place at the destination before the slide or fade
  starts, and reports nothing more when it ends. Turns animate in pages
  only: in continuous scrolling a turn moves at once, and a scroll the page
  made itself is not reported again when it settles. When the system
  asks for less motion the reader sets `--motion: none` and turns move at
  once. Each has `moved: true`, the `Load` record of the load it was made
  in (its number, its chapter and that chapter's length) and a `count`, the
  page's count of its moves in that load. `View::load` returns the number,
  which grows with every load, so the load's number and then the count put
  every move in the order it was made. The record is the place's own: the
  view keeps the record of every load that became ready, so a place arrives
  with the right chapter however many chapters have been asked for since. A
  caller must not assume a place is for the load showing. A reflow sends
  `Event::Place` with `moved: false`, new page numbers, and the offset and
  count of the last move.
- **`place.rs` turns those events into what to do.** `Keeper::step` takes
  an `Input` (a load asked for, a load ready, a report, a chapter failing,
  the save timer firing, leaving) and answers `Effect`s: show a place, blank the status,
  start or stop the save timer, submit a place to the library, wait for the
  library, end the page. It holds the place of the most recently made move
  that has arrived, shows a place only for the load being shown, submits
  saves in the order their places were made, and after a leave answers
  nothing. `reader.rs` performs the effects in the order given, and saves
  through one function, `submit`, which has the save in the library's queue
  before it returns.
  `Landing::Start` is the first page whatever is on it.
- **A chapter cannot run for ever.** A load that takes longer than
  `Config::watchdog` (15 seconds) ends the web process. So does each call
  into the script, on a limit of its own, and so does holding more than
  `Config::memory_mib` (1,024) while the process still answers.
  `Event::Failed` says which, and the next `load` starts a new process. A
  chapter the `Source` cannot give is `Event::Failed` at once, with the
  source's own words.

## Side effects

- One compiled content filter under the folder given to `Config::new`, written
  once per process.
- One WebKit web process per view, and WebKit's shared network process.
  Nothing is cached or stored: the network session is ephemeral.

## Three things that must stay as they are

WebKitGTK 2.54 aborts a web process, leaving a core dump, in three cases the
tests found. All are avoided in `view.rs`. The first two are guarded by
`two_views_share_one_process_without_a_crash`, and all three by the test
program failing on any core dump.

- A web process shutting itself down when its view is destroyed. Now and
  then, under load, it aborts in a timer's destructor. `View::shut`, and the
  view's own drop, end the process outright first. WebKit ends a running
  process with a kill signal, which leaves no core dump, and cancels one
  that is still starting. Call `shut` before destroying the window that
  holds the view, and when any window that owns that one closes: the reader
  does both.

- A second `WebKitWebContext` with its own memory limit. Every view shares
  one context, so the memory limit is the first view's.
- Saving the compiled content filter again while a view uses it. The filter
  is compiled once per process.

## Checking it

```sh
apps/desktop/tests/private-session.sh cargo nextest run -p ebook-manager --test view
cargo test -p ebook-manager --bin ebook-manager reader::
```

The second line needs no display. It runs `place.rs` through 20,000 seeded
random sequences of loads, moves, reflows, timer firings and leaves, with
answers from any earlier load arriving late and in any order, and compares
each step with a reference that remembers everything. A second test puts
back, one at a time, four faults that reviews found in the code `place.rs`
replaced, and requires the random test to catch each. A failure prints its
seed. Two view tests, `an_answer_two_chapters_late_is_held_and_not_shown`
and `a_chapter_loaded_again_shows_only_its_new_load`, feed the same
function from a real page.

The same line runs `settings.rs`'s tests. Name the `--bin`: without it a
filter can also match a view test and run it on your own desktop.

The view tests also build the text settings panel alone, from
`settings.rs` and `text_panel.rs`, and check that its controls show the
settings, that showing them never changes the level, that Reset refreshes
every control, that changing one control changes only its setting, and
that the panel is freed with its window. Its controls' handlers hold the
panel's widgets weakly for that reason. Every change goes through one
function that then shows the settings in every control, and while it shows
them a counter that only the outermost showing can bring back to zero
keeps any control from writing a setting.

The view tests serve hostile chapters to a real web view and count
connections to a local listener. Each is named for the statement of the
design it checks. Where `coredumpctl` exists, the test program fails if a
core dump was recorded while it ran, whatever the tests asserted.
