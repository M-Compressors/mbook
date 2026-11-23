# Changelog

## Unreleased

### Added

- The application runs on Android. `crates/mbook-android` is its shell, and
  `crates/mbook-android/android.sh` builds an APK for arm64 with the tools of
  the Android SDK. The release workflow builds and signs one for each
  release, as `mbook-vX.Y.Z-android-arm64.apk`; continuous integration lints
  the Android build and keeps a debug-signed APK of every run. On Android
  the interface runs on gpui-pre with the platform of gpui-mobile, pinned to
  a commit of longbridge/gpui-mobile#56; the desktop stays on GPUI Fast.
- On Android, books and pictures are chosen with the document picker of the
  system, and **Export** sends the converted file to the share sheet, from
  which it can be saved to a folder. **Share** works as on the desktop.

- The reader goes to the next chapter with a swipe to the left over a
  scrolling text, and to the previous one with a swipe to the right, with a
  finger or two on a trackpad.
- An announcement is dismissed with a tap or a sideways swipe, since a touch
  screen never shows its close button.
- The application registers itself for the book formats it reads. Finder,
  Explorer and the file managers and apps of Android offer it for an mbook
  file, which it owns, and under **Open with** for EPUB, FB2, PDF, comic
  archives, Word, HTML, Markdown and text. A file opened with it, or named on
  its command line, is added to the library, and one file opens its book.
  macOS reads the formats from the bundle, Android from the manifest, and on
  Windows the application writes them to the registry of the current user
  when it starts.

### Changed

- Views keep clear of the system bars that gpui-mobile measures on Android,
  in addition to the insets GPUI reports. On Android the bar at the top of a
  page has no window buttons.
- A scrolling text runs to the bottom of the reader, under a footer that
  stays; the text fades into the page behind its buttons and status. A paged text
  keeps the room of its footer. Hidden controls come back when the text is
  scrolled back and at the end of a chapter.
- `mbook_ui::run` takes the files to open as a third argument.
- In a compact window, the editor's list of sections and chapters opens as a
  sheet from a menu button beside Back, and Back leaves the editor. Back used
  to open the list first.
- Notifications are as wide as the window allows, up to their own width, and
  start below the system bars of a phone.

### Fixed

- A Linux build of `mbook-ui` with its default features compiled with a
  warning about an unused variant of the updater; continuous integration now
  lints that build.
- On Android, **Share** and **Export** showed no share sheet. They now go
  through the activity of `mbook-android`, which lends the file with a
  read-only content provider and no longer leaves a copy in
  `Download/Robius Share/`.
- A tap or a sideways swipe on a notification over the reader turned the
  page or changed the chapter as well.
- On Android, **Show the file** in the book menu did nothing; it is left
  out there, since no file manager can see the library of the application.

## [v0.3.2](https://github.com/M-Compressors/mbook/releases/tag/v0.3.2) - 2026-11-23

### Added

- The book menu of the library has **Share**. It opens the share sheet of
  the system with the mbook file of the book, named after its title, so it
  can be sent through AirDrop, Messages, Mail, a messenger or a social
  network. It uses the `robius-share` crate.
- The reader uses the landmarks of a book. A book read for the first time
  opens at its `start` landmark instead of at its first page. The contents
  panel lists the landmarks under **Go to**, above the entries, and choosing
  one goes there; Escape comes back.

### Changed

- The editor's Landmarks section handles every kind of landmark, not only
  `notes`. It opens with what landmarks give a reader. Where the text
  begins has a block of its own that says whether it is marked. The other
  places are listed by the name a reader is shown, and **Add a landmark…**
  chooses the kind from a menu grouped by what a reader looks for, with a
  line that says what the choice gives. A new landmark is put in the order
  of the book.

## [v0.3.1](https://github.com/M-Compressors/mbook/releases/tag/v0.3.1) - 2026-11-23

### Added

- The reader opens a picture, a diagram or a formula that stands alone in
  its paragraph over the whole window when it is clicked or tapped. The
  viewer fits it to the window and zooms it with buttons, `+`, `-` and `0`,
  a pinch, or the wheel with Command or Control held; an enlarged picture
  is moved by dragging or scrolling. A picture is decoded again at its own
  size for the viewer.
- `mbook_converter::figure`, with the `export` feature, draws a Mermaid
  diagram or a KaTeX formula as SVG with its size and baseline, in the
  colours of a page. `mbook-ui` draws the figures of its pages with it; the
  renderer moved there from the application.

### Changed

