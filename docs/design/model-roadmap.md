# First model checks

Start with one EPUB, two readers and two offline installations. Import the book, mark a
passage, rename the file, then import a corrected copy while one device still holds the
original. The library should show one ordinary book. Both readers should recover their own
marks and know which copy each mark refers to. Use this sequence to exercise the model
before choosing a persistent schema. The [data-model inputs](../requirements/data-model-inputs.md) define the constraints.

## Build the first slice in this order

1. **Import and identify.** Allocate permanent IDs offline for shared and personal records.
   Keep the edition-to-file link explicit, allow an edition with no file, and keep paths on
   the installation that can open them. Import the same bytes at two paths, then rename one.
   Both locations should resolve to the original version. Two independently catalogued
   editions may share those bytes. The [catalogue experiment](catalogue.md) checks these relationships and paging by
   title. Custom-field sorting needs its own measurement.
2. **Assign ownership at creation.** Give catalogue facts a library and every personal mark,
   session and rating a user. Give collections, tags and custom fields an explicit shared or
   personal scope. Keep grants to local folders and jobs on their installation. Key
   rebuildable indexes by their inputs. Change the shared title, give each reader a
   different rating and a shelf with the same name, then rebuild the index. Both should see
   the new title and only their own personal values in search and export. Preserve field
   source, lock and storage permission on import. Exercise a locked correction and a
   display-only source before persisting raw imports or journalling their values. [Ownership](https://github.com/endqwerty/ebook-manager/issues/14)
3. **Track the bytes each device read.** Keep immutable versions with algorithm-labelled
   whole-file, partial and text fingerprints where available. Use partial matches as
   candidates and retain aliases for external identifiers. Record the parent version and
   whether a change was an edit, conversion to another file, or source replacement. Edit the
   same original twice. Both children must survive, and a device reporting the old
   fingerprint must still resolve to the old version. Try equal text in different packaging
   and different files with equal sampled bytes before choosing hash profiles. [File identity](https://github.com/endqwerty/ebook-manager/issues/10)
4. **Resolve a point and a range.** Store their original version, part, part and whole
   fractions, plus available structural coordinates and surrounding text. Try a chapter
   rename, a paragraph insertion, a repeated quote and a deleted passage on an EPUB, a
   conversion and a corrected edition. Check both ends of a cross-chapter highlight by hand.
   Keep the original anchor and note when a match is missing or ambiguous. Offer an
   approximate resume position separately. Freeze encodings only after round trips for a PDF
   page region, comic page and track offset as well. [Locator trials](https://github.com/endqwerty/ebook-manager/issues/11)
5. **Replay changes locally.** Give changes stable IDs and enough causal history to
   recognise retries and concurrent edits despite clock skew. Replay two offline histories
   in both orders and twice. Independent field edits survive, competing notes remain
   recoverable, tag additions and removals converge, and a stale edit cannot silently revive
   a deletion. Merge two independently imported catalogue records, follow the losing ID from
   an old device, then split them and check both users' annotations. Keep original
   associations wherever a split would otherwise require guessing. Choose conflict rules and
   tombstone retention after testing a year-offline device against compacted history.
   [Merge design](https://github.com/endqwerty/ebook-manager/issues/12)

Use the same fixture for a clean rebuild. Compare IDs, ownership, version parents, redirects
and unresolved anchors after restoring the portable records. Test an interrupted write and a
reader that encounters a newer format before freezing a sidecar version. Use the [storage and sync decision](library-storage.md)
and [platform plan](platforms.md) when implementing this trial. The [layout issue](https://github.com/endqwerty/ebook-manager/issues/13) tracks the on-disk format.

## Cases to put through the catalogue experiment

Use these as paper fixtures first. Ask a fresh reader to identify what appears on the shelf,
what opens and where a bookmark returns. Record any case that needs an exception before
fixing the entity boundaries. [Content cases](../requirements/02-content-types.md#where-one-book-record-is-the-wrong-unit)

| Fixture | What the model must let the reader do |
|---|---|
| Twelve tracks for one narration, one recording for a boxed set | Play ordered tracks as one edition. Open each contained title at its own interval in the shared recording. |
| Ebook, translation, abridgement and two narrators | Find all editions under the work. Resume each on its own timeline. Switch between text and narration at a verified pair of anchors when an alignment exists. |
| Anthology containing a story also owned separately | Find the story in both places and open the anthology at its entry. Removing one copy preserves the other. |
| Comic archive labelled chapter, issue, volume or whole run | Preserve the supplied kind, reading order and issue label such as “2015 Special”. Open the selected page even when an archive spans several issues. |
| Magazine issue containing articles | Browse by title, volume, issue label and date. Open an article while keeping reading state for the issue. |
| Serial replaced after a chapter was inserted | Resume in the matching passage of the new version. Recover notes on deleted text against the old version. |
| Dictionary and fixed-layout picture book | Select the dictionary as a lookup source by language. Open the picture book as one edition whose page layout, narration and text stay together. |
| Score with instrumental parts, saved website, folder of loose scans | Select the violin part within the score. Preserve the website's entry page and resource links. Page through scans in their stated order after moving the folder. |
| Paper copy or wishlist item | Catalogue it without bytes. Add a digital edition later and retain the original acquisition or loan record. |

## Decisions that can wait

- **Part storage and item types.** Start with ordered, version-bound parts and explicit
  kinds. Compare per-part rows with a manifest on a 5,000-page comic and a saved website.
  Measure opening, random page lookup, correcting one entry and rebuild cost before choosing
  a layout. Add kind-specific records when an importer or reader needs a distinct constraint
  or query.
- **Alignment and derived features.** Keep pairs of version-bound locators representable
  now. Build alignment when a text/audio switching trial can check matched passages and
  skipped abridged chapters. Add generated audio, summaries or text indexes when a consumer
  can demonstrate invalidation after an edit and deletion of private inputs.
- **Performance choices.** Use measured catalogue results at 100,000 and one million books
  before fixing custom-field indexes or denormalising the shelf. For annotation storage,
  fetch one chapter of a book with 10,000 marks. For history, replay twenty years of
  sessions and check that a manual reading status survives recomputation. These are fixture
  sizes, not observed performance.
