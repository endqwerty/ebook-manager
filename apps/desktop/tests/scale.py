"""Starts the desktop against a generated library of 100,000 books and
checks that it stays quick. See apps/desktop/README.md."""

import argparse
import json
import sqlite3
import statistics
import subprocess
import sys
import time
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

sys.path.insert(0, str(Path(__file__).parent))
import smoke
from smoke import Atspi, GLib, environment, find, focus, key, stop, walk

BOOKS = 100_000
# Enough books that the import outlasts the driver's own looks on a fast
# machine: the check needs four gaps between refreshes it saw.
IMPORTS = 800
STARTS = 5
# LIB-70: the library is shown within this long of starting the process.
LAUNCH_LIMIT = 2.0
# The window promises to reopen its view for catalogue changes at most once
# a second. The driver's own polling blurs each reading by a few hundredths.
REFRESH_INTERVAL = 1.0
POLL = 0.02


def application():
    desktop = Atspi.get_desktop(0)
    for index in range(desktop.get_child_count()):
        child = desktop.get_child_at_index(index)
        if child and child.get_name() == "ebook-manager":
            return child
    return None


FIRST = 5


def first_titles():
    """The titles of the first books on screen, in order. Empty while fewer
    than `FIRST` are shown: a title in the accessibility tree that is not
    showing has not been drawn."""
    try:
        app = application()
        if app is None:
            return []
        titles = []
        for node in walk(app):
            if (
                node.get_role_name() == "label"
                and (node.get_name() or "").startswith("Book ")
                and node.get_state_set().contains(Atspi.StateType.SHOWING)
            ):
                titles.append(node.get_name())
                if len(titles) == FIRST:
                    return titles
    except GLib.Error:
        pass
    return []


def launch(binary, env):
    """Starts the app and returns it with two times from the start of the
    process: when the last look that found too few titles began, and when
    the look that read the first titles on screen ended. The titles were
    drawn between the two."""
    started = time.monotonic()
    proc = smoke.launch_app(binary, env)
    try:
        while True:
            looked = time.monotonic()
            titles = first_titles()
            if titles:
                return proc, looked - started, time.monotonic() - started, titles
            assert looked - started < 30, "no titles after 30 seconds"
            time.sleep(POLL)
    except BaseException:
        if proc.poll() is None:
            stop(proc)
        raise


def cells():
    """The book cells on screen: each one's spoken name and whether it is selected."""
    found = []
    for node in walk(application()):
        states = node.get_state_set()
        if node.get_role_name() == "table cell" and states.contains(
            Atspi.StateType.SHOWING
        ):
            node.clear_cache()
            found.append(
                (node.get_name() or "", states.contains(Atspi.StateType.SELECTED))
            )
    return found


def settled():
    """The cells on screen, once two looks in a row agree."""
    last = None
    for _ in range(50):
        try:
            now = cells()
        except GLib.Error:
            now = None
        if now and now == last:
            return now
        last = now
        time.sleep(0.1)
    raise AssertionError(f"the list did not settle: {last}")


def expect_view(number):
    # All three GTK widgets have distinct accessible roles.
    roles = {1: ("grid", "table"), 2: ("list",), 3: ("tree table", "tree grid")}
    deadline = time.monotonic() + 10
    seen = []
    while time.monotonic() < deadline:
        try:
            for node in walk(application()):
                if node.get_name() == "Books":
                    role = node.get_role_name()
                    if role in roles[number]:
                        if number != 3:
                            return node
                        return next(child for child in walk(node) if child.get_role_name() == "list")
                    seen = [role]
        except GLib.Error:
            pass
        time.sleep(POLL)
    raise AssertionError(f"view {number} did not appear: {seen}")


def visible_books(body, number, *, read_focus=False):
    # SHOWING alone includes GTK's overscan. Compare the item bounds with
    # the viewport and read the whole row even if its title is clipped.
    frame = body.get_extents(Atspi.CoordType.WINDOW)
    role = {1: "table cell", 2: "list item", 3: "table row"}[number]
    rows = []
    try:
        for item in walk(body):
            if item.get_role_name() != role or not item.get_state_set().contains(Atspi.StateType.SHOWING):
                continue
            bounds = item.get_extents(Atspi.CoordType.WINDOW)
            if bounds.y + bounds.height <= frame.y or bounds.y >= frame.y + frame.height:
                continue
            title = author = None
            focused = False
            for node in walk(item):
                if read_focus:
                    node.clear_cache()
                    focused |= node.get_state_set().contains(Atspi.StateType.FOCUSED)
                if node.get_role_name() == "label":
                    name = node.get_name() or ""
                    if name.startswith("Book "):
                        title = name
                        if not read_focus:
                            break
                    if read_focus and name.startswith("Author "):
                        author = name
            if title:
                rows.append((bounds, (title, author), focused))
    except GLib.Error:
        return []
    return sorted(rows, key=lambda row: (row[0].y, row[0].x))


