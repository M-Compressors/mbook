# Changelog

## Unreleased

### Added

- RFC 0001, the mbook file format, version 1.0, with seven decision records.
- RFC 0002, the API of `mbook-core`.
- RFC 0003, a draft of the API of `mbook-converter`, a crate that converts
  EPUB, FB2, PDF and other formats into mbook. The crate is not implemented.
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

- RFC 0001 and ADR-0008: a chapter can name lines that are not headings. The
  names are stored in an anchor table of the chapter, a resource of the new
  `kind` 3 placed directly before it, and are reached by a link fragment like
  the anchor of a heading.
- `mbook-core`: `Chapter::add_anchor`, `remove_anchor` and `extra_anchors`
  give such names to a chapter of a `Book`; `BookReader::read_extra_anchors`
  reads them, and `BookReader::read_resolve` follows a link to one. A book
  without extra anchors is written to the same bytes as before, so the
  checked-in example file is unchanged.

### Changed

- `mbook-core`: `BookReader::resolve` returns the new
  `Destination::ExtraAnchor` for a fragment that is not the anchor of a
  heading when the target chapter has an anchor table. `Chapter::set_text`
  removes the extra anchors of the chapter. `ResourceKind` has the variant
  `ChapterAnchors`, and `Error` the variants `InvalidAnchor` and
  `Unresolved`.

- RFC 0001: a `toc` row with a non-zero offset must match an `anchors` row,
  so that a contents target can always be written as a link destination.
- RFC 0001: a heading whose anchor name would be empty gets no anchor row;
  and the part of a valid position that needs the text of a chapter is
  checked when that chapter is decoded.
