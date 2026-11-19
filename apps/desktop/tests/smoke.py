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
from zipfile import ZIP_DEFLATED, ZIP_STORED, ZipFile

import gi

gi.require_version("Atspi", "2.0")
from gi.repository import Atspi, GLib


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
        capture("The Lantern Atlas · Revised", root / "scrolled.png")
        before = markers(root / "scrolled.png")
        assert before and before[0] > 0, before
        click("Back to library")
        time.sleep(0.3)
        click("Close", find("Ebook Manager", "frame"))
        assert proc.wait(timeout=10) == 0
        proc = subprocess.Popen([binary], env=env)
        find("Mira Vale")
        click("Read")
        find("Chapter 2 of 2", contains=True)
        assert "Northern shore" in Atspi.Text.get_text(find("", "text"), 0, -1)
        capture("The Lantern Atlas · Revised", root / "resumed.png")
        after = markers(root / "resumed.png")
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
        try:
            run(binary, evidence, Path(os.environ["HOME"]))
        finally:
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
                LC_ALL="C.UTF-8",
            )
            for key in (
                "WAYLAND_DISPLAY",
                "DBUS_SESSION_BUS_ADDRESS",
                "AT_SPI_BUS_ADDRESS",
            ):
                env.pop(key, None)
            subprocess.run(
                [
                    "xvfb-run",
                    "-a",
                    "-s",
                    "-screen 0 1100x800x24 -nolisten tcp",
                    "dbus-run-session",
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
