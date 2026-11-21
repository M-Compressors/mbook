# Changelog

## Unreleased

### Changed

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

- The native book, picture, and cover pickers filter to supported file
  extensions.

- The shelf hover highlight has padding around the cover and reading status.
- AVIF pictures decode on a dedicated stack, so opening an illustrated
  chapter works on macOS background queues.

### Added

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
