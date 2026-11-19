"""Run under a private Xvfb + DBus session. See apps/desktop/README.md."""

import argparse
import hashlib
import os
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
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


def walk(node):
    yield node
    for index in range(node.get_child_count()):
        child = node.get_child_at_index(index)
        if child:
            yield from walk(child)


def find(name="", role="", *, contains=False, parent=None):
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


def xd(*args):
    subprocess.run(["xdotool", *map(str, args)], check=True)


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
        # Some X keymaps make xdotool add Alt to reach the F11 symbol, and the
        # reader ignores Alt+F11. Its keycode sends the bare key.
        xd("key", {"F11": "95"}.get(key, key))
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


def run(binary, evidence, root):
    env = dict(
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
    )
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
    originals = {p: hashlib.sha256(p.read_bytes()).digest() for p in books.iterdir()}
    proc = None
    try:
        proc = subprocess.Popen([binary], env=env)
        find("Your first book belongs here")
        capture("Ebook Manager", evidence / "empty.png")
        click("Add folder")
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
        click("Add files")
        picker(books / "03-Revised.epub", "Choose EPUB files")
        find("Import complete")
        find("Duplicate ·", "label", contains=True)
        click("Done")
        print("PASS file picker import", flush=True)
        search = find("Search library", "entry")
        search.set_text_contents("unmatched")
        find("No matching books")
        search.set_text_contents("mira")
        find("The Lantern Atlas")
        search.set_text_contents("")
        click("Edit")
        edit = find("Edit book", "frame")
        find("Book title", "text", parent=edit).set_text_contents(
            "The Lantern Atlas · Revised"
        )
        find("Book authors", "text", parent=edit).set_text_contents("Mira Vale")
        click("Save", edit)
        find("Mira Vale")
        capture("Ebook Manager", evidence / "shelf.png")
        print("PASS search and edit", flush=True)
        click("Read")
        find("Chapter 1 of 2", contains=True)
        keys(evidence)
        click("Next")
        find("Chapter 2 of 2", contains=True)
        view = find("", "text")
        assert "Northern shore" in Atspi.Text.get_text(view, 0, -1)
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
        proc.terminate()
        proc.wait(timeout=10)
        for attempt in range(3):
            if attempt:
                click("Close", find("Ebook Manager", "frame"))
                assert proc.wait(timeout=10) == 0
            proc = subprocess.Popen([binary], env=env)
            find("Mira Vale")
            click("Read")
            find("Chapter 2 of 2", contains=True)
            assert "Northern shore" in Atspi.Text.get_text(find("", "text"), 0, -1)
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
            click("Export")
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
    finally:
        if proc and proc.poll() is None:
            proc.send_signal(signal.SIGTERM)
            proc.wait(timeout=10)


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
            run(binary, evidence, Path(os.environ["HOME"]))
        finally:
            manager.terminate()
            manager.wait(timeout=10)
            proc.terminate()
            proc.wait(timeout=10)
    else:
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
            for key in (
                "WAYLAND_DISPLAY",
                "DBUS_SESSION_BUS_ADDRESS",
                "AT_SPI_BUS_ADDRESS",
                "XDG_CURRENT_DESKTOP",
                "XDG_SESSION_DESKTOP",
                "DESKTOP_SESSION",
                "HYPRLAND_INSTANCE_SIGNATURE",
            ):
                env.pop(key, None)
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
                    __file__,
                    str(binary),
                    str(evidence),
                    "--private-session",
                ],
                env=env,
                check=True,
            )
