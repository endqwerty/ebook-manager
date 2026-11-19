# A personal library for the desktop and an X4 Pro

A reader with a folder of DRM-free EPUBs should be able to move an evening's
reading onto the desktop or their Xteink X4 Pro. Start on Linux, offline, with
a library stored on this computer. The first release path has five steps.

1. Choose a folder or files. Get a result for each EPUB and an explanation when
   one needs attention. Preserve originals and keep a managed copy.
2. Close and reopen the app. Find a book by title or author, open its details
   and correct a mislabelled title or author. Opening a folder again must not
   produce a shelf full of duplicate entries.
3. Open the book, read its chapters in order, navigate to another chapter, then
   quit and return to the saved place. Publisher-controlled scripts and remote
   resources have no place in this reading path.
4. Enable USB Drive mode in the X4 Pro CrossPoint fork, or mount its SD card.
   Choose the device folder and send an EPUB. Export the same book twice.
   Identical bytes at the destination should remain intact. A name collision
   with another book should give the incoming book its own filename, keeping
   both readable under their original catalogue titles.
5. Recover the local catalogue from the library's portable records. Show a
   clear failure when a managed file or portable record is missing or damaged.

The shell coordinates these steps. The Rust library owns book identities, copies
and reading state. The isolated EPUB worker produces metadata and readable
content. The [device crate](../../crates/ebook-device/README.md) owns mounted-folder
export. A useful first desktop build can read EPUBs without launching any
store or signing into a service. Firmware installation, wireless sync, other
publication formats and page-faithful rendering can each follow with their own
device or publication fixtures.

Exercise the entire journey using a generated multi-chapter publication, a
duplicate, a changed copy, and a destination folder. Validate both the exported
bytes and the visible shelf after restart. Claim on-device acceptance only after
the copied book has actually been opened on the intended firmware.

[Library import](../requirements/01-library.md#importing), [reader](../requirements/04-reading.md) and [device transfer](../requirements/07-devices.md#sending) requirements define the larger release. The first export covers mounted storage (DEV-1) and parts of DEV-13. Embedded metadata export (DEV-15) remains open.