def first_row_titles(body, number):
    rows = visible_books(body, number)
    return [title for bounds, (title, _), _ in rows if bounds.y == rows[0][0].y]


def wait_for_scroll(body, number, before, *, first_only=False):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        now = first_row_titles(body, number)
        if now and (now[:1] != before[:1] if first_only else now != before):
            return now
        time.sleep(POLL)
    raise AssertionError(f"the wheel did not move view {number} from {before!r}")


def wait_for_wheel_stop(body):
    """The top book can change several times as GTK finishes the wheel motion."""
    bar = find("", "scroll bar", parent=body.get_parent())
    deadline = time.monotonic() + 10
    last = None
    changed = time.monotonic()
    while time.monotonic() < deadline:
        value = Atspi.Value.get_current_value(bar)
        now = time.monotonic()
        if value != last:
            changed, last = now, value
        elif now - changed >= 0.25:
            return
        time.sleep(POLL)
    raise AssertionError("the scrollbar continued to move after the wheel event")


def focused_book(body, number):
    return next((book for _, book, focused in visible_books(body, number, read_focus=True) if focused), None)


def wait_for_focus(body, number, book=None):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        focused = focused_book(body, number)
        if focused and (book is None or focused == book):
            return focused
        time.sleep(POLL)
    visible = [(name, focused) for _, name, focused in visible_books(body, number, read_focus=True)]
    raise AssertionError(f"view {number} has no visible focus on {book!r}. Visible books: {visible!r}")


