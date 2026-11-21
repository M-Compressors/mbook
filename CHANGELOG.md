# Changelog

## Unreleased

### Added

- RFC 0001, the mbook file format, version 1.0, with seven decision records.
- RFC 0002, the API of `mbook-core`.
- RFC 0001 and ADR-0010: a book can list its notable places as *landmarks*,
  each with a kind from a fixed vocabulary (`start`, `glossary`, `index`,
  …), an optional title and a place named like the target of a link. They
  are stored in an optional table, a resource of the new `kind` 4 placed
  after the five fixed ones.
- `mbook-core`: `Book::landmarks` and `Book::set_landmarks` with the new
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
- `mbook-core`, the library of RFC 0002:
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
- `mbook-core`: a `Book` reads the anchor definitions of a chapter from its
  text, whenever the text is given or replaced, and shows them as
  `Chapter::extra_anchors`; `BookReader::read_extra_anchors` reads them from
  a file, and `BookReader::read_resolve` follows a link to one. A book
  without anchor definitions is written to the same bytes as before, so the
  checked-in example file is unchanged.

### Changed

- `mbook-core`: `BookReader::resolve` returns the new
  `Destination::ExtraAnchor` for a fragment that is not the anchor of a
  heading when the target chapter has an anchor table. `ResourceKind` has
  the variant `ChapterAnchors`, and `Error` the variant `Unresolved`.

- RFC 0001: a `toc` row with a non-zero offset must match an `anchors` row,
  so that a contents target can always be written as a link destination.
- RFC 0001: a heading whose anchor name would be empty gets no anchor row;
  and the part of a valid position that needs the text of a chapter is
  checked when that chapter is decoded.
