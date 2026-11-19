# Interface design: a library in each platform's own style

For [the interface guidelines issue](https://github.com/endqwerty/ebook-manager/issues/91).
Read this page before you design or build a screen in any shell. It covers the
library window and book details. The reader gets its own page when it is designed,
and [its section here](#the-reader) only sets the direction.

Someone opening the app should feel they are in a library: quiet, ordered and
warm, with the books as the thing they notice. The app should also look like it
came with their system. Both hold when the work is split in two.

- **The platform owns the frame.** Bars, sidebars, menus, sheets, controls,
  materials, the UI font and the accent colour are the system's standard parts,
  unmodified. On a Mac, iPhone and iPad that is Liquid Glass. On Windows 11 it is
  Mica and the WinUI controls. On GNOME it is libadwaita. On Android it is
  Material 3.
- **This page owns the content.** How covers are drawn, where things sit, what
  lines up with what, where a serif appears, which shadows exist and how a book
  opens. The library feel lives here and is the same on every platform.

Two checks decide whether a screen follows this page. Hide every cover, and the
window should pass for one of the system's own apps. Show the covers, and nothing
other than the books should hold the eye.

![The library window, cover states and book details drawn to these rules](interface-sample-light.png)

The picture shows the content rules on plain surfaces. It is not a mock-up of any
platform's frame. [`interface-sample.html`](interface-sample.html) draws it, and
there is [a dark version](interface-sample-dark.png).

## What makes it feel like a library

Four things, each of which you can measure on a screen.

| Quality | Where it comes from |
|---|---|
| Order | Covers stand on one line per row. Every screen has one leading edge. Captions keep a fixed height. Numbers line up in columns. |
| Quiet | One prominent action per screen. No badges, banners or colour that ask for attention. Nothing moves unless the reader caused it. |
| Warmth | A serif for the names of things. Covers drawn as objects with weight. Paper tones in the reader. |
| Books first | The frame takes its colour from the system. The only strong colour in the window is on the covers. |

Things the app never draws: wood, leather, cloth or paper textures, a shelf under
the books, page curls, gilt, drop caps or ornaments in the interface, and
book-shaped custom controls. They date the app and fight the platform's materials.

## Rules for the content layer

Lengths are in the platform's own unit: points on Apple systems, effective pixels
on Windows, CSS pixels in GTK and density-independent pixels on Android.

### Placement

- The sidebar lists places, in this order: Library (All books, Reading now,
  Unread, Finished), then Collections, then Devices. A count sits at the trailing
  edge of each row.
- The screen's title sits at the top of the content at its leading edge, with the
  number of books beside it on the same baseline. It names the place selected in
  the sidebar.
- Search and Add go where the platform puts search and the primary action. See
  [the platform table](#how-each-rule-maps-to-a-platform).
- A screen has one prominent action. On the library it is Add. On book details it
  is Continue reading, or Read for a book not yet started. Every other action is
  plain, in a menu or in the context menu.
- Book details put the cover at the leading edge and the facts beside it. On a
  narrow window the cover moves above the facts and stays at the leading edge.
- Status that needs no decision, such as a finished import, uses the platform's
  transient message. It never covers a book cover.

### Alignment

- One leading edge per screen. The title, section headers, the first cover in each
  row and the first line of an empty state start at the same x position.
- Covers in a row share their bottom edge. Covers have different proportions, so
  their tops differ, as books of different heights do on a shelf. Never crop or
  stretch a cover to a common shape. Scale it to the column width and let the row
  take the height of its tallest cover.
- The caption starts at the cover's leading edge. It is never centred under the
  cover. The title takes up to two lines and the author one. The caption block
  keeps the height of three lines even when the title needs one, so the next row
  starts level.
- Truncate at the end with an ellipsis. Wherever a title is truncated, the full
  title is available in a tooltip on pointer platforms and in book details
  everywhere.
- In a table, text columns align to the leading edge and number columns to the
  trailing edge. Dates count as text.
- In book details the labels align to the trailing edge of their column and the
  values to the leading edge of theirs, so the two meet at one line.

### Type

- The platform's UI font sets everything except the roles below. Use its named
  text styles. Never set a font size by hand, because named styles follow the
  user's text size setting and fixed sizes do not.
- A serif sets three things: the screen title, the book's title in book details,
  and the headline of an empty state. Nowhere else in the library window. Captions
  under covers, sidebar rows, buttons, menus and table cells stay in the UI font.
- The serif is never smaller than the platform's title style, about 17 units.
  Small serif text renders poorly on low-density Windows and Linux displays.
- The serif is semibold in titles. No italics, no small capitals and no all-capital
  labels anywhere in the interface.
- Counts, sizes, dates in tables and anything that updates in place use tabular
  figures so digits do not shift.
- Capital letters follow each platform's own rule, so a string can differ between
  shells. See [the platform table](#how-each-rule-maps-to-a-platform).

### Colour

- Window, sidebar, bar and control colours are the platform's semantic colours.
  The app defines none of its own for them.
- The accent is the user's system accent, or the wallpaper-derived scheme on
  Android. The app does not set a brand accent. The accent appears on the
  prominent action, the selection and focus. It does not appear on text, progress
  marks or decoration.
- The app's own colours are the placeholder cover colours below and the app icon.
- State never depends on colour alone. A book being read has a progress line, a
  finished book has a check mark after the author's name, and both are announced
  to assistive technology in words.

### Covers

A cover is the one thing in the window drawn as a physical object.

| Property | Value |
|---|---|
| Shape | The image's own proportions. Corners of 2 units, 3 when the cover is 200 units wide or more. The platform's control radius never applies to a cover. |
| Edge | A 1 unit line inside the edge, white at 9% in dark appearance and black at 8% in light, so a cover the colour of the window keeps its outline. |
| Spine | A shade over the leading 5% of the width, black at 14% fading to nothing. Leave it out when the cover is under 64 units wide. |
| Widths | Small 96, medium 128, large 160. The grid fills the row with equal columns no narrower than the chosen width. |
| Gaps | 20 between columns, never under 16. 8 from cover to caption. 28 from the caption block to the next row. The larger gap between rows makes each row read as a shelf. |
| Progress | A line 2 units thick under the caption, as wide as the cover, in the secondary label colour. Present only while a book is partly read. |
| Loading | A block in the cover's average colour until the image is decoded, then a cross-fade. A cover never pops in and never shows a spinner. |

A book with no cover image gets a plain binding set in type: a flat colour, the
title in the serif at the top, a hairline rule, and the author in small UI type at
the bottom. Pick the colour from this list by a stable hash of the book's
identifier, so a book keeps its colour on every device.

| Name | Colour | Text | Contrast |
|---|---|---|---|
| Navy | `#24405c` | `#f1e6cc` | 8.6:1 |
| Oxblood | `#6f2a2a` | `#f1e6cc` | 8.3:1 |
| Forest | `#315047` | `#f1e6cc` | 7.1:1 |
| Umber | `#7a3b22` | `#f1e6cc` | 6.8:1 |
| Plum | `#51446b` | `#f1e6cc` | 7.1:1 |
| Ochre | `#b98a3c` | `#23190a` | 5.6:1 |
| Charcoal | `#2a2b2f` | `#f1e6cc` | 11.4:1 |
| Moss | `#2f3a2b` | `#f1e6cc` | 9.6:1 |

### Shadows and depth

- Covers cast a shadow. So do the platform's own floating parts, which bring
  their own. Nothing else the app draws has a shadow: no cards, panels or rows.
- A cover's shadow has two layers. A contact shadow sits 1 unit below with a
  2 unit blur. An ambient shadow sits 6 units below with a 14 unit blur, pulled in
  by 4 units so it stays under the cover.
- In light appearance both layers are black at 16%. In dark appearance they are
  black at 28%, and the inner edge line does most of the work because a shadow
  hardly shows on a dark window.
- Light comes from above. Shadows fall straight down and never to the side.
- Group content with space and alignment first, a hairline second. Boxes are for
  the platform's own grouped lists.

### Motion

- Every animation answers something the reader did. Nothing loops, pulses or
  plays on arrival. Rows do not animate in when a screen opens.
- Pointer over a cover: it rises 3 units and its shadow lengthens, the contact
  layer to 2 units below with a 3 unit blur and the ambient layer to 14 below with
  a 22 unit blur. It does not scale. Use the platform's short control duration.
- Press: the cover settles back to rest. Release opens the book or selects it,
  as the platform's collections do.
- Opening a book or its details: the cover travels from its place in the grid to
  its place in the destination, using the platform's shared-element transition.
  Closing reverses it to the same cover. If the cover has scrolled out of view,
  cross-fade instead.
- Sorting, filtering and resizing move covers to their new places with the
  platform's standard layout animation. With more than about 200 covers changing
  place, cross-fade the grid instead.
- Use the platform's named durations and springs. Do not invent timing values.
- When the system asks for less motion, replace the rise, the shared-element
  travel and the layout moves with a cross-fade or nothing.

### Words

- Plain words in the reader's language: Library, Collection, Book, Author,
  Series, Add, Read. No bookish vocabulary such as tome, shelf-mate or bibliophile.
- An empty state has a serif headline of a few words, one sentence in the UI font
  saying what to do, and one action. For example "No books yet", "Add EPUB files
  or a folder to start your library.", Add.
- No exclamation marks. No praise for routine actions.

### Settings that change the look

[READ-70 to READ-72](../requirements/04-reading.md#access) require these to work.
The platform's standard parts follow them without help. The covers, the serif
roles and the motion above are drawn by the app, so the app must follow them.

| Setting | What the content layer does |
|---|---|
| Larger text | The serif roles and captions scale with it. When captions grow past three lines' original height, the grid drops a column before it truncates more. |
| More contrast | The cover's edge line becomes the platform's high-contrast border colour at full strength. The spine shade and both shadows are removed. |
| Less transparency | Nothing in the content layer is translucent, so nothing changes. |
| Less motion | As described under [Motion](#motion). |
| Bold text | The serif roles step up one weight, as the UI font does. |

## How each rule maps to a platform

| Rule | Mac | iPhone and iPad | Windows 11 | GNOME | Android |
|---|---|---|---|---|---|
| Window structure | `NavigationSplitView` with a sidebar and an inspector for details | Tab bar on iPhone, sidebar on iPad | `NavigationView` at the left under a Mica window | `AdwNavigationSplitView` inside `AdwToolbarView` | `NavigationSuiteScaffold`, with a list-detail layout from the expanded width up |
| Search | Toolbar search field | A search tab, `Tab(role: .search)` | `AutoSuggestBox` in the title bar | Search button in the header bar that reveals a search bar | `SearchBar` in the top app bar |
| Serif | New York, through the system serif design | New York | Sitka | A bundled serif | A bundled serif |
| Grid | AppKit collection view | `LazyVGrid` or a UIKit collection view | `ItemsView` or `GridView` | `GtkGridView` | `LazyVerticalGrid` |
| Table | `Table` | Not offered on iPhone | `ListView` with column headers | `GtkColumnView` | Not offered on phones |
| Pointer over a cover | The rise above | iPad pointer only | The rise above | The rise above | None |
| Opening a book | Zoom from the cover | `navigationTransition(.zoom)` from a `matchedTransitionSource` | Connected animation | The navigation view's push, with a cross-fade of the cover | `sharedBounds` in a `SharedTransitionLayout` |
| Empty state | `ContentUnavailableView` | The same | A centred text block in the content layer | `AdwStatusPage` | A centred text block |
| Transient message | None. Show progress in the toolbar. | The same | `InfoBar` | `AdwToast` | Snackbar |
| Button and menu capitals | Title Case | Title Case | Sentence case | Header Capitalisation | Sentence case |
| Icons | SF Symbols | SF Symbols | Segoe Fluent Icons | Symbolic icons from GTK and the Icon Library | Material Symbols |

### Mac, iPhone and iPad: Liquid Glass

The current systems are version 27, which refined the material introduced in
version 26. An app built on the standard components picks the refinements up
without changes.

- Glass belongs to the layer of navigation and controls that floats above
  content. Covers, captions, table rows and book details are content and never
  get a glass background. Apple says to keep custom glass to a few important
  functional elements and never to put glass on glass.
- Use the standard sidebar, toolbar, tab bar, sheet, popover and menu, and remove
  any custom background behind them. A custom bar background stops the system's
  material and its scroll edge effect from working.
- Content scrolls under the bars. The system's scroll edge effect keeps the bar
  legible, so the grid needs no fade or rule of its own at the top. Use the hard
  edge style above a table's pinned column headers.
- From version 27 the sidebar reaches the window's edges and its icons take the
  accent colour. Leave both alone.
- Tint only the prominent action, with `.buttonStyle(.glassProminent)`. Every
  other toolbar control is untinted glass.
- Let toolbar items group themselves. Separate the view switch and sort from Add
  with a `ToolbarSpacer`.
- On iPhone the tab bar shrinks as the grid scrolls,
  `.tabBarMinimizeBehavior(.onScrollDown)`. If audio playback arrives, its
  controls go in `.tabViewBottomAccessory`.
- Shapes the app adds near the frame, such as an inspector's grouped box, use
  concentric corners (`ConcentricRectangle`). Covers keep their 2 unit corners
  because they are content and not part of the frame.
- At rest no cover should sit half under a floating control. Size the top inset
  so the first row clears the toolbar.
- Use the text styles: Large Title for the screen title and Title 1 for the
  book's title, both with the serif design. On a Mac those are 26 and 22 points.
  The Mac has no Dynamic Type, so the app offers no text size of its own in the
  library.
- Springs are the default animation. Use `.smooth` for the cover's rise and for
  layout moves, and the system's zoom transition for opening.
- The system adapts glass to Reduce Transparency, Increase Contrast and Reduce
  Motion, and to the user's clear-to-tinted glass setting. Test the covers and the
  serif roles under each, because the system does not adapt those.
- The app icon is layered artwork made in Icon Composer, with default, dark, clear
  and tinted appearances.

### Windows 11

The current release is 26H2, with Windows App SDK 2.7.

- The window's base layer is Mica (`MicaBackdrop`). Navigation and commands sit
  on it. The content area is one layer on top, filled with
  `LayerFillColorDefaultBrush`. The grid sits directly on that layer.
- Do not wrap books in cards. Windows gives a card an elevation of 8 with a
  stroke, which would put a box around every cover.
- Acrylic is for menus and flyouts, which bring it themselves. Never use it
  behind the grid.
- Corners are 8 for windows, flyouts and dialogs and 4 for controls. These come
  from `OverlayCornerRadius` and `ControlCornerRadius`. Covers keep 2.
- Use the `TitleBar` control at its tall height of 48, with the search box in it.
  Keep everything clear of the caption buttons.
- `NavigationView` in its automatic mode collapses at 1008 and 640 effective
  pixels. Give the content 24 of margin, 12 in minimal mode.
- Type comes from the Windows ramp: Title, 28 on a 36 line, for the screen title
  and the book's title in Sitka, Body for captions and Caption for secondary
  lines.
- The cover's rise uses `ControlFastAnimationDuration`, 167 ms. Page changes use
  the transition `NavigationView` recommends.
- Colours come from theme resources only, such as `TextFillColorPrimaryBrush` and
  `TextFillColorSecondaryBrush`. Those switch to system colours under a contrast
  theme. A colour written as a number does not.
- Mica and Acrylic fall back to solid colours by themselves when transparency is
  off or a contrast theme is on.

### GNOME

The current releases are GNOME 51 and libadwaita 1.10.

- `AdwNavigationSplitView` holds the sidebar and the content, and collapses to a
  navigation stack on a narrow window. Each side is an `AdwToolbarView` with an
  `AdwHeaderBar` in the flat style, which shows a shadow only when content has
  scrolled under it.
- Book details open in an `AdwOverlaySplitView` utility pane on a wide window and
  as a pushed page on a narrow one.
- The grid is a `GtkGridView` over a list model, placed directly in a
  `GtkScrolledWindow` so it recycles its widgets. Keep per-book state in the
  model.
- Do not put books in `.card` boxes. A card has an outline and two shadows of its
  own.
- Colours come from libadwaita's CSS variables. The app follows the system accent
  and dark style, which is the default behaviour, and sets no accent of its own.
- Use the type classes: `.title-1` for the screen title and the book's title with
  the bundled serif as the family, `.caption` for the author line, `.numeric` for
  counts, and `.dimmed` for secondary text. They are relative sizes and follow
  the user's font setting.
- Write the cover's CSS for `prefers-contrast: more` and for dark appearance.
  Standard widgets handle both. Custom drawing does not.
- Animate with `AdwTimedAnimation` or `AdwSpringAnimation`. They skip themselves
  when animations are turned off. Reduced motion is a separate setting,
  `gtk-interface-reduced-motion`, which the app checks for the cover's rise.
- Support a window 360 wide, which GNOME asks of apps that suit a phone.
- Off GNOME, GTK reads what settings the desktop shares and libadwaita chooses
  light when it learns no preference. The app adds no detection of its own.

### Android

The current releases are Android 17 and Compose Material 3 1.5.

- Colour follows the wallpaper through `dynamicLightColorScheme` and
  `dynamicDarkColorScheme` on Android 12 and later. The fallback scheme is a
  neutral one built from the Forest placeholder colour.
- Covers sit directly on `surface`. Do not wrap them in `Card`. The shape scale,
  where a card is 12, does not apply to a cover.
- Navigation is a bar on a compact width and a rail from medium up, through
  `NavigationSuiteScaffold`. From the expanded width, 840, the library and book
  details show side by side.
- The app draws edge to edge, which Android enforces from version 15. Inset the
  grid with `WindowInsets.safeDrawing`.
- The screen title is `headlineMedium` in a large flexible top app bar that
  collapses as the grid scrolls. The book's title is `headlineSmall`. Both take
  the bundled serif through per-role `TextStyle` in `Typography`. Captions are
  `bodyMedium` and `bodySmall`.
- Motion uses the standard `MotionScheme`, not the expressive one. Its spatial
  springs are damped at 0.9 and do not overshoot. A library does not bounce.
- Leave out the Expressive shape-morphing components and the wavy progress
  indicators.
- Every touch target is at least 48 by 48, including a cover in the small grid
  and its context menu.
- Read the system contrast level from `UiModeManager.getContrast()` for the
  cover's edge line, because the app draws it.

## The reader

The reader is the warmest place in the app and the plainest. The frame hides
while reading and returns on a tap or pointer movement. The page offers white,
paper, grey and black, with paper as the default in light appearance. Text is set
in a serif with a line of 60 to 70 characters. Page turns slide or cross-fade and
never curl. The reader's own design page will set the details.

## Decisions made without asking

Each was the default taken to finish this page. Any can change.

1. **Follow the system accent, with no brand colour in the frame.** The
   alternative is a fixed library green on the prominent action. It was turned
   down because it breaks the first check on this page and fights the user's own
   choice on four of the five platforms.
2. **The serif appears in three roles only.** Setting every book title under
   every cover in a serif would be more bookish, and also noisier and worse on
   low-density displays.
3. **Covers keep their own proportions and share a bottom edge.** Cropping all
   covers to 2:3 gives a tidier grid and is what most stores do. It was turned
   down because it cuts artwork and removes the one cue that reads as a shelf.
4. **A faint spine shade stays.** It is the only drawn hint that a cover is a
   book. Remove it if it reads as decoration on real covers.
5. **Bundle one serif on GNOME and Android.** Neither guarantees a serif. Literata
   is the candidate. It needs the [LIC-1](../requirements/10-constraints.md#licences-of-parts)
   check and an entry in the release manifest before it ships. Using it on every
   platform would make titles identical everywhere, and was turned down because
   New York and Sitka are what those systems' own apps use.
6. **No transient message on Apple platforms.** Their guidelines have no toast.

## Check on a device before relying on it

The research for this page read each platform's documentation. These points were
not confirmed there, or the documentation contradicts itself.

- Whether a native Mac app gets New York from the serif font design. Apple's
  guidelines confirm it for iPhone, iPad and Mac Catalyst.
- Whether the zoom navigation transition is available on the Mac, and each glass
  API's minimum system version.
- The iPhone sizes of the text styles. The Mac sizes above are from Apple's table.
- Whether Sitka picks its optical size from the font size by itself or the app
  must choose between Sitka Text, Heading and Display.
- Whether Mica falls back to a solid colour under Battery Saver. Two Microsoft
  pages disagree.
- How Windows connected animations and the app's own implicit animations respond
  when animation effects are turned off. Microsoft's material points to
  `UISettings.AnimationsEnabled`.
- The exact sizes of libadwaita's type classes. The values seen came from an
  installed 1.10 stylesheet: `.title-1` is 181% at weight 800.
- What accent and fonts libadwaita uses on KDE Plasma and on compositors with no
  settings portal.
- Whether Compose's shared transition APIs are stable in the current release, and
  how Compose animations follow Android's Remove animations setting.
- Whether `titleLarge` is regular or medium weight. Google's guide and the
  library's tokens disagree, so this page avoids that role.

## Sources

Read on 18 November 2026.

- Apple: [Adopting Liquid Glass](https://developer.apple.com/documentation/technologyoverviews/adopting-liquid-glass),
  [Typography](https://developer.apple.com/design/human-interface-guidelines/typography),
  [Accessibility](https://developer.apple.com/design/human-interface-guidelines/accessibility),
  [Meet Liquid Glass](https://developer.apple.com/videos/play/wwdc2025/219/),
  [Build a SwiftUI app with the new design](https://developer.apple.com/videos/play/wwdc2025/323/),
  [the 2026 design update](https://developer.apple.com/videos/play/wwdc2026/102/),
  [Animate with springs](https://developer.apple.com/videos/play/wwdc2023/10156/).
- Microsoft: [Mica](https://learn.microsoft.com/en-us/windows/apps/design/style/mica),
  [materials](https://learn.microsoft.com/en-us/windows/apps/develop/ui/materials),
  [layering and elevation](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/layering),
  [geometry](https://learn.microsoft.com/en-us/windows/apps/design/signature-experiences/geometry),
  [title bar](https://learn.microsoft.com/en-us/windows/apps/develop/title-bar),
  [NavigationView](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/navigationview),
  [list view and grid view](https://learn.microsoft.com/en-us/windows/apps/develop/ui/controls/listview-and-gridview),
  [Windows 11 font list](https://learn.microsoft.com/en-us/typography/fonts/windows_11_font_list),
  [Fluent typography](https://fluent2.microsoft.design/typography),
  [WinUI theme resources](https://github.com/microsoft/microsoft-ui-xaml/blob/main/controls/dev/CommonStyles/Common_themeresources_any.xaml).
- GNOME: [adaptive layouts](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.10/adaptive-layouts.html),
  [styles and appearance](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.10/styles-and-appearance.html),
  [CSS variables](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.10/css-variables.html),
  [toolbar styles](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.10/enum.ToolbarStyle.html),
  [AdwAnimation](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.10/class.Animation.html),
  [GTK list widgets](https://docs.gtk.org/gtk4/section-list-widget.html),
  [reduced motion setting](https://docs.gtk.org/gtk4/property.Settings.gtk-interface-reduced-motion.html),
  and the guidelines on [typography](https://developer.gnome.org/hig/guidelines/typography.html),
  [writing style](https://developer.gnome.org/hig/guidelines/writing-style.html),
  [grid views](https://developer.gnome.org/hig/patterns/containers/grid-views.html)
  and [adaptive design](https://developer.gnome.org/hig/guidelines/adaptive.html).
- Android: [Material 3 in Compose](https://developer.android.com/develop/ui/compose/designsystems/material3),
  [Compose Material 3 releases](https://developer.android.com/jetpack/androidx/releases/compose-material3),
  [window size classes](https://developer.android.com/develop/adaptive-apps/guides/use-window-size-classes),
  [window insets](https://developer.android.com/develop/ui/compose/system/insets),
  [fonts](https://developer.android.com/develop/ui/compose/text/fonts),
  [accessibility](https://developer.android.com/guide/topics/ui/accessibility/apps),
  [UiModeManager](https://developer.android.com/reference/android/app/UiModeManager),
  [MotionScheme source](https://android.googlesource.com/platform/frameworks/support/+/268d841a45644cadf438fc335c793869728449ec/compose/material3/material3/src/commonMain/kotlin/androidx/compose/material3/MotionScheme.kt).
