"""Exercise the proposed catalogue with an EPUB generated entirely in memory."""

from hashlib import sha256
from io import BytesIO
from pathlib import Path
import sqlite3
from uuid import uuid4
from xml.etree import ElementTree
from zipfile import ZIP_STORED, ZipFile, ZipInfo


def make_epub(revised=False):
    passage = "The boat came home at dawn." if revised else "The boat came home."
    files = {
        "mimetype": "application/epub+zip",
        "META-INF/container.xml": """<container version="1.0"
            xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
            <rootfiles><rootfile full-path="book.opf"
            media-type="application/oebps-package+xml"/></rootfiles></container>""",
        "book.opf": """<package version="3.0" unique-identifier="id"
            xmlns="http://www.idpf.org/2007/opf">
            <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
            <dc:identifier id="id">urn:example:harbour</dc:identifier>
            <dc:title>Harbour</dc:title><dc:language>en</dc:language>
            <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta></metadata>
            <manifest><item id="text" href="text.xhtml" media-type="application/xhtml+xml"/>
            <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
            </manifest><spine><itemref idref="text"/></spine></package>""",
        "text.xhtml": f"""<html xmlns="http://www.w3.org/1999/xhtml"><head>
            <title>Harbour</title></head><body><p>{passage}</p></body></html>""",
        "nav.xhtml": """<html xmlns="http://www.w3.org/1999/xhtml"
            xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head>
            <body><nav epub:type="toc"><ol><li><a href="text.xhtml">Harbour</a></li>
            </ol></nav></body></html>""",
    }
    stream = BytesIO()
    with ZipFile(stream, "w") as archive:
        for name, contents in files.items():
            archive.writestr(ZipInfo(name), contents, compress_type=ZIP_STORED)
    return stream.getvalue()


def add_generated_epub(db, data, path):
    digest = sha256(data).hexdigest()
    matches = db.execute("SELECT id FROM file_version WHERE sha256 = ?", (digest,)).fetchall()
    if len(matches) > 1:
        return "needs review"
    if matches:
        version, = matches[0]
        with db:
            prior = db.execute("SELECT version_id FROM location WHERE path = ?", (path,)).fetchone()
            if prior and prior != (version,):
                return "needs review"
            if prior is None:
                db.execute("INSERT INTO location VALUES (?, ?)", (path, version))
        return "duplicate"

    with ZipFile(BytesIO(data)) as archive:
        container = ElementTree.fromstring(archive.read("META-INF/container.xml"))
        package_path = container.find(".//{*}rootfile").attrib["full-path"]
        package = ElementTree.fromstring(archive.read(package_path))
        title = package.findtext(".//{http://purl.org/dc/elements/1.1/}title")
    work, edition, file, version = (str(uuid4()) for _ in range(4))
    with db:
        db.execute("INSERT INTO work VALUES (?, ?)", (work, title))
        db.execute("INSERT INTO edition VALUES (?, ?)", (edition, work))
        db.execute("INSERT INTO file VALUES (?)", (file,))
        db.execute("INSERT INTO edition_file VALUES (?, ?)", (edition, file))
        db.execute("INSERT INTO file_version VALUES (?, ?, ?)", (version, file, digest))
        db.execute("INSERT INTO location VALUES (?, ?)", (path, version))
    return "added"


def run():
    if not __debug__:
        raise SystemExit("Run with assertions enabled to check the catalogue model.")
    db = sqlite3.connect(":memory:")
    db.executescript(Path(__file__).with_name("catalogue.sql").read_text())
    data = make_epub()
    assert add_generated_epub(db, data, "books/harbour.epub") == "added"
    assert add_generated_epub(db, data, "books/harbour.epub") == "duplicate"
    assert db.execute("SELECT title FROM work").fetchall() == [("Harbour",)]
    print("Found Harbour after importing its EPUB twice.")

    version, file = db.execute("SELECT id, file_id FROM file_version").fetchone()
    assert add_generated_epub(db, data, "copies/harbour.epub") == "duplicate"
    db.execute("UPDATE location SET path = 'shelf/harbour.epub' WHERE path = 'books/harbour.epub'")
    assert db.execute("SELECT version_id FROM location WHERE path = 'shelf/harbour.epub'").fetchone() == (version,)
    replacement = make_epub(revised=True)

    # A new import at an occupied path fails after its catalogue rows were staged.
    db.commit()
    try:
        add_generated_epub(db, replacement, "shelf/harbour.epub")
    except sqlite3.IntegrityError as error:
        assert "location.path" in str(error)
    else:
        raise AssertionError("The occupied path was accepted")
    assert db.execute("SELECT count(*) FROM work").fetchone() == (1,)
    print("An occupied path rolled back the new catalogue entry.")

    with db:
        next_version = str(uuid4())
        db.execute("INSERT INTO file_version VALUES (?, ?, ?)",
                   (next_version, file, sha256(replacement).hexdigest()))
        db.execute("UPDATE location SET version_id = ? WHERE path = 'shelf/harbour.epub'", (next_version,))
        db.execute("INSERT INTO work VALUES ('light', 'Lighthouse'), ('paper', 'Paper Moon')")
        db.execute("INSERT INTO edition VALUES ('light-edition', 'light')")
        db.execute("INSERT INTO edition_file VALUES ('light-edition', ?)", (file,))
    assert db.execute("SELECT count(*) FROM file_version WHERE file_id = ?", (file,)).fetchone() == (2,)
    assert db.execute("SELECT path, version_id FROM location ORDER BY path").fetchall() == [
        ("copies/harbour.epub", version), ("shelf/harbour.epub", next_version)
    ]
    print("Moving retained the version ID. Explicit replacement retained both revisions.")

    catalogue = db.execute("""
        SELECT work.title, count(DISTINCT edition_file.file_id) FROM work
        LEFT JOIN edition ON edition.work_id = work.id
        LEFT JOIN edition_file ON edition_file.edition_id = edition.id
        GROUP BY work.id ORDER BY work.title
    """).fetchall()
    assert catalogue == [("Harbour", 1), ("Lighthouse", 1), ("Paper Moon", 0)]
    print("One file serves two works. Paper Moon appears without a file.")
    # Duplicate titles span page boundaries. Fetch the last page by title and ID.
    db.executemany("INSERT INTO work VALUES (?, ?)",
                   ((f"w{i:06}", f"Title {i // 8:06}") for i in range(100_000)))
    sql = "SELECT title, id FROM work WHERE (title, id) > (?, ?) ORDER BY title, id LIMIT 5"
    cursor = ("Title 012499", "w099994")
    assert db.execute(sql, cursor).fetchall() == [
        ("Title 012499", f"w{i:06}") for i in range(99_995, 100_000)
    ]
    for row in db.execute("EXPLAIN QUERY PLAN " + sql, cursor):
        print(row[-1])
    db.close()


if __name__ == "__main__":
    run()
