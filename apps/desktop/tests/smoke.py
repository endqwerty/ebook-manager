"""Run under a private Xvfb + DBus session. See apps/desktop/README.md."""

import argparse
import hashlib
import json
import os
import re
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import time
import zlib
from pathlib import Path
from xml.sax.saxutils import escape
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib

SCREEN = (1100, 800)

BUS_CONFIG = """<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-BUS Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:tmpdir=/tmp</listen>
  <auth>EXTERNAL</auth>
  <servicedir>{services}</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>
"""


def epub(path, revised=False):
    ns = "http://www.w3.org/1999/xhtml"
    with ZipFile(path, "w", ZIP_DEFLATED) as archive:
        archive.writestr("mimetype", "application/epub+zip", ZIP_STORED)
        archive.writestr(
            "META-INF/container.xml",
            """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>""",
        )
        archive.writestr(
            "book.opf",
            """<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:identifier id="id">urn:uuid:bbf9ffdf-0ec0-4962-a077-5375e799a012</dc:identifier><dc:title>The Lantern Atlas</dc:title><dc:creator>Mira Field</dc:creator><dc:language>en</dc:language><meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="b" href="b.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/></manifest><spine><itemref idref="a"/><itemref idref="b"/></spine></package>""",
        )
        archive.writestr(
            "nav.xhtml",
            f'<html xmlns="{ns}" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="a.xhtml">Harbor lights</a></li><li><a href="b.xhtml">Northern shore</a></li></ol></nav></body></html>',
        )
        for filename, title in [("a", "Harbor lights"), ("b", "Northern shore")]:
            paragraphs = "".join(
                f"<p>Marker {n}. A lantern shines along the shore.</p>"
                for n in range(80)
            )
            # Chapter one ends in a paragraph longer than a page, so its last
            # page is not the top of its last paragraph.
            if filename == "a":
                paragraphs += f"<p>{'The tide turns. ' * 150}Marker 80 ends it.</p>"
            archive.writestr(
                f"{filename}.xhtml",
                f'<html xmlns="{ns}"><head><title>{title}</title></head><body><h1>{title}</h1><p>{"Second edition" if revised else "First edition"}</p>{paragraphs}</body></html>',
            )


def png(width, height, pixel):
    """A PNG whose colour at each point is `pixel(x, y)`, with x and y from 0 to 1."""
    rows = b"".join(
        b"\0"
        + bytes(
            channel
            for x in range(width)
            for channel in pixel(x / (width - 1), y / (height - 1))
        )
        for y in range(height)
    )

    def chunk(kind, data):
        body = kind + data
        return struct.pack(">I", len(data)) + body + struct.pack(">I", zlib.crc32(body))

    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(rows))
        + chunk(b"IEND", b"")
    )


def artwork(ground, figure, style):
    """A drawn stand-in for cover art. It is not a real cover."""

    def pixel(x, y):
        if style == "band":
            inside = 0.18 < y < 0.42
        elif style == "disc":
            inside = (x - 0.5) ** 2 + ((y - 0.38) * 1.5) ** 2 < 0.09
        elif style == "stripes":
            inside = int((x + y * 1.5) * 7) % 2 == 0
        elif style == "frame":
            inside = not (0.1 < x < 0.9 and 0.07 < y < 0.93)
        else:
            inside = y > 0.62 - 0.12 * abs(x - 0.5)
        shade = 1 - 0.18 * y
        return tuple(int(c * shade) for c in (figure if inside else ground))

    return pixel


# Title, author, the author's sort name, series and position, cover art
# (ground, figure, style, width, height) or None, language, right to left.
GALLERY = [
    ("A Field Guide to Tides", "Mira Field", None, None, ((24, 94, 110), (232, 220, 190), "band", 200, 300), "en", False),
    ("The Glass Orchard", "Tomas Reyes", "Reyes, Tomas", ("Orchard", "1"), ((150, 52, 44), (240, 200, 120), "disc", 200, 300), "en", False),
    ("Salt and Iron", "Tomas Reyes", "Reyes, Tomas", ("Orchard", "2"), ((52, 70, 110), (220, 226, 236), "disc", 200, 300), "en", False),
    ("Northern Lights at Noon", "Ada Lindqvist", None, None, ((30, 44, 60), (120, 210, 170), "stripes", 200, 300), "en", False),
    ("The Cartographer’s Daughter and the Unfinished Map of the Western Isles", "Ines Calloway", None, None, ((214, 180, 96), (70, 50, 30), "frame", 210, 300), "en", False),
    ("Letters from the Harbor", "Jonas Ferreira and Amélie Dubois-Laurent", None, None, None, "en", False),
    ("Small Hours", "", None, None, None, "en", False),
    ("Tide Tables", "Harbor Board", None, None, ((236, 232, 220), (40, 90, 130), "hill", 300, 300), "en", False),
    ("海辺の灯台", "夏目 遥", None, None, ((200, 60, 70), (250, 240, 230), "hill", 200, 300), "ja", False),
    ("رسائل البحر", "ليلى حداد", None, None, ((40, 110, 90), (240, 230, 200), "band", 200, 300), "ar", True),
    ("Winter Count", "Ada Lindqvist", None, None, ((18, 18, 20), (60, 60, 66), "frame", 200, 300), "en", False),
    ("Paper Boats", "Mira Field", None, None, ((250, 250, 248), (226, 226, 220), "stripes", 200, 300), "en", False),
]


