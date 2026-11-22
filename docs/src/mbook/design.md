# Design

What the library is built on, what its design costs, and the choices behind
it. A choice between alternatives is recorded in the table of
[Rationale and alternatives](#rationale-and-alternatives); the choices of the
file format itself are the [decision records](../rfcs/adr/overview.md).

## Dependencies and features

| Feature | Default | Enables | Brings in |
| --- | --- | --- | --- |
| `read` | yes | `BookReader`, `Book::read`, `Book::open` | mopenzl decoding |
| `write` | yes | `Book::new` and every call that changes, checks or saves a `Book` | mopenzl encoding, `markdown`, `github-slugger`, `uuid`, `bytes`, `tempfile` |

`thiserror` and `percent-encoding` are used with either feature.

- At least one of the two must be enabled. With `read` alone a `Book` is a
  fully loaded book that can be looked at but not changed.
- A reading application that builds with `read` only gets no Markdown parser
  from this crate; `resolve` needs none.
- `markdown` 1.0.0 is the parser that
  [ADR-0006](../rfcs/adr/0006-markdown-dialect.md) found to cover the whole dialect
  and to report byte offsets.
- **A mechanism comes from an established crate; only the rules of the
  format are written here.**

  | Need | Crate |
  | --- | --- |
  | Heading and link analysis | `markdown` 1.0.0 |
  | Anchor names by GitHub's rule | `github-slugger` |
  | Percent-decoding of destinations | `percent-encoding` |
  | Reading and writing integers in byte buffers | `bytes` |
  | Saving through a temporary file | `tempfile` |
  | Book identifiers | `uuid` |
  | Error types | `thiserror` |

- **The pixel size of a picture is the one thing read by code of this
  crate**, about 150 lines over `bytes` that walk the AVIF boxes to the
  properties of the primary item. The crates looked at, and what stands in
  the way of each:

  | Crate | Why it does not fit |
  | --- | --- |
  | `image` 0.25 | AVIF needs the C library libdav1d; the picture is decoded in full to learn its size; the size is that of the decoded frame, before crop and rotation. |
  | `imagesize` 0.15 | Takes the largest `ispe` box of the file: ignores the crop and which item a property belongs to. |
  | `avif-parse` 2 | Needs Rust 1.90, above the minimum of this crate, and is MPL-2.0. |
  | `zenavif-parse` 0.6 | Gives the right size on the test files, but is MPL-2.0 and brings about 25 crates for three properties. The format author decided on 2026-11-21 to keep the code of this crate. |

  No image decoder is linked.

## Required from mopenzl

`BookReader` needs mopenzl 0.2.6 or later. Earlier versions offered random
access only through `FramedSeekReader<'a, R>`, which borrows its decoder, so
one struct could not hold the decoder, the source and the directory.
Version 0.2.6 adds a directory that owns its data and two calls on the
decoder:

```rust
/// A validated directory, detached from the source and the decoder.
pub struct FramedIndex { /* base offset, entries */ }
impl Deref for FramedIndex { type Target = [ResourceInfo]; }

impl FramedDecompressor {
    /// Reads the header, the footer and the directory; decodes nothing.
    pub fn framed_index<R: Read + Seek>(&self, source: R)
        -> Result<FramedIndex, FramedError>;

    /// Seeks to one record, reads it and decodes it.
    pub fn read_indexed_resource<R: Read + Seek>(
        &mut self, source: R, index: &FramedIndex, resource: usize,
    ) -> Result<DecodedResource, FramedError>;
}
```

`BookReader` holds a `FramedDecompressor`, a `FramedIndex` and the source as
three plain fields.

## Drawbacks

- **Two types to learn.** A program that reads and then edits meets both, and
  resources are addressed by number in one and by path in the other.
- **Editing loads the whole book.** Changing the title of a 2 GB comic decodes
  2 GB into memory and writes 2 GB back. This cost is accepted.
- **Unchanged chapters are compressed again on save.** Pictures are stored, so
  they cost a copy and a checksum; text costs a real compression pass.
- **The write side carries a Markdown parser**, although the crate renders
  nothing.
- **Two calls resolve a link.** `resolve` cannot say where a fragment leads
  when it is not a heading's, and a reading application has to handle
  `Destination::ExtraAnchor`.
- **`buffered` waits for the end of the stream** before anything can be shown,
  because the directory is at the end of the file.

## Rationale and alternatives

| Decision | Chosen | Rejected | Why |
| --- | --- | --- | --- |
| Name of the crate | `mbook`, the name of the format | `mbook-core` | Decided by the format author. The library is the reference implementation of the format; crates built on it, such as `mbook-converter`, carry a suffix. |
| Shape of the API | `BookReader` for lazy reading, `Book` for the editable model | One type generic over its storage | A lazy read can fail and needs `&mut`; an in-memory read cannot and does not. One type would force the weaker signature on both. |
| | | A `BookSource` trait implemented by both | Same problem, plus a trait in every caller's signatures. |
| Streams that cannot seek | `buffered` gives the ordinary reader; `Book::read` gives the model | A third, forward-only reader type | Reading applications get one API. Memory is the compressed size instead of the decoded size. |
| Editing | Load into `Book`, change, write a new file | Nothing else was considered | Decided by the format author: one in-memory model serves both creating and editing. |
| Addressing in `Book` | Path | Resource number; opaque handles | Numbers shift on every insertion. Paths are stable, unique, and already what chapters use. |
| Navigation data | Derived from text at write time | Editable `toc` and `anchors` tables with offsets | An offset that is still a valid position after an edit points at the wrong heading without any error. |
| Anchors of lines that are not headings | Read from anchor definitions in the text ([ADR-0009](../rfcs/adr/0009-anchor-definitions.md)) | `add_anchor(name, offset)`, dropped by `set_text` | Implemented first and removed: correcting one letter of a chapter deleted all its link targets. |
| | | The same, with offsets carried across `set_text` by comparing lines | A rewritten line is matched by position, which can name the wrong line without any error. |
| Landmarks in `Book` | A list replaced as a whole, each target named by path and anchor | `add_landmark` and `remove_landmark` | The contents work the same way; one call keeps the rule about `start` in one place. |
| Landmarks in `BookReader` | Read on request, by `read_landmarks` | Decoded when a book is opened | RFC 0001 keeps them out of the resources every book has; most books have none. |
| Resolving a fragment that is not a heading's | `resolve` without I/O returns `ExtraAnchor`; `read_resolve` reads the table | `resolve` reads the table itself | Table lookups take `&self` and do no I/O; a renderer calls `resolve` for every link of a page. |
| | | The reader keeps every anchor table it has read | Memory would grow with the chapters visited, for a table of a few hundred bytes that sits next to the chapter. |
| When rules are checked | One-item rules at the call, cross-item rules at `write` | Everything at `write` | The error names the call that caused it. |
| | | Everything at the call | A chapter could not be added before the pictures it shows. |
| Ids in `BookReader` | `ChapterId` and `PictureId`, and later `DiagramId` and `FormulaId` | One `ResourceId` | Reading a picture as a chapter becomes a compile error instead of `WrongKind` at run time. |
| Diagrams and formulas in the API | Two types, `Diagram` and `Formula`, with calls of their own | One `Figure` type with a kind | A diagram cannot be handed to a formula typesetter by mistake, and the two lists of an editor are two calls. The price is the same six calls twice. |
| Changing a diagram or a formula | `diagram_mut` and `set_source` | Remove and add, as for a picture | Its source is text that is edited; removing it would move it to the end of its list and leave the book, for a moment, with references that lead nowhere. |
| Checking Mermaid and KaTeX notation | Not done | A parser of each behind a feature | Neither notation has a specification, a parser would tie a book to one implementation's version, and RFC 0001 lets a reader fall back to the alternative text. |
| | | A UUID for every resource | The format has nowhere to store one, and the path is already a stable unique name. UUID version 7 is used for the book's `id` only. |
| Line endings and byte order marks | Normalised silently | Rejected with an error | The meaning of the text does not change, and every caller would write the same replacement. |
| Moving a resource to another path | Remove and add | A `rename` that rewrites links | The crate would start editing chapter text it was given. |
| Helpers | Established crates for mechanisms; own code for the rules of the format | Hand-written cursors, decoders and temporary files | Less code to test and to keep correct; the exception is listed under [Dependencies and features](#dependencies-and-features). |
| mopenzl in the API | Hidden behind `Error::Container` and `Limits` | Re-exporting `FramedError` and `FramingConfig` | A mopenzl major version would become an mbook major version. |

## Prior art

| Crate | Taken from it | Left behind |
| --- | --- | --- |
| `zip` | A reader generic over `Read + Seek`; entries decoded on request | Entries borrowed from the archive, which makes holding two at once awkward |
| `epub` | `EpubDoc::new(path)` as the one-line entry point; typed metadata getters | A cursor ("current page") inside the document object |
| `std::fs` | `open` for a path, `new` for a value; `read_` for calls that do I/O | |
| `toml_edit` | A document model that is loaded, changed and written back | |

## Settled and open questions

The questions raised by the first draft were settled by the format author on
2026-11-21:

| Question | Decision |
| --- | --- |
| How is a book edited? | Only through a `Book` in memory. |
| Which Markdown parser does the write side use? | `markdown` 1.0.0. |
| Where does the `id` of a new book come from? | `Book::new` generates a UUID version 7 with the `uuid` crate. |
| Are resources identified by UUID as well? | No. Only the book's `id` is a UUID. |
| CRLF line endings and a byte order mark? | Normalised silently. |
| A contents row at a position without an anchor? | Forbidden by RFC 0001; a contents target is a chapter and an optional anchor. |
| Renaming a resource? | Not provided; remove and add. |
| How are reader resources identified? | `ChapterId`, `PictureId`, `DiagramId` and `FormulaId`. |
| `Limits::default()`? | 1,000,000 resources, 64 MiB per resource, 16 GiB per file. |
| Can a link lead to a line that is not a heading? | Yes, through the anchor table of its chapter ([ADR-0008](../rfcs/adr/0008-chapter-anchor-tables.md)). |
| Where do the names of such lines come from? | From anchor definitions in the text of the chapter ([ADR-0009](../rfcs/adr/0009-anchor-definitions.md)). |
| Can a book list its notable places? | Yes, as landmarks in an optional table ([ADR-0010](../rfcs/adr/0010-landmarks.md)). |
| Can a book carry diagrams and formulas as text? | Yes, as resources shown like pictures ([ADR-0011](../rfcs/adr/0011-diagrams-and-formulas.md)); decided on 2026-11-22. |

The change required from mopenzl
([Required from mopenzl](#required-from-mopenzl)) is a dependency, not an open
question of this design.

Still *proposed*, and not confirmed by the format author:

- the names `extra_anchors`, `read_extra_anchors`, `read_resolve` and
  `Destination::ExtraAnchor`;
- the names `Landmark`, `LandmarkEntry`, `landmarks`, `set_landmarks` and
  `read_landmarks`;
- that a contents item cannot lead to an extra anchor.

## Future possibilities

- **Reusing buffers.** `read_chapter_into` and `read_picture_into` for readers
  that decode many resources in a row.
- **Asynchronous sources**, for books opened over a network with range
  requests.
- **`no_std` reading** from a byte slice, using the container layer's
  `no_std` build.
- **Packing and unpacking a folder**, as a small crate on top: the model
  already maps one to one onto files.
