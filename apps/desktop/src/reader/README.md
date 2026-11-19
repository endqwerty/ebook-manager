# The reader's web view

`view.rs` draws one chapter in a WebKitGTK 6.0 web view and keeps the book's
content inside it. `reader.js` and `reader.css` are the app's own script and
style sheet for the page. `progress.rs` works out how far through the book a
place is. The window, the keys and saving the place are in
[`../reader.rs`](../reader.rs). The design is
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
view.turn(1, false, |moved| { /* false at the chapter's edge */ });
view.set_style(&[("--page", "#2b2b2e")]);   // now, and for chapters opened later
view.current(|place| { /* Some((offset, page, pages)) after any turn already asked for */ });
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
  only if it has a known kind, the current load's number and values in range:
  a page inside the page count and an offset inside the chapter's length.
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

## Two things that must stay as they are

WebKitGTK 2.54 aborts a web process, leaving a core dump, in two cases the
tests found. Both are avoided in `view.rs` and guarded by
`two_views_share_one_process_without_a_crash`.

- A second `WebKitWebContext` with its own memory limit. Every view shares
  one context, so the memory limit is the first view's.
- Saving the compiled content filter again while a view uses it. The filter
  is compiled once per process.

## Checking it

```sh
apps/desktop/tests/private-session.sh cargo nextest run -p ebook-manager --test view
cargo test -p ebook-manager progress
```

The view tests serve hostile chapters to a real web view and count
connections to a local listener. Each is named for the statement of the
design it checks. Where `coredumpctl` exists, the test program fails if a
core dump was recorded while it ran, whatever the tests asserted.