def gallery(folder):
    """Writes the books the screenshots show: covers of several kinds and
    books with none."""
    folder.mkdir()
    ns = "http://www.w3.org/1999/xhtml"
    for n, (title, author, sort, series, art, language, rtl) in enumerate(GALLERY):
        metadata = f"<dc:identifier id=\"id\">urn:uuid:00000000-0000-4000-8000-{n:012d}</dc:identifier><dc:title>{escape(title)}</dc:title><dc:language>{language}</dc:language><dc:date>{1990 + n * 3}</dc:date>"
        if author:
            metadata += f'<dc:creator id="creator">{escape(author)}</dc:creator>'
        if sort:
            metadata += f'<meta refines="#creator" property="file-as">{escape(sort)}</meta>'
        if series:
            metadata += f'<meta property="belongs-to-collection" id="series">{series[0]}</meta><meta refines="#series" property="collection-type">series</meta><meta refines="#series" property="group-position">{series[1]}</meta>'
        cover = '<item id="cover" href="cover.png" media-type="image/png" properties="cover-image"/>' if art else ""
        direction = ' page-progression-direction="rtl"' if rtl else ""
        with ZipFile(folder / f"{n:02d}.epub", "w", ZIP_DEFLATED) as archive:
            archive.writestr("mimetype", "application/epub+zip", ZIP_STORED)
            archive.writestr(
                "META-INF/container.xml",
                """<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>""",
            )
            archive.writestr(
                "book.opf",
                f'<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="id"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">{metadata}<meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata><manifest><item id="a" href="a.xhtml" media-type="application/xhtml+xml"/><item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/>{cover}</manifest><spine{direction}><itemref idref="a"/></spine></package>',
            )
            archive.writestr(
                "nav.xhtml",
                f'<html xmlns="{ns}" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head><body><nav epub:type="toc"><ol><li><a href="a.xhtml">One</a></li></ol></nav></body></html>',
            )
            archive.writestr(
                "a.xhtml",
                f'<html xmlns="{ns}"><head><title>One</title></head><body><p>{escape(title)}.</p></body></html>',
            )
            if art:
                ground, figure, style, width, height = art
                archive.writestr("cover.png", png(width, height, artwork(ground, figure, style)))


def walk(node):
    yield node
    for index in range(node.get_child_count()):
        child = node.get_child_at_index(index)
        if child:
            yield from walk(child)


def find(name="", role="", *, contains=False, parent=None):
    if role == "button":
        # AT-SPI renamed "push button" to "button" without changing its role.
        role = Atspi.role_get_name(Atspi.Role.PUSH_BUTTON)
    deadline = time.monotonic() + 15
    last_error = None
    while time.monotonic() < deadline:
        try:
            nodes = list(walk(parent or Atspi.get_desktop(0)))
            for node in nodes:
                value = node.get_name() or ""
                if (
                    (name in value if contains else name == value)
                    and (not role or role == node.get_role_name())
                    and node.get_state_set().contains(Atspi.StateType.VISIBLE)
                ):
                    return node
        except GLib.Error as exc:
            last_error = exc
        time.sleep(0.1)
    raise AssertionError(
        f"Missing {role} {name!r} (last AT-SPI error: {last_error}). Visible tree: {[(n.get_role_name(), n.get_name()) for n in walk(Atspi.get_desktop(0))][:100]}"
    )


def click(name, parent=None):
    obj = find(name, "button", parent=parent)
    assert obj.do_action(0), name


def accessible_text(node):
    # Older GTK clamps negative end offsets to zero for editable fields.
    return Atspi.Text.get_text(node, 0, Atspi.Text.get_character_count(node))


def xd(*args):
    subprocess.run(["xdotool", *map(str, args)], check=True)


def send_key(name):
    # Some XKB maps make xdotool add Alt when looking up function keys.
    # Send the physical keycode for bare F1 through F12.
    if function := re.fullmatch(r"F([1-9]|1[0-2])", name):
        keymap = subprocess.check_output(
            ["xkbcomp", "-xkb", os.environ["DISPLAY"], "-"], text=True
        )
        match = re.search(rf"<FK{int(function[1]):02d}>\s*=\s*([0-9]+);", keymap)
        assert match is not None, f"{name} is missing from the X server's keymap"
        name = match[1]
    xd("key", name)


def picker(path, title):
    find(title, contains=True)
    wid = subprocess.check_output(
        ["xdotool", "search", "--onlyvisible", "--name", title], text=True
    ).splitlines()[-1]
    xd("windowfocus", "--sync", wid)
    xd("key", "ctrl+l")
    time.sleep(0.18)
    find("", "text", parent=find(title, "dialog")).set_text_contents(str(path))
    time.sleep(0.4)
    xd("key", "Return")


def capture(title, out):
    wid = subprocess.check_output(
        ["xdotool", "search", "--onlyvisible", "--name", "^" + title + "$"], text=True
    ).splitlines()[-1]
    time.sleep(0.3)
    subprocess.run(["import", "-window", wid, str(out)], check=True)


def markers(image):
    text = subprocess.check_output(
        ["tesseract", str(image), "stdout"], stderr=subprocess.DEVNULL, text=True
    )
    return [int(n) for n in re.findall(r"Marker (\d+)", text)]


def size(title):
    wid = subprocess.check_output(
        ["xdotool", "search", "--onlyvisible", "--name", "^" + title + "$"], text=True
    ).splitlines()[-1]
    out = subprocess.check_output(
        ["xdotool", "getwindowgeometry", "--shell", wid], text=True
    )
    values = dict(line.split("=") for line in out.split())
    return int(values["WIDTH"]), int(values["HEIGHT"])


