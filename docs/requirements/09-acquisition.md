# Where books come from

Each lawful way a book reaches the library, and what the app must remember about it.
A book can arrive more than once: borrowed, then bought, then replaced by a corrected
file.

- `ACQ-1` (expected) A book records how it was acquired: the source, the account,
  the date, the original address, the store's product identifier, the price and
  currency, the order reference, and the licence terms that came with it. A book can
  have several such records.
- `ACQ-2` (expected) A book the user owns but cannot hold as a file is still
  catalogued: a Kindle, Apple Books or Kobo purchase that stays inside the store's
  app. The record links to the store and says where the copy is. Amazon is reported to have
  removed download-by-cable for purchases on 26 February 2025, so this case is now
  common. It is also reported to offer EPUB and PDF downloads of eligible unprotected
  purchases since 20 January 2026.
- `ACQ-3` (expected) Browse and download from open catalogues inside the app, over
  OPDS: Standard Ebooks, Project Gutenberg, and any catalogue the user adds. The app
  follows each catalogue's rules for automated use.
- `ACQ-4` (expected) Import from stores that sell files without protection, by the
  file the user downloaded. A watermarked file is recorded as personal to the buyer
  and is kept out of sharing by default.
- `ACQ-5` (later) Library loans are records with a library, a due date, a state and
  a place in a hold queue. The due date shows everywhere the book does, and the app
  warns before it lapses. When a loan ends the book record, positions and
  annotations stay, and attach again if the title is borrowed or bought later.
  Libby exports notes one title at a time.
- `ACQ-6` (later) Loans delivered as Readium LCP files open with the user's licence.
  See [content types](02-content-types.md).
- `ACQ-7` (later) Web serials and fan works that update: the app stores the source
  address, checks for new chapters on a schedule the site allows, and replaces the
  file while keeping the reader's place and notes. Dates last checked and last
  updated are separate. The works carry their own vocabulary: fandoms,
  relationships, warnings, word count, chapters posted out of chapters planned,
  complete or in progress.
- `ACQ-8` (later) Articles saved to read later, newsletters and feeds become books
  with a stored copy of the text. Import from Pocket, Instapaper, Wallabag and
  Readwise Reader exports.
- `ACQ-9` (later) Paper books catalogued beside ebooks: entered by scanning the
  barcode with a phone, with a shelf location and a record of who borrowed one.
- `ACQ-10` (later) A wish list: books not owned, with an optional release date and
  target price. Series views show books the user does not own, so gaps are visible.
- `ACQ-11` (later) Papers and other academic documents keep their identifiers and
  export cleanly to a reference manager. The app does not try to replace one.

## What the app must store

- Acquisition records per book: kind (purchase, loan, free download, bundle, gift,
  own work, web capture, feed), source, account, date, address, product identifier,
  price, order reference, licence note, region.
- Whether a file is present, and if not, where the copy lives.
- Loans: library, card, due date, state, hold position, how to return it, the
  licence file and a hint for its passphrase.
- The protection scheme of each acquired file and the account it is tied to.
- For serials and feeds: address, kind, schedule, last checked, last updated,
  last entry seen, counts, status.
- Paper copies: location, lent to, lent on.
- Wish-list entries: release date, target price.
- Per source: the access rules the app enforces (rate, User-Agent, permitted feed).
- Vocabulary for fan works as tags with a kind.

## Not verified

Both Amazon dates in `ACQ-2` come from news coverage, because Amazon's pages refused
automated readers. Hoopla, cloudLibrary and BorrowBox, the terms of fan fiction and serial sites, what
Apple Books and Google Play Books let a buyer export today, and the licence fields of
the library lending standards were not read from primary pages.

## Sources

Libby and Internet Archive help pages, the Readium LCP specification, Standard
Ebooks' feeds page, Open Library's developer pages, Kobo help, Zotero's storage
documentation, and coverage of the Pocket and Omnivore shutdowns.
