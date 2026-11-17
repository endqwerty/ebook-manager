# Features built on a book's text

Search by meaning, summaries, recaps, speech and audio alignment. Reading apps added
these from 2023 to 2026, mostly through cloud services. None is needed for a first
release. They are listed because each one needs data the model should allow for now,
and adding that later means reworking annotations, positions and file identity.

## What others shipped, and how it was received

- Kindle added recaps of earlier books in a series, a summary of the story so far,
  and questions answered from the book's text. The question feature launched on
  11 December 2025 with no way for authors to opt out. The Authors Guild objected.
  After complaints about spoilers, Amazon limited answers to what the reader had
  already read.
- Kindle's machine translation for publishers drew a public objection from European
  writers' and translators' councils on 24 November 2025.
- Google Play Books added a spoiler-safe "catch me up", and lets books the user
  bought feed a notebook assistant.
- Kindle Scribe summarises and searches handwritten notebooks.
- Independent apps offer chat with a book, chapter summaries, neural voices and
  side-by-side translation.
- An open-source project aligns an audiobook with its EPUB so the text follows the
  narration. Read-along is the most requested feature on Audiobookshelf's tracker.
- In user discussions, some readers ask for "less AI", and one book server lost its
  community partly over undisclosed machine-written code. No survey was found that
  measures demand for summaries inside books.

## Requirements

- `DERIV-1` (later) Search the library by meaning as well as by words, using an
  index made on the user's machine.
- `DERIV-2` (later) Read aloud with natural voices that run on the device.
- `DERIV-3` (later) Align an audiobook with its text, so the reader can switch
  between them and read along.
- `DERIV-4` (later) Summaries of a chapter or of the story so far, limited to what
  the reader has read, with that limit stored beside the result.
- `DERIV-5` (later) A list of people, places and terms in a book with where they
  appear, limited the same way.
- `DERIV-6` (later) Translation shown beside the original, kept apart from the
  book's text.
- `DERIV-7` (later) Suggested tags and genres, and likely duplicates, offered as
  proposals the user reviews.
- `DERIV-8` (core, as a rule for all of the above) Everything derived is off or
  local by default. Sending a book's text to a service needs the user's consent for
  that book and that service, and is recorded in the log of data that left the
  machine.
- `DERIV-9` (core, as a rule) Every generated result shows what made it, when, and
  what part of the book it covers. The user can delete or regenerate it. It is never
  written into the book file and never passed off as the author's or a human
  translator's work.
- `DERIV-10` (core, as a rule) Derived data is rebuilt, not trusted, when its source
  changes. The user's own data (annotations, history, positions) is stored apart
  from anything that can be rebuilt. A Kobo update in September 2026 left owners
  without annotations and statistics for a time.
- `DERIV-11` (core, as a rule) A model or voice the app bundles or downloads is
  listed with its licence and size. Several popular ones are not compatible with
  the project's licence.

## What the app must store

Plan these now. Each is cheap to include in the first schema and costly to add later.

- A content hash for every file and every version of it, and a "stale" state for
  anything derived from it.
- One way of expressing a place in a book, shared by positions, annotations, text
  chunks, summaries, entity lists and alignment. See
  [annotations](05-annotations-history.md).
- Extracted plain text per chapter, with the hash of the file it came from.
- Text chunks with their place in the book.
- One general record for derived results: kind, source hash, what produced it
  (program or model, version, local or which service), the part of the book covered,
  when, and its state. Summaries, search vectors, entity lists, translations, speech
  audio and alignment maps all fit it.
- Consent per book and per service, and what kind of text may be sent.
- Reading sessions, not only totals.
- A job queue that can pause and resume heavy work.
- Large derived files (vectors, audio) kept outside the main database, where they
  can be deleted and rebuilt.

## Not verified

How much memory and time local speech, speech recognition and small language models
need on a laptop and a phone came from blog posts, so treat the feasibility ranking
as rough. Several apps named in the research could not be read at all.

## Sources

Amazon's announcements and help pages, statements from the Authors Guild and the
European Writers' Council, Google's product posts, model cards for the open models
named in the research report, and the round-two report on new features.