def view_switches():
    """Check top row and visible book focus on all six transitions."""
    wid = focus("Ebook Manager")
    for source, target in [(2, 1), (1, 2), (2, 3), (3, 1), (1, 3), (3, 2)]:
        key(f"ctrl+{source}")
        body = expect_view(source)
        add = find("Add", "toggle button")
        bounds = add.get_extents(Atspi.CoordType.WINDOW)
        smoke.xd("mousemove", "--window", wid, bounds.x + bounds.width // 2,
                 bounds.y + bounds.height // 2, "click", 1)
        find("Add", "menu")
        key("Escape", "ctrl+shift+a")
        before = first_row_titles(body, source)
        smoke.xd("mousemove", "--window", wid, 120, 400, "click", "--repeat", 60, "--delay", 15, 5)
        wait_for_scroll(body, source, before)
        # Wait through the wheel animation before recording the first title.
        time.sleep(0.3)
        anchor = first_row_titles(body, source)[0]
        key(f"ctrl+{target}")
        body = expect_view(target)
        deadline = time.monotonic() + 10
        while True:
            titles = first_row_titles(body, target)
            # GTK grids scroll by whole rows. Check that the anchor is on
            # the first row, including when it lands after the first slot.
            if (anchor in titles if target == 1 else titles[:1] == [anchor]):
                break
            assert time.monotonic() < deadline, (source, target, anchor, titles)
            time.sleep(POLL)
        print(f"PASS view {source} → {target}: {anchor!r}, first shown {titles[0]!r}", flush=True)
        # Now focus a book low in the source viewport. The title and author
        # together distinguish it from the other copies of the same title.
        key(f"ctrl+{source}")
        body = expect_view(source)
        frame = body.get_extents(Atspi.CoordType.WINDOW)
        bounds, book, _ = next(
            row for row in reversed(visible_books(body, source, read_focus=True))
            if row[0].y >= frame.y and row[0].y + row[0].height <= frame.y + frame.height
        )
        smoke.xd("mousemove", "--window", wid, bounds.x + bounds.width // 2,
                 bounds.y + bounds.height // 2, "click", 1)
        wait_for_focus(body, source, book)
        key(f"ctrl+{target}")
        wait_for_focus(expect_view(target), target, book)
        print(f"PASS view {source} → {target} keeps focus visible on {book!r}", flush=True)
    key("ctrl+1")


def small_epub(path, n):
    ns = "http://www.w3.org/1999/xhtml"
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr("mimetype", "application/epub+zip", ZIP_STORED)
        archive.writestr(
            "META-INF/container.xml",
            """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>""",
        )
        archive.writestr(
            "book.opf",
            f'<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:10000000-0000-4000-8000-{n:012d}</dc:identifier><dc:title>Imported {n:04d}</dc:title><dc:creator>Scale Check</dc:creator><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/></spine></package>',
        )
        archive.writestr(
            "nav.xhtml",
            f'<html xmlns="{ns}" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="a.xhtml">One</a></li></ol></nav></body></html>',
        )
        archive.writestr(
            "a.xhtml",
            f'<html xmlns="{ns}"><head><title>One</title></head><body><p>Book {n}.</p></body></html>',
        )


def shelf_run(binary, root):
    settings = root / "config/ebook-manager/settings.json"
    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(
        json.dumps({"view": "shelf", "sort": {"shelf": [{"field": "author"}]}})
    )
    db = sqlite3.connect(root / "data/ebook-manager/library.sqlite")
    last = db.execute(
        "select title,authors from book order by author_key desc,n desc limit 1"
    ).fetchone()
    db.close()
    started = time.monotonic()
    proc = smoke.launch_app(binary, environment(root, EBOOK_SHELF_FRAME_TIMES="1"))
    try:
        body = find("Bookshelf")
        find("Book ", "table cell", contains=True, parent=body)
        launch_time = time.monotonic() - started
        assert launch_time < LAUNCH_LIMIT, launch_time
        find("#", "label", parent=body)
        wid = focus("Ebook Manager")

        def spines():
            return [n for n in walk(body) if n.get_role_name() == "table cell"]

        count = len(spines())
        pane = find("", "scroll pane")
        bounds = pane.get_extents(Atspi.CoordType.WINDOW)
        limit = (bounds.width // 16 + 1) * (bounds.height // 263 + 4)
        assert 0 < count <= limit, (count, limit)
        smoke.xd(
            "mousemove",
            "--window",
            wid,
            100,
            350,
            "click",
            "--repeat",
            24,
            "--delay",
            18,
            5,
        )
        time.sleep(0.5)

        samples = [
            int(line.split()[-1]) / 1000
            for line in (root / "app-stderr.log").read_text().splitlines()
            if line.startswith("shelf-scroll-frame-us ")
        ]
        assert samples, "no scroll frame timings"
        print(
            f"Shelf scroll frame intervals under Xvfb and Cairo, {len(samples)} samples: median {statistics.median(samples):.2f} ms, max {max(samples):.2f} ms",
            flush=True,
        )

        def first_full_row():
            top = pane.get_extents(Atspi.CoordType.WINDOW).y
            places = [(n.get_extents(Atspi.CoordType.WINDOW), n) for n in spines()]
            y = min(r.y for r, n in places if r.y >= top and r.height > 0)
            return [n for r, n in places if r.y == y]

        anchor = first_full_row()[0].get_name()
        old_width = pane.get_extents(Atspi.CoordType.WINDOW).width
        smoke.xd("windowsize", wid, 1020, 720)
        deadline = time.monotonic() + 5
        while True:
            try:
                if pane.get_extents(
                    Atspi.CoordType.WINDOW
                ).width != old_width and anchor in [
                    n.get_name() for n in first_full_row()
                ]:
                    break
            except (GLib.Error, ValueError):
                pass
            assert time.monotonic() < deadline, (anchor,old_width,pane.get_extents(Atspi.CoordType.WINDOW).width,[n.get_name() for n in first_full_row()][:3])
            time.sleep(POLL)
        book = first_full_row()[0]
        r = book.get_extents(Atspi.CoordType.WINDOW)
        smoke.xd(
            "mousemove",
            "--window",
            wid,
            r.x + r.width // 2,
            r.y + r.height // 2,
            "click",
            1,
        )
        key("End", "shift+Left")
        end = find(", ".join(last), "table cell", contains=True, parent=body)
        deadline = time.monotonic() + 5
        while True:
            selected = [
                n
                for n in spines()
                if n.get_state_set().contains(Atspi.StateType.SELECTED)
            ]
            if len(selected) == 2:
                break
            assert time.monotonic() < deadline, [n.get_name() for n in selected]
            time.sleep(POLL)
        print(
            f"PASS grouped shelf launch {launch_time:.3f}s, {count} spines (bound {limit}), resize kept {anchor}, selection reaches {end.get_name()}",
            flush=True,
        )
        key("ctrl+1")
        find("Books", "table")
        key("ctrl+w")
        assert proc.wait(timeout=10) == 0
    finally:
        if proc.poll() is None:
            stop(proc)



def run(binary, fixture, root):
    env = environment(root)
    subprocess.run(
        [fixture, root / "data/ebook-manager", str(BOOKS)], check=True, env=env
    )
    shelf_run(binary, root)
    incoming = root / "Incoming"
    incoming.mkdir()
    for n in range(IMPORTS):
        small_epub(incoming / f"{n:04d}.epub", n)

    # The first start is not counted: it loads the program and its
    # libraries into the file cache, as the catalogue benchmark warms its own.
    proc, *_ = launch(binary, env)
    stop(proc)
    before, seen = [], []
    for _ in range(STARTS):
        proc, look_began, titles_read, titles = launch(binary, env)
        # Title order: `Book 0` files before every other number, and 25 of
        # the generated books have that title.
        assert titles == ["Book 0 A distant atlas"] * FIRST, titles
        before.append(look_began)
        seen.append(titles_read)
        stop(proc)
    interval = max(b - a for a, b in zip(before, seen))
    print(
        f"launch to the first {FIRST} titles shown and read, {STARTS} starts, seconds: "
        f"median {statistics.median(seen):.3f} "
        f"[{min(seen):.3f}–{max(seen):.3f}], "
        f"the driver's look takes up to {interval:.3f}",
        flush=True,
    )
    assert statistics.median(seen) < LAUNCH_LIMIT + interval, seen
    print(f"PASS first titles within {LAUNCH_LIMIT} seconds of launch", flush=True)

    proc, *_ = launch(binary, env)
    try:
        find(f"{BOOKS:,} books")
        focus("Ebook Manager")
        view_switches()
        # A quarter of the generated titles end in `atlas`.
        typed = time.monotonic()
        smoke.search("atlas")
        find(f"{BOOKS // 4:,} books")
        print(
            f"search for atlas shown {time.monotonic() - typed:.2f} seconds after the first key, typing included",
            flush=True,
        )
        key("Down", "ctrl+End")
        find("Book 4092 A distant atlas")
        key("Escape")
        find(f"{BOOKS:,} books")
        # The book that had the focus is still on screen in the whole library.
        find("Book 4092 A distant atlas")
        key("ctrl+End")
        find("Book 4095 Straße")
        key("ctrl+Home")
        find("Book 0 A distant atlas")
        print("PASS search, and the last rows of the search and of the library", flush=True)

        # Move down a few screens, which selects a book on the way.
        key("Next", "Next", "Next")
        placed = settled()
        assert sum(selected for _, selected in placed) == 1, placed
        assert all(name.startswith("Book ") for name, _ in placed), placed
        count = find(f"{BOOKS:,} books", "label")
        wid = focus("Ebook Manager")
        changes, gaps, watching = [], [], False
        checked, scrolled, last = False, None, count.get_name()
        key("ctrl+shift+o")
        smoke.picker(incoming, "Choose a folder of EPUBs")
        deadline = time.monotonic() + 600
        while last != f"{BOOKS + IMPORTS:,} books":
            now = time.monotonic()
            assert now < deadline, "the import did not finish"
            count.clear_cache()
            name = count.get_name()
            if name != last:
                changes.append(now)
                if watching:
                    gaps.append(now - changes[-2])
                watching = True
                last = name
            if len(changes) == 3 and not checked:
                # Three refreshes in, the same books are in the same places
                # and the same one is selected.
                assert settled() == placed, (placed, cells())
                checked = True
                # Turn the wheel over the first column, beside the import
                # window, while the import runs.
                body = expect_view(1)
                before = first_row_titles(body, 1)
                assert before, "the grid had no visible titles before the wheel event"
                smoke.xd("mousemove", "--window", wid, 60, 400, "click", "--repeat", 24, "--delay", 12, 5)
                wait_for_scroll(body, 1, before, first_only=True)
                wait_for_wheel_stop(body)
                after = first_row_titles(body, 1)
                assert after and after[0] != before[0], (before, after)
                scrolled = settled()
                # Still moved once the view is still: a view put back after
                # the wheel would match what was there before it.
                assert {n for n, _ in scrolled} != {n for n, _ in placed}, (placed, scrolled)
                print(f"PASS import wheel moved the top title from {before[0]!r} to {after[0]!r}", flush=True)
                # Rows, not blanks: every cell on screen names its book.
                assert all(name.startswith("Book ") for name, _ in scrolled), scrolled
                # Those looks took seconds, and changes went unseen meanwhile.
                count.clear_cache()
                last, watching = count.get_name(), False
            time.sleep(POLL)
        print(
            f"import of {IMPORTS} books into {BOOKS:,}: the count was seen to change {len(changes)} times, "
            f"gaps {min(gaps):.2f} to {max(gaps):.2f} seconds",
            flush=True,
        )
        assert len(gaps) >= 4, changes
        # A late look by the driver can shorten one gap it measures.
        assert min(gaps) > REFRESH_INTERVAL - 0.2, gaps
        assert scrolled and settled() == scrolled, (scrolled, cells())
        print(
            "PASS during the import the list refreshed at most once a second, "
            "kept its place and selection, and scrolled to loaded rows",
            flush=True,
        )
    finally:
        stop(proc)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("fixture", type=Path)
    parser.add_argument(
        "--private-session", action="store_true", help=argparse.SUPPRESS
    )
    args = parser.parse_args()
    binary, fixture = args.binary.resolve(), args.fixture.resolve()
    if args.private_session:
        smoke.desktop(lambda home: run(binary, fixture, home))
    else:
        smoke.private_session(__file__, [binary, fixture])
