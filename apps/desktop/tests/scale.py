"""Starts the desktop against a generated library of 100,000 books and
checks that it stays quick. See apps/desktop/README.md."""

import argparse
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
IMPORTS = 400
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


def first_row_titles(body, number):
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
            for label in walk(item):
                title = label.get_name() or ""
                if label.get_role_name() == "label" and title.startswith("Book "):
                    rows.append((bounds.y, bounds.x, title))
                    break
    except GLib.Error:
        return []
    first_y = min((row[0] for row in rows), default=None)
    return [title for y, _, title in sorted(rows) if y == first_y]


def view_switches():
    """Scroll with focus outside the books and check all six transitions."""
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
        deadline = time.monotonic() + 10
        while True:
            now = first_row_titles(body, source)
            if now and now != before:
                break
            assert time.monotonic() < deadline, (source, before, now)
            time.sleep(POLL)
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


def run(binary, fixture, root):
    env = environment(root)
    subprocess.run(
        [fixture, root / "data/ebook-manager", str(BOOKS)], check=True, env=env
    )
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
                smoke.xd("mousemove", "--window", wid, 60, 400, "click", "--repeat", 6, 5)
                scrolled = settled()
                assert {n for n, _ in scrolled} != {n for n, _ in placed}, scrolled
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
