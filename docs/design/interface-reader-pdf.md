# Interface design: the PDF reader

Part of [the interface design](interface.md) and companion to
[the text reader](interface-reader.md). This page covers reading fixed-layout
PDF documents ([READ-20 to READ-26](../requirements/04-reading.md#pdf) and
[#30](https://github.com/endqwerty/ebook-manager/issues/30)).

## The page

A PDF page is a fixed visual object, like a cover, so it sits on a neutral
surface rather than filling the window edge to edge.

| Part | Value |
|---|---|
| Backdrop | The platform's secondary fill colour in both light and dark appearance. Under More contrast, the platform's window colour. |
| Page object | Drawn whole with the cover's two-layer shadow in light and dark appearance, and a 1 unit outline in the primary text colour with no shadow under More contrast. |
| View modes | Single page (default), Two-page spread (with a Cover alone toggle), and Continuous vertical scroll. |
| Fit modes | Fit page (default on desktop), Fit width (default on phones), and Auto-crop white margins (opt-in fit mode, off by default). |
| Rotation | 90-degree clockwise and counter-clockwise steps, remembered per book. |
| Foot line | Same quiet line as the text reader: the PDF's own page label (including roman numerals when present) and ordinal, such as `iv · Page 4 of 312`. |

## Controls and navigation

- The top and bottom floating bars reuse [the text reader's controls](interface-reader.md#the-controls),
  hiding and revealing by the same rules without shifting or resizing the page.
- The top bar menu offers View mode, Fit mode, Rotate, and Reflowed text view
  (for tagged PDFs).
- The shared leading panel offers Outline (PDF bookmarks), Page thumbnails,
  Bookmarks, Highlights, and Search.
- Text-layer selection and highlights reuse the text reader's selection menu,
  five highlight colours and underline, and note popovers.

## Accessibility

- **Tagged PDFs:** Offer a plain reflowed text view that respects the reader's
  font, size and page-colour settings.
- **Scanned PDFs with no text layer:** State clearly in the document info and
  when search or selection is attempted that the file contains page images with
  no text layer.
- **Reading order:** Expose the PDF structure tree (or page text order when
  untagged) to assistive technology.
- **Zoom:** Keyboard zoom (`+`, `-`, `0`) never traps keyboard focus; Escape or
  Tab always reaches the controls.