- The documentation is one site with two kinds of document. RFC 0001 and
  its decision records specify the format and nothing else. RFC 0002,
  RFC 0003 and RFC 0004 are retired: what they said about `mbook`,
  `mbook-converter` and `mbook-ui` is now a section of pages per crate,
  with an overview, a guide, reference pages and the design. The pages of a
  crate describe the code and are not normative.
- The book moved from `rfcs/` to `docs/`, with the format under
  `docs/src/rfcs/`, and is built with `mdbook build docs`. The addresses the
  site had before lead to the new pages.
- The README of each crate holds more examples and a summary of its pages:
  the two types, limits and features of `mbook`; the formats, the losses a
  conversion reports and the defaults of `mbook-converter`; the screens,
  keys and storage of `mbook-ui`.
- The site shows the icon of the application, links every page to its
  source for editing, and gives tables the width of the column.

### Fixed

- Export writes the diagrams and formulas of a book as PNG pictures in
  EPUB, DOCX, FictionBook, PDF, comic archives, HTML and Markdown instead
  of their alternative text. A formula alone in its paragraph is a display
  formula, one inside a line sits on the baseline of the line in EPUB, HTML
  and Markdown, and a figure that cannot be drawn is still written as its
  alternative text.
- The reading example of the README compiles: it kept the iterator of
  chapters borrowed while it read a chapter.
- The pages of `mbook-ui` no longer list conversion from mbook among the
  non-goals, nor export among the future possibilities: the application
  exports since v0.3.0.

## [v0.3.0](https://github.com/M-Compressors/mbook/releases/tag/v0.3.0) - 2026-11-23

- Export a saved book from the library’s book menu as EPUB, DOCX, PDF, FB2,
  CBZ, HTML, Markdown or plain text.
- `mbook-converter` exposes `Converter::export` for an existing `Book` and
  `Format::extension` for suggesting an output file name, both behind the
  optional `export` feature. The application enables that feature.

- Paged reading keeps the final page aligned with its starting block, so a
  short final page shows the whole illustration without carrying over a
  strip of the preceding page.

- The reader defaults to scrolling, with optional pagination, five page tones,
  bundled Literata and Noto Sans, and locally saved controls for enlarged text,
  column width, paragraph spacing, motion and reading progress. It keeps the
  saved chapter and source position when appearance changes or is reset.
- Find highlights the current match when the Markdown renderer can map its
  source span. Closing Find returns to the passage where it opened. Footnote
  references open their definitions and offer a return to the prior passage.

- The reader searches all chapters of the book, and the editor searches the
  current chapter including unsaved edits. Open search with the magnifying
  glass, Command+F on macOS or Control+F on Windows and Linux.
- The release workflow publishes `mbook` and `mbook-converter` to crates.io
  through Trusted Publishing before building the application installers.
- A manual release run uses the selected Git tag as its ref.
- The update banner sits below the title bar, keeping macOS window controls,
  the update message, and the update buttons clear of one another.

## [v0.2.0](https://github.com/M-Compressors/mbook/releases/tag/v0.2.0) - 2026-11-23

### Added

- Format 1.0 stores diagrams and formulas as Mermaid and KaTeX source.
  Chapters refer to them using Markdown image syntax, with alternative text
  for readers that cannot render them. `mbook` reads, writes and edits both
  resource kinds.
- The reader and chapter Preview render diagrams and formulas locally. A
  formula alone in a paragraph is set as a display formula. A formula in
  running text sits on the baseline.
- The editor has **Diagrams** and **Formulas** sections with source editing,
  Mermaid and LaTeX syntax highlighting, preview and removal. The Visual
  toolbar inserts existing resources at the caret. Leaving the alternative
  text field empty uses **Diagram** or **Formula** as the description.

- The book editor's Landmarks section can add, edit and remove `notes`
  landmarks with a chapter, an optional anchor and an optional title.
- The book editor has a Contents section. It shows the contents a reader is
  shown, says whether they follow the headings or are a list the book keeps,
  and opens the chapter of an entry.
- A contents list the book keeps is edited entry by entry in the Contents
  section: an entry gets another title or place, moves one level in or out
  with the entries under it, or is removed, and **Add an entry…** adds one.
  The editor keeps the list in the order of the book. Entries that lead
  nowhere or are out of order are marked, and one control sorts the list.
  The Problems section has **Open Contents** beside the control that
  rebuilds the contents.
- The entries of a contents list the book keeps follow their chapter: they
  move with it in the reading order, when the list was in the order of the
  book, keep leading to it when its file is renamed, and are removed with
  it. The question before a chapter is removed says how many entries go
  with it. Each of the three used to block the save.