def keys(evidence):
    """Turn pages and toggle full screen from the keyboard, starting and ending in chapter one."""
    title = "The Lantern Atlas · Revised"

    def press(key):
        send_key(key)
        time.sleep(0.4)

    def page(name, blank=False):
        capture(title, evidence / name)
        found = markers(evidence / name)
        assert found or blank, name
        return found

    first = page("page-1.png")
    assert first[0] == 0, first
    press("Right")
    second = page("page-2.png")
    assert second[0] > 0 and first[-1] <= second[0] <= first[-1] + 1, (first, second)
    press("Left")
    assert page("page-1-again.png") == first
    press("Next")
    assert page("page-2-again.png") == second
    press("Prior")
    assert page("page-1-again.png") == first
    press("space")
    assert page("page-2-again.png") == second
    press("Right")
    assert page("page-3.png")[0] > second[0]
    press("Left")
    assert page("page-2-again.png") == second
    press("shift+space")
    assert page("page-1-again.png") == first
    shown, turns = first, 0
    while True:
        press("Right")
        turns += 1
        assert turns < 30, "Right never reached chapter two"
        if any("Chapter 2 of 2" in (n.get_name() or "") for n in walk(Atspi.get_desktop(0))):
            break
        # Pages inside the closing paragraph show no marker.
        turned = page("page-turned.png", blank=True)
        if turned:
            assert turned[0] <= shown[-1] + 1 and turned[-1] > shown[-1], (shown, turned)
            shown = turned
    assert shown[-1] == 80, shown
    assert page("chapter-2.png")[0] == 0
    press("Left")
    find("Chapter 1 of 2", contains=True)
    assert page("chapter-1-end.png", blank=True)[-1:] == [80]
    print(
        f"PASS arrow and page keys: {turns} turns through chapter one without a skipped paragraph, back from chapter two to its last page",
        flush=True,
    )
    windowed = size(title)
    press("F11")
    find("Leave full screen", "button")
    assert size(title) == SCREEN, size(title)
    capture(title, evidence / "full-screen.png")
    press("Right")
    find("Chapter 2 of 2", contains=True)
    press("Left")
    find("Chapter 1 of 2", contains=True)
    press("Escape")
    find("Full screen", "button")
    assert size(title) == windowed, (size(title), windowed)
    click("Full screen")
    find("Leave full screen", "button")
    assert size(title) == SCREEN, size(title)
    press("F11")
    find("Full screen", "button")
    assert size(title) == windowed, (size(title), windowed)
    print("PASS full screen from F11 and its button, left with Escape and F11", flush=True)


def focus(title):
    wid = subprocess.check_output(
        ["xdotool", "search", "--onlyvisible", "--name", "^" + title + "$"], text=True
    ).splitlines()[-1]
    xd("windowactivate", "--sync", wid)
    return wid


def key(*names):
    for name in names:
        send_key(name)
        time.sleep(0.15)


def type_text(text):
    xd("type", "--delay", "20", text)
    time.sleep(0.3)


def replace_text(text):
    """Replaces the focused field's text from the keyboard."""
    key("ctrl+a")
    type_text(text)


def gone(name):
    """Waits until nothing visible has this name."""
    deadline = time.monotonic() + 15
    while time.monotonic() < deadline:
        try:
            if not any(
                (n.get_name() or "") == name
                and n.get_state_set().contains(Atspi.StateType.SHOWING)
                for n in walk(Atspi.get_desktop(0))
            ):
                return
        except GLib.Error:
            pass
        time.sleep(0.1)
    raise AssertionError(f"{name!r} is still shown")


def search(words):
    """Types into the library window, which opens the search."""
    type_text(words)


def select_book(words):
    """Finds a book by typing and selects it, as a reader would."""
    focus("Ebook Manager")
    key("Escape")
    search(words)
    time.sleep(0.5)
    key("Down")


def painted(image, colour, fuzz):
    """How many pixels of the picture are within `fuzz` percent of a colour."""
    target = "rgb({},{},{})".format(*colour)
    out = subprocess.check_output(
        [
            # ImageMagick 7 names its command `magick`, and 6 `convert`.
            shutil.which("magick") or "convert",
            str(image),
            "-alpha",
            "off",
            "-fuzz",
            f"{fuzz}%",
            "-fill",
            "black",
            "+opaque",
            target,
            "-fill",
            "white",
            "-opaque",
            target,
            "-format",
            "%[fx:mean*w*h]",
            "info:",
        ],
        text=True,
    )
    return float(out)


BINDINGS = [
    (0x24, 0x40, 0x5C),
    (0x6F, 0x2A, 0x2A),
    (0x31, 0x50, 0x47),
    (0x7A, 0x3B, 0x22),
    (0x51, 0x44, 0x6B),
    (0xB9, 0x8A, 0x3C),
    (0x46, 0x53, 0x66),
    (0x2F, 0x3A, 0x2B),
]


def environment(root, **extra):
    return dict(
        os.environ,
        HOME=str(root),
        XDG_DATA_HOME=str(root / "data"),
        XDG_CONFIG_HOME=str(root / "config"),
        XDG_CACHE_HOME=str(root / "cache"),
        GDK_SCALE="1",
        GDK_DEBUG="no-portals",
        GDK_BACKEND="x11",
        GTK_A11Y="atspi",
        GSK_RENDERER="cairo",
        **extra,
    )


def launch_app(binary, env):
    with (Path(env["HOME"]) / "app-stderr.log").open("a") as log:
        return subprocess.Popen([binary], env=env, stderr=log)


