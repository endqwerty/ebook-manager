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

The copy gets its name without replacing anything, by the first way the
filesystem supports: a rename that refuses to replace, a hard link, or, on FAT
and exFAT under macOS, which have neither, a file created under the name only
if it is free. Export frees the temporary copy’s space, copies from the open
source into the new file, syncs it and checks its digest. That last
way touches only the file it created, so a file that something else puts under
the name meanwhile is left alone. An export cut short there leaves an empty or
partial file under the name. The next export of that book then finds the name
taken by different bytes and uses the name with the hash.

Run `cargo test -p ebook-device` from the workspace root. The operation tests use
temporary directories and check retries, name collisions and filenames derived
from unusual titles. [Device behaviour](../../specs/mounted-epub-export.md)
