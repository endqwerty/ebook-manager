# Mounted reader export

Call `export_epub(source, directory, title)` off the UI thread after the library
resolves the managed EPUB. The caller chooses an existing directory on a mounted
volume. The function copies bytes and returns the destination; it does not change
the source or rewrite the publication's embedded title.

The library title supplies a portable filename. If the name already exists,
matching bytes give `AlreadyPresent`. Different bytes go to a filename with a content-hash suffix. If that filename
is also occupied by different bytes, the existing files stay untouched. A transfer uses a temporary file on the selected
volume, checks its SHA-256 digest and publishes it without overwriting any other
file. On Linux the transfer goes through the open directory's file descriptor, so it
stays on that directory if the mount changes. On macOS it uses the directory's
path as it was when the transfer started, then checks that the path still
leads to the open directory before reporting success. If it doesn't, the
export fails and asks for the book to be sent again.

Run `cargo test -p ebook-device` from the workspace root. The operation tests use
temporary directories and check retries, name collisions and filenames derived
from unusual titles. [Device behaviour](../../specs/mounted-epub-export.md)