def check_diagnostics(root):
    log = root / "app-stderr.log"
    if not log.exists():
        return
    diagnostic = re.compile(
        r"(?:^|\s)(?:[\w.-]+-)?(?:WARNING|CRITICAL|ERROR) \*\*:"
    )
    # GTK 4.24 on X11 also reports this on main when a reader drop-down
    # and its window repaint together. It is a duplicate frame timing report.
    allowed = re.compile(
        r"\bGdk-WARNING \*\*: \d{2}:\d{2}:\d{2}\.\d+: "
        r"gdk_frame_timings_submitted\(\) called on submitted frame\.$"
    )
    unexpected = [
        line for line in log.read_text(errors="replace").splitlines()
        if diagnostic.search(line) and not allowed.search(line)
    ]
    assert not unexpected, "\n".join(unexpected)
    print("PASS no unexpected GTK warnings or criticals from any app process", flush=True)


def stop(proc):
    proc.terminate()
    assert proc.wait(timeout=10) in (0, -signal.SIGTERM), proc.returncode


def run(binary, evidence, root):
    env = environment(root)
    books = root / "Books"
    books.mkdir()
    dest = root / "Device"
    dest.mkdir()
    source = books / "01-Atlas.epub"
    epub(source)
    shutil.copyfile(source, books / "02-Identical.epub")
    linked = root / "Outside.epub"
    shutil.copyfile(source, linked)
    (books / "02-Linked.epub").symlink_to(linked)
    epub(books / "03-Revised.epub", revised=True)
    (books / "04-Damaged.epub").write_bytes(b"synthetic invalid file")
    gallery(root / "Gallery")
    originals = {p: hashlib.sha256(p.read_bytes()).digest() for p in books.iterdir()}
    proc = None
    try:
        proc = launch_app(binary, env)
        find("No books yet")
        capture("Ebook Manager", evidence / "empty.png")
        focus("Ebook Manager")
        key("ctrl+shift+o")
        picker(books, "Choose a folder of EPUBs")
        click("Replace selected")
        find("Import complete")
        for text in ("Added ·", "Duplicate ·", "Replaced ·", "Failed ·"):
            find(text, "label", contains=True)
        find("02-Linked.epub\nDuplicate ·", "label", contains=True)
        capture("Import results", evidence / "import.png")
        click("Done")
        find("1 book")
        print(
            "PASS folder import, duplicate, changed-copy review and invalid file",
            flush=True,
        )
        focus("Ebook Manager")
        key("ctrl+o")
        picker(books / "03-Revised.epub", "Choose EPUB files")
        find("Import complete")
        find("Duplicate ·", "label", contains=True)
        click("Done")
        print("PASS file picker import", flush=True)
        # Typing anywhere in the window searches.
        focus("Ebook Manager")
        # Faster than anyone types: every key must reach the search field,
        # those typed while it opens included.
        xd("type", "--delay", "3", "unmatched")
        find("No books match")
        find("Nothing in the library matches “unmatched”.", "label")
        capture("Ebook Manager", evidence / "no-match.png")
        key("Escape")
        find("1 book")
        # A dead key and its letter go through the search field's input
        # method, from a closed search.
        xd("key", "--delay", "100", "dead_acute", "e")
        time.sleep(0.5)
        field = find("Search library", "entry")
        assert field.get_state_set().contains(Atspi.StateType.SHOWING)
        text = accessible_text(field)
        assert text == "é", text
        key("Escape")
        # Backspace cancels a dead key that is waiting for its letter.
        xd("key", "--delay", "100", "dead_acute", "BackSpace", "e")
        time.sleep(0.5)
        field = find("Search library", "entry")
        text = accessible_text(field)
        assert text == "e", text
        key("Escape")
        select_book("mira")
        find("The Lantern Atlas")
        key("F2")
        edit = find("Edit book", "frame")
        replace_text("The Lantern Atlas · Revised")
        key("Tab")
        replace_text("Mira Vale")
        key("Tab")
        type_text("Lantern Atlas")
        key("Tab")
        type_text("Vale, Mira")
        key("Tab")
        type_text("Tidewater")
        key("Tab")
        type_text("1.5")
        key("Return")
        gone("Edit book")
        find("Mira Vale")
        find("The Lantern Atlas · Revised")
        # The series is searched, and the filing fields were stored.
        select_book("tidewater vale")
        find("The Lantern Atlas · Revised")
        # Typing with a book focused goes on to the search field.
        type_text("x")
        find("Nothing in the library matches “tidewater valex”.", "label")
        key("BackSpace")
        find("The Lantern Atlas · Revised")
        key("Down")
        key("F2")
        edit = find("Edit book", "frame")
        for name, value in (
            ("Sort title", "Lantern Atlas"),
            ("Author sort names", "Vale, Mira"),
            ("Series", "Tidewater"),
            ("Position in series", "1.5"),
        ):
            field = find(name, "text", parent=edit)
            assert accessible_text(field) == value, name
        key("Escape")
        gone("Edit book")
        print("PASS search by typing, edit and filing from the keyboard", flush=True)
        focus("Ebook Manager")
        # The Menu key opens the focused book's menu, and Edit is in it.
        key("Menu")
        find("", "menu item")
        # Read has the focus. Edit is the next item.
        key("Down", "Return")
        find("Edit book", "frame")
        key("Escape")
        gone("Edit book")
        print("PASS a book's menu from the keyboard", flush=True)
        focus("Ebook Manager")
        key("Return")
        find("Chapter 1 of 2", contains=True)
        keys(evidence)
        click("Next")
        find("Chapter 2 of 2", contains=True)
        view = find("", "text")
        assert "Northern shore" in accessible_text(view)
        capture("The Lantern Atlas · Revised", evidence / "reader.png")
        wid = subprocess.check_output(
            [
                "xdotool",
                "search",
                "--onlyvisible",
                "--name",
                "The Lantern Atlas · Revised",
            ],
            text=True,
        ).splitlines()[-1]
        xd("mousemove", "--window", wid, 300, 260, "click", "--repeat", 12, 5)
        time.sleep(0.6)
        capture("The Lantern Atlas · Revised", evidence / "scrolled.png")
        before = markers(evidence / "scrolled.png")
        assert before and before[0] > 0, before
        # End the first process with the reader still open after scrolling settled.
        stop(proc)
        for attempt in range(3):
            if attempt:
                focus("Ebook Manager")
                key("ctrl+w")
                assert proc.wait(timeout=10) == 0
            proc = launch_app(binary, env)
            find("Mira Vale")
            select_book("lantern")
            key("Return")
            find("Chapter 2 of 2", contains=True)
            assert "Northern shore" in accessible_text(find("", "text"))
            capture("The Lantern Atlas · Revised", evidence / "resumed.png")
            after = markers(evidence / "resumed.png")
            assert after and abs(after[0] - before[0]) <= 1, (before, after)
            print(
                f"PASS first visible paragraph before close {before[0]}, after restart {after[0]}",
                flush=True,
            )
            click("Back to library")
            time.sleep(0.3)
        print("PASS chapter navigation and resume after process restart", flush=True)
        for expected in ("Book exported", "Already exported"):
            select_book("lantern")
            key("ctrl+d")
            picker(dest, "Choose an export folder")
            find(expected, contains=True)
            click("Close", find("", "dialog"))
            time.sleep(0.3)
        files = list(dest.glob("*.epub"))
        assert (
            len(files) == 1
            and files[0].read_bytes() == (books / "03-Revised.epub").read_bytes()
        )
        assert all(
            hashlib.sha256(p.read_bytes()).digest() == digest
            for p, digest in originals.items()
        )
        print(
            "PASS folder export, idempotent repeat and unchanged source bytes",
            flush=True,
        )
        covers(binary, evidence, root, proc)
        proc = None
    finally:
        if proc and proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            proc.wait(timeout=10)