- Landmarks follow their chapter in the same way: they keep leading to it
  when its file is renamed and are removed with it, and the question counts
  them.
- **Picture** in a chapter opens one dialog: choose a picture of the book by
  its thumbnail, or add one from a file, describe it, and it is put where
  the caret is. Picture files can also be dropped on a chapter.
- **Link** works without a selection: the dialog asks for the words too. A
  link can lead to a chapter or to a heading inside it, chosen by title, and
  is removed from the same dialog.

### Changed

- The editor lists chapters by number and title instead of by path, under
  the sections about the whole book. A compact window shows both in one
  list.
- **New chapter…** asks for a title, which becomes the first heading of the
  chapter, instead of a path.
- The views of a chapter are named in words. Moving, renaming and removing
  a chapter are in a menu at the end of the chapter bar.
- The Visual toolbar is one row. The control that removed a link is now in
  the link dialog.
- A notes landmark is given its place by choosing a chapter and a heading,
  not by typing an anchor name.

### Fixed

- Saving a book with an empty contents list builds it from chapter headings.
  The editor remembers the automatic mode when the book is reopened.

## [v0.1.0](https://github.com/M-Compressors/mbook/releases/tag/v0.1.0) - 2026-11-22

### Changed

- Open the first chapter in Visual when editing a book, including in
  compact windows.
- Update the native file-picker dependency to `rfd` 0.18 and use
  `similar` 3 for Markdown block comparisons.
- The reader shows a page number for the whole book and marks estimates
  while chapters are still unmeasured. A position slider previews the
  destination and seeks on release. The slider appears when the pointer
  reaches the bottom strip, while seeking, or when it has keyboard focus.
- The default reading text size is 20 px. Saved text sizes still apply.
- The mbook reader lays out chapters as individual pages, with an animated
  turn, a persistent page number, and progress that fades with the controls.
  The appearance panel uses page-tone swatches and icon steppers, and books
  fall back to installed serif fonts.
- The library gives cover art and reading status more space. The editor uses
  a segmented icon picker for its chapter views and draws its preview in the
  selected page tone.

- A dependency that only one crate uses is declared in that crate's own
  manifest; the manifest of the workspace keeps what several crates share.
- Continuous integration installs the libraries GPUI links on Linux, and
  checks the minimum Rust version of the libraries without `mbook-ui`,
  which declares its own.
- The library crate `mbook-core` is renamed `mbook`: the package, the
  folder `crates/mbook/`, the import path `mbook::` and RFC 0002
  (`0002-mbook-api.md`). `mbook-converter` re-exports it as
  `mbook_converter::mbook`.

### Fixed

- Install NASM for the x86 AVIF decoder in Linux CI and in the macOS and
  Windows x64 release builds.
- Compile the update installer only on platforms that use it, with its unit
  tests still available on Linux.
- Keep GPUI Kit's window buttons outside the headless accessibility
  assertions for application controls.
- Fit tall pictures within a reader page while keeping their proportions.
- Command+S is available through File > Save on macOS. Save includes
  pending edits in the Details form.
- Enter in Visual starts a new paragraph without showing Markdown's blank
  separator line.
- The native book, picture, and cover pickers filter to supported file
  extensions.

- The shelf hover highlight has padding around the cover and reading status.
- AVIF pictures decode on a dedicated stack, so opening an illustrated
  chapter works on macOS background queues.

### Added

- Offer a chapter picker in the Visual link dialog and a chapter picture
  preview in its toolbar. The Pictures section displays a thumbnail gallery. Picture
  and cover import offer quality and resolution controls before encoding.
- Visual uses `gpui-kit-markdown-editor` with editable GFM tables, nested
  lists and task items, link editing, picture insertion, and fenced code
  blocks with a language picker.
- A Visual chapter editor with formatting controls and undo and redo.
  Source and Split modes highlight Markdown syntax.
- **New book…** in the library creates a book and opens its empty first
  chapter in the Markdown editor.
- RFC 0004, the design of `mbook-ui`: a reading application with a library,
  a reader and an editor, built with GPUI Kit, usable without a pointer and
  with a screen reader.
