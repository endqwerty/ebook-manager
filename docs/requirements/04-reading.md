# Reading

The built-in reader for text, PDF, comics and audio, and the access needs it must
meet.

## Text

- `READ-1` (core) Open a book at the place the user left it. Show a table of
  contents, landmarks and the page list when the book has one.
- `READ-2` (core) Paged or continuous scrolling, one or two columns, and a choice
  of page-turn animation including none.
- `READ-3` (core) Font family, size and weight, line, letter and word spacing,
  margins, justification, hyphenation, and the choice between the book's own styles
  and the user's.
- `READ-4` (core) Themes: light, dark, sepia, true black, and custom foreground and
  background colours. A theme can follow the system or the time of day.
- `READ-5` (core) Settings have a scope chain: global default, library, series,
  book and device. The app shows which level is in force and offers a reset. Kavita
  creates hidden per-device profiles and Apple Books resets fonts, and users complain
  about both.
- `READ-6` (core) The user loads their own fonts.
- `READ-7` (core) Progress shows the percentage, the chapter, pages left in the
  chapter, and time left in the chapter and the book from the user's own pace. When
  the publisher supplied print page numbers the reader uses them, and says which
  print edition they match.
- `READ-8` (core) Search inside the book with results in context.
- `READ-9` (core) Select text to highlight, note, copy, look up, translate or
  share. Footnotes open in place.
- `READ-10` (core) Go back after following a link or jumping. Syncing to a position
  from another device is offered, never forced.
- `READ-11` (expected) Dictionary lookup from installed dictionaries, with word
  stemming, offline. Lookup on Wikipedia or the web sends the word out, so the
  reader says so.
- `READ-12` (expected) Translate a selection. A service that receives the text is
  named before use.
- `READ-13` (expected) Looked-up words are saved with their sentence and book, for
  review as flashcards.
- `READ-14` (expected) A reading ruler or line focus, and an option to dim the text
  around the current paragraph.
- `READ-15` (expected) Two books, or two places in one book, side by side.
- `READ-16` (expected) Fixed-layout books: one page per spine item, spreads, zoom
  and pan, with their own default settings.
- `READ-17` (expected) Right-to-left books and vertical Japanese text with ruby,
  emphasis marks and correct line breaking. Page turns follow the book's direction.
- `READ-18` (expected) Scripts in a book are off until the user allows them for that
  book. Remote resources are blocked until allowed. See
  [constraints](10-constraints.md).
- `READ-19` (later) A bilingual view that shows a translation beside or under the
  original.
- `READ-20` (later) An entity index for a book: people, places and terms with where
  they appear, limited to what the reader has reached.

## PDF

- `READ-30` (core) Page view with zoom, fit to width or page, single, double or
  continuous, rotation, and the outline. Page labels are shown when the file has
  them.
- `READ-31` (core) Text selection and search when the PDF has a text layer. The app
  says when it has none and offers OCR. See
  [conversion](06-conversion-editing.md).
- `READ-32` (expected) Crop margins. A reflow view that shows the extracted text in
  the text reader, with a warning that order may be wrong.
- `READ-33` (expected) Annotations on PDFs are kept beside the file and written
  into a copy on export. See [annotations](05-annotations-history.md).
- `READ-34` (core) PDF scripts, launch actions and embedded files never run.

## Comics

- `READ-40` (expected) Reading direction per series: left to right, right to left,
  vertical, or one long strip for webtoons.
- `READ-41` (expected) Fit to height, width or screen, or original size. Single and
  double page, with an offset for the cover and a choice of how to treat wide pages
  (as a spread, or split in two with either half first).
- `READ-42` (expected) Crop borders, rotation lock, background colour, and filters
  that help old scans.
- `READ-43` (later) Panel-by-panel view. It needs panel regions, either detected or
  authored, stored per page.
- `READ-44` (expected) A large archive opens without unpacking all of it.

## Audio

- `READ-50` (expected) Play audiobooks with chapters, a speed setting that keeps
  pitch, skip forward and back by a set number of seconds, and a small rewind when
  playback resumes after a pause.
- `READ-51` (expected) Sleep timer by minutes or to the end of the chapter, with a
  fade out and a way to extend it.
- `READ-52` (expected) Bookmarks and clips at a time, with a note.
- `READ-53` (expected) Speed, skip-silence and volume-boost settings follow the
  same scope chain as text settings.
- `READ-54` (later) Lock-screen, car and watch controls on mobile.
- `READ-55` (later) Switch between the text and the audio of one work at the same
  place, using the alignment map in [content types](02-content-types.md).
- `READ-56` (later) Read along: the text highlights in time with the audio, from
  EPUB media overlays or an alignment the app made.

## Read aloud

- `READ-60` (expected) Read the text aloud with the platform's voices, highlighting
  the current sentence. Speed and voice are settable per language, and playback
  resumes from the reading position.
- `READ-61` (expected) EPUB media overlays play with their highlighting, and fall
  back to synthesised speech where a book has none.
- `READ-62` (later) Pronunciation corrections per book or per language.
- `READ-63` (later) Voices that run on the device, installed by the user. The
  common open engine is GPL, so it cannot be built into the app.

## Access

EU law has required e-book reading software to be accessible since 28 June 2025. The
points below follow its Annex I and the EPUB Accessibility specification.

- `READ-70` (core) Every control has a name, role and state for screen readers on
  each platform: VoiceOver, TalkBack, NVDA, JAWS and Orca. The reading position and
  focus are announced. Screen readers can read the book continuously across pages
  and chapters without announcing page counters.
- `READ-71` (core) Everything works from the keyboard, with a visible focus and a
  documented list of shortcuts.
- `READ-72` (core) Text scales without clipping. High-contrast themes exist. The
  system's reduced-motion setting turns off page animations.
- `READ-73` (core) The book's accessibility metadata is shown in plain words and
  can be searched and filtered: has image descriptions, no flashing hazard,
  conformance level, who certified it.
- `READ-74` (core) Protection on a file never disables text-to-speech or screen
  reader access.
- `READ-75` (expected) Fonts and spacing that help dyslexic readers.
- `READ-76` (core) The app does not profile a user from their accessibility
  settings or reading habits.

## What the app must store

- Reader settings at each scope: global, library, series, book, device. Named
  themes. User fonts as files.
- Separate default settings for reflowable text, fixed layout, PDF, comics and
  audio.
- Per book and device: the last position and when (see
  [annotations](05-annotations-history.md) for its form), the view mode and zoom,
  and the per-book permissions for scripts and network.
- A navigation history for "go back".
- Vocabulary entries: word, stem, sentence, book, time looked up, dictionary used,
  review state.
- Installed dictionaries and voices per language, and pronunciation overrides.
- Panel regions per comic page when they exist.
- Player settings and bookmarks or clips with a time, an end time and a note.
- Generated entity lists with the positions they cover.

## Not verified

Apple's own pages on per-book settings, PDF markup and audiobook speed range could
not be read. Kindle's "page flip" and Kobo's typography controls are described from
reviews. Colour e-ink considerations were not found in any source.

## Sources

EPUB Reading Systems 3.3, EPUB Accessibility 1.1, the W3C mapping of the European
Accessibility Act to EPUB, the W3C Japanese layout requirements, Apple's iPhone and
Mac user guides, Google Play Books help, Kobo help, the KOReader user guide, and the
Kavita and Mihon reader documentation.
