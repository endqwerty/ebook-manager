# Export an EPUB to a mounted reader

The reader export copies an existing managed EPUB into a selected
directory. For an Xteink X4 Pro running the CrossPoint fork, enable USB Drive
mode and select a folder on the mounted SD card, or mount the card in a reader.
CrossPoint documents native EPUB support and browsing folders on that card.
[Firmware description](https://github.com/endqwerty/crosspoint-reader/blob/9e4c058944c422823cd9f76f1a4c9c9c7edf7a57/README.md)

- Export preserves the source bytes. The library's display title becomes a safe
  filename in the chosen folder, including when the title contains a slash or is
  too long for a single filename.
- A retry with identical bytes at the destination reports that the book is
  already there. If another book has that name, export keeps it and appends the
  first 16 hex digits of the incoming EPUB's SHA-256 to the new filename. A
  retry recognises that copy too. If both names contain different bytes, the
  user chooses another folder.
- Export syncs and checks a temporary copy before publishing it. On macOS FAT
  and exFAT volumes, it frees the temporary space and copies from the original
  into an exclusively created destination file, then verifies the digest again. Disconnecting during that write can leave an empty or partial EPUB under
  the final name. A retry preserves that file and tries the hash-suffixed name.
  An interrupted transfer can also leave a hidden temporary file. Safely eject
  the volume after the transfer.


CrossPoint can display the title embedded in the EPUB instead of the filename.
A catalogue correction changes the exported filename. This MVP preserves the
embedded metadata. Exporting a revised book can leave both versions on the card. Reading progress on
the old device path stays with that path. Updating embedded metadata and managing
existing device copies remain part of DEV-15 through DEV-18.