LANTERN = "The Lantern Atlas · Revised"
CARTOGRAPHER = GALLERY[4][0]
FIELDS = ["Title", "Author", "Series", "Date Added", "Date Published", "Last Read", "Progress"]


def titles_shown(parent=None):
    """The titles of the books on screen, in the order they are shown."""
    known = {book[0] for book in GALLERY} | {LANTERN}
    return [
        node.get_name()
        for node in walk(parent or Atspi.get_desktop(0))
        if node.get_role_name() == "label"
        and node.get_name() in known
        and node.get_state_set().contains(Atspi.StateType.SHOWING)
    ]


def expect_titles(expected, what, *, parent=None):
    """Waits for the first books on screen to be these, in this order."""
    deadline = time.monotonic() + 15
    seen = None
    while time.monotonic() < deadline:
        try:
            seen = titles_shown(parent)[: len(expected)]
        except GLib.Error:
            seen = None
        if seen == expected:
            return
        time.sleep(0.1)
    raise AssertionError(f"{what}: expected {expected}, the window shows {seen}")


def headers():
    """The table's column headers, in order."""
    return [
        node.get_name()
        for node in walk(Atspi.get_desktop(0))
        if node.get_role_name() == "filler"
        and node.get_name()
        and node.get_state_set().contains(Atspi.StateType.SHOWING)
    ]


def expect_headers(expected):
    deadline = time.monotonic() + 15
    seen = None
    while time.monotonic() < deadline:
        try:
            seen = headers()
        except GLib.Error:
            seen = None
        if seen == expected:
            return
        time.sleep(0.1)
    raise AssertionError(f"expected the headers {expected}, the table has {seen}")


def choose(wanted):
    """Sets the focused drop-down to its choice `wanted`, counting from
    the top, with the keyboard."""
    key("space")
    time.sleep(0.3)
    key("Home", *(["Down"] * wanted), "Return")
    time.sleep(0.3)


def capture_screen(title, out):
    """A picture of a window with the menus and popovers over it, which
    are windows of their own."""
    wid = subprocess.check_output(
        ["xdotool", "search", "--onlyvisible", "--name", "^" + title + "$"], text=True
    ).splitlines()[-1]
    geometry = dict(
        line.split("=")
        for line in subprocess.check_output(
            ["xdotool", "getwindowgeometry", "--shell", wid], text=True
        ).split()
    )
    time.sleep(0.3)
    subprocess.run(
        [
            "import",
            "-window",
            "root",
            "-crop",
            f"{geometry['WIDTH']}x{geometry['HEIGHT']}+{geometry['X']}+{geometry['Y']}",
            "+repage",
            str(out),
        ],
        check=True,
    )


def first_row():
    """How many books are in the first row of the grid."""
    tops = [
        node.get_extents(Atspi.CoordType.WINDOW).y
        for node in walk(Atspi.get_desktop(0))
        if node.get_role_name() == "table cell"
        and node.get_state_set().contains(Atspi.StateType.SHOWING)
    ]
    return tops.count(min(tops))