- `mbook-ui`, a first version of the application of RFC 0004. The package
  requires Rust 1.100 and is not published to crates.io.
  - A library kept under the data directory of the platform, with a `redb`
    catalogue that is rebuilt from the books when it is lost or damaged.
  - Opening a file adds it: an mbook file is copied, a file in any format of
    `mbook-converter` is converted first. A book the library has is not
    added twice.
  - A reader that takes one chapter and the pictures on screen from the
    file, keeps the place a book was left at, and has page tones, a choice of
    typeface and size, and a table of contents.
  - An editor for the metadata, the cover, the chapters as Markdown with a
    preview, and the pictures; a picture that is not AVIF is encoded with the
    `image` crate.
  - Updates from the releases of this repository through `self_update`,
    verified against a key built into the application.
  - The Cargo feature `visual` and the test of the same name render every
    screen into `target/visual/` without a window.
- `.github/workflows/release.yml`: builds the installers of the application
  for a tag, signed, when a maintainer starts it. It has not been run.
- `mbook`: tests that count the bytes a reader takes from a large book. To
  open it takes less than a twentieth of the file; one chapter, less than
  two thirty-seconds.
- RFC 0001, the mbook file format, version 1.0, with seven decision records.
- RFC 0002, the API of `mbook`.
- RFC 0001 and ADR-0010: a book can list its notable places as *landmarks*,
  each with a kind from a fixed vocabulary (`start`, `glossary`, `index`,
  …), an optional title and a place named like the target of a link. They
  are stored in an optional table, a resource of the new `kind` 4 placed
  after the five fixed ones.
- `mbook`: `Book::landmarks` and `Book::set_landmarks` with the new
  `Landmark`, `BookReader::read_landmarks` with `LandmarkEntry`,
  `ResourceKind::Landmarks`, `Error::InvalidLandmark` and
  `ProblemKind::DanglingLandmark`. A book without landmarks is written to
  the same bytes as before.
- `mbook-converter`: the landmarks of an EPUB 3 book, and the guide of an
  EPUB 2 book, become the landmarks of the converted book.
- RFC 0003, the API of `mbook-converter`.
- `mbook-converter`, the crate of RFC 0003: `Converter` turns a book given as
  any `Read` or as a byte slice into an mbook file or into a `Book`.
  - Source formats, one Cargo feature each and all on by default: EPUB, FB2
    (plain or inside a ZIP), PDF (a scan becomes one picture per page), comic
    book archives, Word documents, HTML, Markdown and plain text. The format
    is detected from the content.
  - Pictures in JPEG, PNG, GIF, WebP and BMP are encoded as AVIF through the
    `image` crate; AVIF pictures are kept as they are.
  - Links inside a book keep their target down to the block, through anchor
    definitions; the contents of the source become the contents of the book.
  - Whatever cannot be kept is reported as a `Warning`.
- `mbook`, the library of RFC 0002:
  - `BookReader` opens a file from anything that can seek, checks its tables
    against RFC 0001, and decodes chapters and pictures on demand; it
    resolves link and picture destinations.
  - `Book` is the in-memory model: `Book::read` and `BookReader::load` build
    it from a file, `Book::new` from nothing; chapters and pictures are
    added, moved and removed by path.
  - `Book::check` reports every broken link, missing picture, piece of raw
    HTML and unresolved contents item; `Book::write` and `Book::save` refuse
    an invalid book, and `save` replaces a file atomically.
  - Anchors and the table of contents are computed from the headings, with
    names by GitHub's rule; picture sizes are read from the AVIF container.
- Workspace lints, CI, and the repository's policy documents.

- RFC 0001, ADR-0008 and ADR-0009: a chapter can name lines that are not
  headings, with an anchor definition in front of the line, `[fig-1]: #fig-1`.
  The names are stored in an anchor table of the chapter, a resource of the
  new `kind` 3 placed directly before it, and are reached by a link fragment
  like the anchor of a heading.
- `mbook`: a `Book` reads the anchor definitions of a chapter from its
  text, whenever the text is given or replaced, and shows them as
  `Chapter::extra_anchors`; `BookReader::read_extra_anchors` reads them from
  a file, and `BookReader::read_resolve` follows a link to one. A book
  without anchor definitions is written to the same bytes as before, so the
  checked-in example file is unchanged.

### Changed

- `mbook`: `BookReader::resolve` returns the new
  `Destination::ExtraAnchor` for a fragment that is not the anchor of a
  heading when the target chapter has an anchor table. `ResourceKind` has
  the variant `ChapterAnchors`, and `Error` the variant `Unresolved`.

- RFC 0001: a `toc` row with a non-zero offset must match an `anchors` row,
  so that a contents target can always be written as a link destination.
- RFC 0001: a heading whose anchor name would be empty gets no anchor row;
  and the part of a valid position that needs the text of a chapter is
  checked when that chapter is decoded.
