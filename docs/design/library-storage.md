# Saving an edit and recovering a library

Build for a reader who edits a title offline, later opens it on another device,
and expects to keep it after replacing the first computer. Each installation
queries its own SQLite database. The portable change log is the recovery record
for the catalogue and portable user data. Book bytes remain ordinary files.
A readable sidecar beside each book shows its published metadata. A referenced
library keeps these records in its configured sidecar folder.

An always-on hub would make two devices wait for a third to exchange an edit.
Whole-book sidecars alone would make independent changes to title and author
compete for the same file. A log of individual changes lets both survive and
lets a device rebuild the database without copying a live SQLite file.

## What “saved” means

1. In one local transaction, write the changed rows, the journal entry and a
   pending publication record. Assign the change a stable identity. Acknowledge
   “saved on this device” after commit. Undo records another change in the same
   history. Group all parts of an import under one undo action.
2. Publish missing portable changes into their owning library and user scope.
   Write a temporary file, flush its contents, install it atomically, then make
   the directory update durable. Retries with the same ID and contents are safe.
   Different contents under one ID require attention. Publish the whole group
   before making it visible to a rebuilding reader.
3. Regenerate the affected sidecars and advance the publication checkpoint.
   Report “portable copy up to date” only when all acknowledged changes through
   that checkpoint can be recovered. A full disk leaves local edits pending.
   Restart resumes from the journal. Losing the only device before publication
   can lose an edit that still showed as pending.

Folder grants, absolute paths and keychain references stay on the installation.
Personal records need their owner from the first write. A shared book sidecar
must not include a reader's private notes. Check storage permission before
writing source data into any journal. Use synthetic public records for the first
replay trial. Define permitted export, retention and erasure for each data class
before writing real personal or restricted data to a portable log.

## When records disagree

| What the reader or app finds | What to do |
|---|---|
| SQLite is behind the log | Replay complete changes by ID. Preserve pending local edits before replacing a readable database. |
| A sidecar is a stale, unedited snapshot | Regenerate it from the published state. |
| A reader edited a sidecar | Compare it with the snapshot it came from. Preserve only the edited fields as a proposal. Apply accepted edits once against that base. Show concurrent values for review. |
| The base is missing, a sync tool made a conflict copy, or the format is newer | Preserve the file. Show recoverable fields for review when its version is understood. |
| Only a book and its sidecar survive | Offer a partial import. Check existing IDs and deletion markers. A complete rebuild needs the portable log, including deletions. |

Two failures in a three-device simulation changed the design. A restored writer
that resumed yesterday's counter reused an ID that a peer had already seen.
Give a rebuilt or restored database a fresh writer identity before it authors
edits. The other failure started with a phone sending an edit to a tablet. The
phone lost its database before publishing. The tablet had the edit, but a
publisher that wrote only its own changes left it out of the portable recovery
copy. Publish every eligible change the device has accepted, including a peer's.
Only the originating writer allocates IDs in its own sequence.

## First sync transport

Start with two user-approved devices on the home network. Sync is opt-in. The
transport exchanges missing changes within the approved scope and fetches selected
book versions by content hash. Apply the same changes through the same library
operation whether they arrive directly, through a future hosted peer or via files
in user-owned storage. No live database goes in a synced folder.

Replay must preserve independent edits, show competing user edits to one field,
and retain a deletion marker across rebuild. Order causally related edits by
what each author had seen. Use a stable display order for concurrent candidates.
The [model roadmap](model-roadmap.md) gives the offline histories that must pass
before choosing a merge algorithm, locator encoding or retention window.

## What to prove before implementing sync

First save a title, interrupt publication at each boundary, restart, and rebuild
from the portable folder alone. Compare IDs and values after a retry. Repeat
with a full disk, a restored writer, a relayed edit and an externally edited
sidecar. Include a fileless book and an interrupted multi-record import.

Run `python3 docs/design/storage-model.py 1000` to exercise logical publication
and replay in memory. Each history uses three devices. The model reruns the
histories with writer identity reuse and with peer publication disabled so the
reader can see which recovery checks fail. Filesystem
durability, detection of restored writer state, private-data erasure, device
pairing and access control still need implementation evidence. The first network
trial must demonstrate that an unapproved device receives no records. The first
shared-folder trial must survive incomplete and out-of-order delivery.