def sorting(evidence):
    """Shows the books as a list and a table and sorts them by every field
    and by three keys, with the keyboard alone."""
    focus("Ebook Manager")
    key("Escape", "ctrl+2")
    # A list row gives the series and the book's place in it.
    find("Orchard · 1", "label")
    find("Tidewater · 1.5", "label")
    key("ctrl+3")
    expect_headers(["Title", "Author", "Series", "Date Added", "Progress"])
    print("PASS the list and the table from the keyboard", flush=True)
    # With a book selected each sort would scroll to it. With none, each
    # starts at the top.
    key("ctrl+shift+a")

    # The sort editor: one field after another in its first row. The Lantern
    # Atlas was added first and is the one being read. The gallery's books
    # were published in the order they were written out.
    key("ctrl+shift+s")
    # A drop-down is named by its choice. The first row starts on Title.
    find("Title", "combo box")
    first = {
        "Author": "Tide Tables",
        "Series": "The Glass Orchard",
        "Date Added": LANTERN,
        "Date Published": "A Field Guide to Tides",
        "Last Read": LANTERN,
        "Progress": LANTERN,
        "Title": CARTOGRAPHER,
    }
    for field, title in first.items():
        choose(FIELDS.index(field))
        expect_titles([title], f"sorted by {field}")
    # Three keys: series, then author, then title from Z to A. Two books
    # share an author twice over, and the third key orders them.
    choose(FIELDS.index("Series"))
    key("Tab", "Tab")
    choose(1 + FIELDS.index("Author"))
    key("Tab", "Tab")
    choose(1 + FIELDS.index("Title"))
    key("Tab")
    choose(1)
    by_three = [
        "The Glass Orchard",
        "Salt and Iron",
        LANTERN,
        "Tide Tables",
        CARTOGRAPHER,
        "Letters from the Harbor",
        "Paper Boats",
        "A Field Guide to Tides",
        "Winter Count",
        "Northern Lights at Noon",
    ]
    expect_titles(by_three, "sorted by series, author and title")
    expect_headers(["Title ³", "Author ²", "Series", "Date Added", "Progress"])
    capture_screen("Ebook Manager", evidence / "sort-editor.png")
    key("Escape")
    print("PASS every sort field and three sort keys through the sort editor", flush=True)
    return by_three


def scroll_to_top():
    wid = focus("Ebook Manager")
    xd("mousemove", "--window", wid, 100, 300, "click", "--repeat", 20, 4)
    xd("mousemove", 2, 2)


