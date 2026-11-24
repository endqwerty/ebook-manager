# Mounted reader export

Call `export_epub(source, directory, title)` off the UI thread after the library
resolves the managed EPUB. The caller chooses an existing directory on a mounted
volume. The function copies bytes and returns the destination; it does not change
the source or rewrite the publication's embedded title.

The library title supplies a portable filename. If the name already exists,
matching bytes give `AlreadyPresent`. Different bytes go to a filename with a content-hash suffix. If that filename
is also occupied by different bytes, the existing files stay untouched. A transfer uses a temporary file in the selected
folder, checks its SHA-256 digest and publishes it without overwriting any other
file. Every file on the device is opened, created and renamed through the open
directory's descriptor, on Linux and macOS alike, so the transfer stays on that
directory if its path or mount changes. The returned path is the chosen folder
joined with the filename.

Allow space for one copy of the EPUB plus filesystem overhead. Export
first writes and verifies a hidden staging file. If the volume fills during
staging, the call returns an error and removes the temporary file when the
filesystem permits it. Pulling the card or killing the process can leave a
hidden `.ebook-*.part` file as large as the book. Retries leave it alone.
If it prevents another export, remove the leftover file after the transfer
has stopped.

Export publishes the staged file with an exclusive rename or a hard link
when supported. On macOS FAT and exFAT volumes, it frees the staged space,
creates the destination exclusively, copies from the open source, syncs the
new file and verifies its digest. A write or verification error triggers an
attempt to empty that file. An interrupted export can leave an empty or
partial EPUB under its final name. A retry preserves that EPUB and tries the
hash-suffixed name. If both names hold different bytes, choose another folder.

Run `cargo test -p ebook-device` from the workspace root. The operation tests use
temporary directories and check retries, name collisions and filenames derived
from unusual titles. [Device behaviour](../../specs/mounted-epub-export.md)
