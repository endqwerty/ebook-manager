# Export an EPUB to a mounted reader

On Linux the reader export copies an existing managed EPUB into a selected
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
- A partial write stays under a temporary name. A complete write is synced and
  its digest checked before publishing the EPUB name. An abrupt disconnection can
  leave a hidden temporary file. Safely eject the volume after the transfer.


CrossPoint can display the title embedded in the EPUB instead of the filename.
A catalogue correction changes the exported filename. This MVP preserves the
embedded metadata. Updating it in an exported copy remains part of DEV-15.