def table_headers(by_three):
    """Sorts from the table's headers, hides and moves a column from a
    header's menu, and changes the cover size, with the keyboard alone."""
    focus("Ebook Manager")
    # A menu opens with its first item focused unless the pointer is over
    # another, so the pointer is put out of the way.
    xd("mousemove", 2, 2)
    key("ctrl+3")
    expect_headers(["Title ³", "Author ²", "Series", "Date Added", "Progress"])
    scroll_to_top()
    expect_titles(by_three, "the table's own sort")
    # Up from the first row is the first header. Return sorts by it alone.
    # Finding a book puts the focus in the table. With nothing selected a
    # sort starts at the top.
    select_book("glass")
    key("Escape", "ctrl+Home", "ctrl+shift+a", "Up", "Return")
    expect_titles([CARTOGRAPHER, "A Field Guide to Tides"], "sorted by the Title header")
    # Control adds a header as the next key, and again turns it round.
    key("Right", "ctrl+Return")
    expect_headers(["Title", "Author ²", "Series", "Date Added", "Progress"])
    key("Return")
    expect_titles(["Tide Tables", CARTOGRAPHER], "sorted by the Author header")
    key("Return")
    expect_headers(["Title", "Author", "Series", "Date Added", "Progress"])
    assert titles_shown()[0] != "Tide Tables", titles_shown()
    key("Return")
    expect_titles(["Tide Tables", CARTOGRAPHER], "sorted by the Author header again")
    # The header's menu: hide Series, the second of its columns.
    key("Menu")
    find("", "menu item")
    key("Down", "Return")
    expect_headers(["Title", "Author", "Date Added", "Progress"])
    # Move Author toward the start. The moves follow the six columns.
    select_book("glass")
    key("Escape", "ctrl+Home", "ctrl+shift+a", "Up", "Right", "Menu")
    find("", "menu item")
    key(*["Down"] * 6, "Return")
    expect_headers(["Author", "Title", "Date Added", "Progress"])
    wid = focus("Ebook Manager")
    bounds = find("Author", "filler").get_extents(Atspi.CoordType.WINDOW)
    xd("mousemove", "--window", wid, bounds.x + bounds.width // 2,
       bounds.y + bounds.height // 2, "click", 3)
    find("", "menu item")
    key("Escape")
    xd("mousemove", 2, 2)
    expect_headers(["Author", "Title", "Date Added", "Progress"])
    print("PASS Table header menu opens by right-click", flush=True)
    print("PASS sorting, hiding and moving columns from the table's headers", flush=True)

    def author_width():
        author = find("Author", "filler")
        return author.get_extents(Atspi.CoordType.WINDOW).width

    def resize_column(command, expected):
        key("Menu")
        find("", "menu item")
        key(*["Down"] * (8 if command == "Make Wider" else 9), "Return")
        deadline = time.monotonic() + 5
        while author_width() != expected:
            assert time.monotonic() < deadline, (command, expected, author_width())
            time.sleep(0.05)

    # Author moved with its focused header.
    width = author_width()
    resize_column("Make Wider", width + 24)
    resize_column("Make Narrower", width)
    resize_column("Make Wider", width + 24)
    print(f"PASS Author width from the keyboard: {width} → {width + 24} → {width} → {width + 24}", flush=True)

    # Large covers, from the main menu: the third of its items.
    send_key("F10")
    menu = find("Main Menu", "menu")
    find("", "menu item", parent=menu)
    key("Down", "Down", "Return")
    key("ctrl+1")
    scroll_to_top()
    expect_titles([CARTOGRAPHER], "the grid's own sort")
    # A row of 842 holds five covers of 128 and four of 160.
    deadline = time.monotonic() + 10
    while first_row() != 4:
        assert time.monotonic() < deadline, first_row()
        time.sleep(0.1)
    # Under 640 wide the views are a menu, and the table gives way to the list.
    wid = focus("Ebook Manager")
    key("ctrl+3")
    expect_headers(["Author", "Title", "Date Added", "Progress"])
    pane = find("", "scroll pane")
    decoration = size("Ebook Manager")[0] - pane.get_extents(Atspi.CoordType.WINDOW).width
    assert decoration >= 0, decoration
    select_book("glass")
    key("Escape", "ctrl+Home", "ctrl+shift+a", "Up", "Menu")
    find("", "menu item")
    xd("windowsize", "--sync", wid, 639 + decoration, 680)
    find("View", "button")
    find("Books", "list")
    scroll_to_top()
    find("Orchard · 1", "label")
    assert not headers(), headers()
    xd("windowsize", "--sync", wid, 640 + decoration, 680)
    expect_headers(["Author", "Title", "Date Added", "Progress"])
    assert find("", "scroll pane").get_extents(Atspi.CoordType.WINDOW).width == 640

    xd("windowsize", "--sync", wid, 639 + decoration, 680)
    find("View", "button")
    key("ctrl+2", "ctrl+3")
    xd("windowsize", "--sync", wid, 900, 680)
    find("Books", "list")
    scroll_to_top()
    find("Orchard · 1", "label")
    assert not headers(), headers()
    key("ctrl+1")
    print("PASS Table at 640, List at 639, and Ctrl+3 at 639 leaves the saved view alone", flush=True)
    return width + 24


def book_menus_across_resize():
    wid = focus("Ebook Manager")
    for number, name in [(3, "Table"), (2, "List"), (1, "Grid")]:
        key(f"ctrl+{number}")
        select_book("glass")
        key("Escape")
        find(f"{len(GALLERY) + 1} books")
        key("Menu")
        menu = find("", "menu")
        assert menu.get_state_set().contains(Atspi.StateType.SHOWING), name
        for width in (639, 900):
            xd("windowsize", "--sync", wid, width, 680)
            if width == 639:
                find("View", "button")
            role = {1: "table", 2: "list", 3: "tree table"}[number]
            if number == 3 and width == 639:
                role = "list"
            find("Books", role)
            deadline = time.monotonic() + 5
            while True:
                menu.clear_cache()
                if not menu.get_state_set().contains(Atspi.StateType.SHOWING):
                    break
                assert time.monotonic() < deadline, f"{name} book menu stayed open at {width}"
                time.sleep(0.05)
        print(f"PASS {name} book menu dismissed on resize and the library stayed usable", flush=True)


def remembered(author_width):
    """After a restart the view, each view's sort, the cover size and the
    table's columns are as they were left."""
    find(CARTOGRAPHER)
    focus("Ebook Manager")
    expect_titles([CARTOGRAPHER], "the grid's sort after a restart")
    assert first_row() == 4, first_row()
    key("ctrl+3")
    expect_headers(["Author", "Title", "Date Added", "Progress"])
    scroll_to_top()
    expect_titles(["Tide Tables", CARTOGRAPHER], "the table's sort after a restart")
    assert find("Author", "filler").get_extents(Atspi.CoordType.WINDOW).width == author_width
    key("ctrl+2")
    find("Books", "list")
    scroll_to_top()
    find("Orchard · 1", "label")
    expect_titles([CARTOGRAPHER], "the list's sort after a restart")
    print("PASS the view, sorts, cover size and columns after a restart", flush=True)


def gallery_view_switches():
    # All six directions, with books already loaded and the default title
    # sort in each view. Wait for a visible book inside the new view.
    gone("Import complete")
    focus("Ebook Manager")
    roles = {1: "table", 2: "list", 3: "tree table"}
    for target in (2, 3, 1, 3, 2, 1):
        send_key(f"ctrl+{target}")
        view = find("Books", roles[target])
        expect_titles([CARTOGRAPHER], f"books in view {target}", parent=view)
    print("PASS all six view switches show books in the gallery", flush=True)


def covers(binary, evidence, root, proc):
    """Imports the gallery and checks what is drawn for a book with a cover
    and for one without, in each appearance."""
    thumbs = root / "data/ebook-manager/cache/thumbs"
    with_cover = sum(1 for book in GALLERY if book[4])
    focus("Ebook Manager")
    key("Escape", "ctrl+shift+o")
    picker(root / "Gallery", "Choose a folder of EPUBs")
    find("Import complete")
    click("Done")
    find(f"{len(GALLERY) + 1} books")
    assert len(list(thumbs.glob("*.png"))) == with_cover, list(thumbs.iterdir())
    gallery_view_switches()
    # Return reads the selected book, as every key acts on the selection,
    # when the focus has moved off it.
    select_book("glass")
    key("Escape")
    find(f"{len(GALLERY) + 1} books")
    key("ctrl+Right")
    focused = [
        node.get_name()
        for node in walk(Atspi.get_desktop(0))
        if node.get_role_name() == "table cell"
        and node.get_state_set().contains(Atspi.StateType.FOCUSED)
    ]
    assert focused and not focused[0].startswith("The Glass Orchard"), focused
    key("Return")
    find("The Glass Orchard", "frame")
    click("Back to library")
    gone("Back to library")
    print("PASS Return reads the selected book and not the focused one", flush=True)
    by_three = sorting(evidence)
    key("ctrl+1")
    time.sleep(0.6)
    stop(proc)
    for look, extra in (
        ("light", {"ADW_DEBUG_COLOR_SCHEME": "prefer-light"}),
        ("dark", {"ADW_DEBUG_COLOR_SCHEME": "prefer-dark"}),
        ("contrast", {"ADW_DEBUG_HIGH_CONTRAST": "1"}),
    ):
        if look == "dark":
            # The folder is a cache: without it the covers are made again.
            shutil.rmtree(thumbs)
        proc = launch_app(binary, environment(root, **extra))
        try:
            find("A Field Guide to Tides")
            if look == "dark":
                deadline = time.monotonic() + 30
                while len(list(thumbs.glob("*.png"))) < 8:
                    assert time.monotonic() < deadline, list(thumbs.glob("*.png"))
                    time.sleep(0.2)
            time.sleep(1.2)
            image = evidence / f"grid-{look}.png"
            capture("Ebook Manager", image)
            # The teal cover's artwork, and a binding in one of the eight
            # colours for the two books with no cover. Each cover is some
            # 30,000 pixels.
            teal = painted(image, (21, 84, 99), 4)
            assert teal > 20000, teal
            bound = sum(painted(image, colour, 0.5) for colour in BINDINGS)
            assert bound > 20000, bound
            if look != "contrast":
                focus("Ebook Manager")
                key("ctrl+2")
                find("Orchard · 1", "label")
                time.sleep(1.0)
                capture("Ebook Manager", evidence / f"list-{look}.png")
                key("ctrl+3")
                expect_headers(["Title ³", "Author ²", "Series", "Date Added", "Progress"])
                scroll_to_top()
                expect_titles(by_three, "the table's sort")
                time.sleep(1.0)
                capture("Ebook Manager", evidence / f"table-{look}.png")
                key("ctrl+1")
                time.sleep(0.6)
        finally:
            stop(proc)
    print(
        "PASS covers and bindings in light, dark and more contrast, thumbnails made again",
        flush=True,
    )
    proc = launch_app(binary, environment(root))
    try:
        find("A Field Guide to Tides")
        author_width = table_headers(by_three)
        book_menus_across_resize()
        # Closing the window saves the choices.
        focus("Ebook Manager")
        key("ctrl+w")
        assert proc.wait(timeout=10) == 0
        proc = launch_app(binary, environment(root))
        remembered(author_width)
        key("ctrl+w")
        assert proc.wait(timeout=10) == 0

        settings = root / "config/ebook-manager/settings.json"
        data = json.loads(settings.read_text())
        data["future"] = "preserve this file on an unchanged close"
        original = json.dumps(data).encode()
        settings.write_bytes(original)
        before = settings.stat()
        proc = launch_app(binary, environment(root))
        find(CARTOGRAPHER)
        focus("Ebook Manager")
        key("ctrl+w")
        assert proc.wait(timeout=10) == 0
        assert settings.read_bytes() == original
        assert settings.stat().st_ino == before.st_ino
        print("PASS closing with unchanged settings leaves the original file alone", flush=True)
    finally:
        if proc.poll() is None:
            stop(proc)


def desktop(body):
    """Inside the private session: starts the accessibility registry and a
    window manager, runs `body`, and stops both."""
    os.environ["AT_SPI_BUS_ADDRESS"] = os.environ["DBUS_SESSION_BUS_ADDRESS"]
    registries = [
        Path("/usr/lib/at-spi2-registryd"),
        Path("/usr/libexec/at-spi2-registryd"),
    ]
    registry = next(p for p in registries if p.exists())
    proc = subprocess.Popen([registry])
    # Full screen is a request to the window manager, so the session needs one.
    manager = subprocess.Popen(["openbox"])
    try:
        deadline = time.monotonic() + 10
        while subprocess.run(
            ["xdotool", "getactivewindow"], capture_output=True
        ).returncode and subprocess.run(
            ["xdotool", "get_num_desktops"], capture_output=True, text=True
        ).stdout.strip() in ("", "0"):
            assert time.monotonic() < deadline, "openbox did not start"
            time.sleep(0.1)
        body(Path(os.environ["HOME"]))
    finally:
        manager.terminate()
        manager.wait(timeout=10)
        proc.terminate()
        proc.wait(timeout=10)
        check_diagnostics(Path(os.environ["HOME"]))


def private_session(script, arguments):
    """Runs `script` again with `--private-session`, under its own X server
    and its own D-Bus session, in a temporary home folder."""
    with tempfile.TemporaryDirectory() as temp:
        env = dict(
            os.environ,
            HOME=temp,
            XDG_DATA_HOME=temp + "/data",
            XDG_CONFIG_HOME=temp + "/config",
            XDG_CACHE_HOME=temp + "/cache",
            GDK_DEBUG="no-portals",
            GDK_SCALE="1",
            GDK_BACKEND="x11",
            GIO_USE_VFS="local",
            GSETTINGS_BACKEND="memory",
            LC_ALL="C.UTF-8",
        )
        for name in (
            "WAYLAND_DISPLAY",
            "DBUS_SESSION_BUS_ADDRESS",
            "AT_SPI_BUS_ADDRESS",
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_DESKTOP",
            "DESKTOP_SESSION",
            "HYPRLAND_INSTANCE_SIGNATURE",
        ):
            env.pop(name, None)
        # The private bus starts no services. Otherwise the app's requests
        # start the host's portals and file-system daemons, which crash
        # when this bus closes and show the user a crash notice.
        services = Path(temp) / "no-services"
        services.mkdir()
        config = Path(temp) / "bus.conf"
        config.write_text(BUS_CONFIG.format(services=escape(str(services))))
        subprocess.run(
            [
                "xvfb-run",
                "-a",
                "-s",
                f"-screen 0 {SCREEN[0]}x{SCREEN[1]}x24 -nolisten tcp",
                "dbus-run-session",
                f"--config-file={config}",
                "--",
                sys.executable,
                script,
                *map(str, arguments),
                "--private-session",
            ],
            env=env,
            check=True,
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("evidence", type=Path)
    parser.add_argument(
        "--private-session", action="store_true", help=argparse.SUPPRESS
    )
    args = parser.parse_args()
    binary = args.binary.resolve()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    if args.private_session:
        desktop(lambda home: run(binary, evidence, home))
    else:
        private_session(__file__, [binary, evidence])
