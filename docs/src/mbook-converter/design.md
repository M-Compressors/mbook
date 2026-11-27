# Design

Which decisions are confirmed, which crates read each format, what the design
costs, and the choices behind it.

## What is settled

The format author settled the points below on 2026-11-21. Every other detail
of this section is *proposed* and listed again under
[Settled and open questions](#settled-and-open-questions) where a choice is
open.

| Decision | Status |
| --- | --- |
| The crate converts EPUB, FB2, PDF, CBZ, DOCX, HTML, Markdown and plain text into mbook | Confirmed |
| Mobipocket is not converted until a maintained crate reads it | Confirmed |
| One Cargo feature per import format; every import format enabled by default | Confirmed |
| Conversion from an I/O source and from a byte slice | Confirmed |
| Formats are read by established crates such as `unpdf` | Confirmed |
| Pictures are encoded as AVIF by `image` with its `avif` feature, which uses `ravif` and `rav1e`; dependencies under BSD licenses are accepted | Confirmed |
| A link to an element that is not a heading keeps its target: the converter writes an anchor definition in front of it ([ADR-0008](../rfcs/adr/0008-chapter-anchor-tables.md), [ADR-0009](../rfcs/adr/0009-anchor-definitions.md)) | Confirmed |
| A source without an identifier gets a UUID version 5 computed from its bytes as `id` | Confirmed |
| A source without a title or a language gets `Untitled` and `und`, each with a warning | Confirmed |
| A PDF without a text layer is converted page by page into pictures | Confirmed |
| A comic archive has one chapter per folder | Confirmed |
| Two kinds of source, `Read` and a slice; no entry point for `Read + Seek` | Confirmed |
| Pictures are encoded with quality 80 and speed 6 unless the options say otherwise | Confirmed |
| Encoding the pictures in parallel is behind the opt-in `rayon` feature | Confirmed on 2026-11-24 |
| A contents entry of the source that does not lead to a heading leads to the nearest heading above its target | Confirmed |
| The landmarks of a source become landmarks of the book ([ADR-0010](../rfcs/adr/0010-landmarks.md)) | Confirmed |
| Streaming means a `Read` as the source and a `Write` as the sink; the book is held in memory while it is converted ([Sources and memory](conversion.md#sources-and-memory)) | Confirmed |

## Dependencies

Every dependency was checked on 2026-11-21 against crates.io and its
repository, and those of figures on 2026-11-22: the version in use is the
latest one unless the notes say otherwise, and the column "Last release"
shows how alive the crate is.

| Need | Crate | License | Last release | Notes |
| --- | --- | --- | --- | --- |
| PDF to Markdown, pictures, outline, pages as pictures | `unpdf` 0.55, without default features, with `raster` | MIT | 2026-11 | Headings are found per page, by font size. Pictures are listed per page, not where they are drawn, and those under 64 pixels are left out. |
| EPUB package, spine, navigation, resources | `rbook` 0.7, without default features | Apache-2.0 | 2026-11 | Reads EPUB only; Mobipocket and KF8 are announced. |
| HTML to Markdown (EPUB, HTML) | `htmd` 0.6 | Apache-2.0 | 2026-11 | Writes the Markdown, with its own escaping; see below. |
| XML events (FictionBook, `ComicInfo.xml`) | `quick-xml` 0.42 with `encoding` | MIT | 2026-08 | |
| Word to Markdown | `undoc` 0.29, with `docx` only | MIT | 2026-11 | Headings need an outline level in their style. Bookmarks are not read, so links inside a document become text. Notes are plain text. A nested item is indented by two spaces a level, whatever the marker of its parent; see "Nested items of a Word list" below. Only `w:start` of a numbering is read, not an override of it. |
| ZIP archives | `zip` 9 | MIT | 2026-11 | |
| Dialect parsing with byte offsets | `markdown` 1.0.0 | MIT | 2025-04 | Already used by `mbook`. |
| Picture decoding and AVIF encoding | `image` 0.25 with `avif`, which encodes through `ravif` 0.13 and `rav1e` 0.8 | MIT OR Apache-2.0; BSD-3-Clause; BSD-2-Clause | 2026-03 | |
| Encoding pictures in parallel (`rayon`) | `rayon` 1.10 | MIT OR Apache-2.0 | 2026-11 | Already in the tree: `image`, `ravif` and `rav1e` encode a picture on its pool. |
| Base64 (pictures of FictionBook and of `data:` addresses) | `base64` 0.23 | MIT OR Apache-2.0 | 2026-08 | |
| Names of headings by GitHub's rule (Markdown) | `github-slugger` 0.1 | MIT | 2023-06 | One release; a port of a frozen algorithm. Already used by `mbook`. |
| The order of the pages of a comic | `alphanumeric-sort` 1.5 | MIT | 2026-07 | |
| Unicode NFC | `unicode-normalization` 0.1 | MIT OR Apache-2.0 | 2026-10 | |
| Text encodings | `encoding_rs` 0.8, `chardetng` 1 | Apache-2.0 OR MIT | 2026-10, 2026-03 | |
| Name-based UUID | `uuid` 1 with `v5` | Apache-2.0 OR MIT | 2026-11 | Already in the workspace. |
| Error types | `thiserror` 2 | MIT OR Apache-2.0 | 2026-09 | Already in the workspace. |
| Mermaid diagrams as SVG (`export`) | `merman` 0.11, without default features, with `all-diagrams`, `svg` and `layout-cytoscape` | MIT OR Apache-2.0 | 2026-11 | Without ELK, whose layout is EPL-2.0, and without math in labels. Moved from `mbook-ui` on 2026-11-22. |
| KaTeX formulas as SVG (`export`) | `ratex-parser`, `ratex-layout`, `ratex-types`, `ratex-svg` with `embed-fonts`, `ratex-font`, `ratex-unicode-font` 0.1 | MIT | 2026-11 | Glyphs as outlines. Moved from `mbook-ui`. |
| Glyph metrics of system fonts; the configuration of `merman` and the walk over a parsed formula (`export`) | `ab_glyph` 0.2, `serde_json` 1 | Apache-2.0; MIT OR Apache-2.0 | 2025-09, 2026-10 | Moved from `mbook-ui`. |
| Rasterising a drawn figure for the writers (`export`) | `resvg` 0.46 with `text`, `system-fonts` and `memmap-fonts` | Apache-2.0 OR MIT | 2026-11 | 0.50 is the latest release; 0.46 is the version GPUI depends on, so the application keeps one copy of `resvg`, `usvg` and `fontdb`. Without `raster-images`: a drawn figure holds no pictures. |

The format author accepted dependencies under BSD-2-Clause and BSD-3-Clause
on 2026-11-21: they ask only that the license text is kept. `image` itself
contains no AV1 encoder, so its `avif` feature cannot be had without `ravif`
and `rav1e`.

**One mechanism is written in this crate: escaping text as Markdown**, about
forty lines in `markdown.rs`, used where a front end writes Markdown itself
(FictionBook, plain text, comic archives). What must be escaped is a rule of
the mbook dialect, not of Markdown in general: a single tilde marks
strikethrough, and the parser that decides is `markdown` 1.0.0. The function
is tested against that parser. `comrak` and `pulldown-cmark-to-cmark` write
Markdown for their own parsers and dialects, and would add a second Markdown
implementation to the dependency tree.

**HTML is written as Markdown by `htmd`**, including its escaping, which was
checked against the dialect: a lone tilde, emphasis marks, the starts of
headings, lists and quotes, brackets and literal tags all come back as text.
`xhtml.rs` adds what Markdown output would otherwise lose, as handlers of
`htmd` and as edits of the Markdown it wrote:

| Added here | Why `htmd` alone is not enough |
| --- | --- |
| `script`, `style`, forms and embedded objects are skipped | Their content would be written as text. |
| `s`, `strike`, `del` as `~~…~~` | Written as plain text. |
| Marks for elements with an `id` | Identifiers are dropped. |
| Footnotes of EPUB | Not recognised. |
| A caption of a figure as the title of its picture | Written as a separate paragraph. |
| A table without a header row | Written as paragraphs; here its first row becomes the header. |
| `pre` without `code`, and pictures inside `svg` | Not handled. |
| `MarkupDropped` for `sub`, `sup`, `u` and merged cells | Dropped silently. |
| A line break inside a heading or a cell becomes a space | It would break the heading or the row. |

Two properties of `htmd` shape that code. A handler is registered for a list
of element names, there is no handler for every element, so the same handler
is registered for about a hundred names, and an element with a name outside
that list cannot be marked. And a handler must be `'static`, so it cannot
call the mappings of links and pictures that a front end supplies: handlers
leave the addresses as they are, and the mappings are applied afterwards, to
the Markdown, by the same code that the assembler uses.

**Mobipocket is not converted** (decided by the format author on
2026-11-21). A front end was written on `mobi` 0.8 and removed again. The
crate has had no release since 2022 and depends on the unmaintained
`encoding` 0.2. Writing against it showed more: its record access slices
without checking and panics on a damaged file, its text accessor drops the
last text record, and its PalmDOC decompressor shortens a match that is
longer than its distance, which shifts every byte offset after it. Only its
header parser could be used; cutting the records and unpacking the text had
to be written here, which is a parser of a binary format in all but name.
No other crate with a permissive license reads the format: `boko` is under
the GPL, and `rbook` has only announced it.

**The structure of FictionBook is read by this crate**, as events of
`quick-xml` (decided by the format author on 2026-11-21). The `fb2` crate was
used first and dropped: it has had no release since 2023, pins `quick-xml`
0.30, fails on a whole document for a series number such as `1.5` or for an
empty `body`, and loses the order between the text of a section and its
subsections.

Crates that were looked at and ruled out:

| Crate | Why it does not fit |
| --- | --- |
| `fb2` 0.4 | Unmaintained since 2023; the defects above. |
| `mobi` 0.8 | Unmaintained since 2022; the defects above. |
| `palmdoc-compression` 0.3 | Would replace one of the three parts `mobi` gets wrong; last release 2024, about 200 recent downloads. Not evaluated. |
| `natord` 1 | No release since 2015; replaced by `alphanumeric-sort`. |
| `ebook-rs` 0.19 | Reads FictionBook, Mobipocket and KF8, but is three months old with 66 releases, cannot switch formats off, and always brings `aes`, `regex`, `whatlang`, `serde_json` and `parking_lot`. Its source was not read. |
| `epub` 2.1 | GPL-3.0-or-later. |
| `html2md` 0.2 | GPL-3.0-or-later. |
| `boko` 0.6 | GPL-3.0-or-later, and needs Rust 1.91. |
| `html-to-markdown-rs` 3 | Needs Rust 1.99. |
| `kreuzberg` 4, `hayro` 0.9 | Need Rust 1.91 and 1.92. |
| `mdka` 3.3 | No hook to rewrite a link or a picture; writes raw `<a id>` and `<br>`. |
| `anytomd` 1.3 | No EPUB, FB2 or PDF; no hook for links; brings an HTTP client by default. |
| `pdf-extract`, `lopdf` | Plain text without headings, tables or an outline; `unpdf` gives all three. |

## Drawbacks

- **Memory is the size of the book, not of a window.** The whole `Book`
  exists before the first byte is written, and for most formats the source
  is held as well.
- **AVIF encoding dominates the time.** The pure-Rust encoder needs seconds
  for a large page; a comic of several hundred pages takes minutes. The
  `rayon` feature spreads the pictures over the cores; a small picture alone
  keeps only a few of them busy.
- **The dependency tree is large with default features**: an AV1 encoder,
  an HTML parser, two versions of `quick-xml`, a PDF parser.
- **The quality of a conversion is the quality of the front-end crate.** A
  PDF has no chapters or paragraphs of its own; what `unpdf` reconstructs is
  what the book contains.
- **A link is kept to the block, not to the word.** A link to an element
  inside a paragraph leads to the start of that paragraph, a link to a row
  of a table to the table, and a contents entry that does not name a heading
  to the heading above its target.
- **One build per feature.** The completion checks of the repository run
  Clippy, the tests and the documentation once for every format alone.
- **Four entry points for one operation**, because Rust cannot choose
  between a `Read` and a slice by itself.

## Rationale and alternatives

| Decision | Chosen | Rejected | Why |
| --- | --- | --- | --- |
| Where formats are parsed | An established crate per format | Parsers of this crate over `zip` and `quick-xml` | Decided by the format author. Less code to keep correct; the cost is memory and a larger tree. |
| What a front end returns | A draft in the source's terms; one assembler | Each front end builds a `Book` | Paths, links, the dialect and pictures would be implemented once per format. |
| Result of a conversion | A `Book`, then `Book::write` | A writer that emits resources while the source is read | Decided by the format author: a `Read` in and a `Write` out is enough. The tables are the first resources of a file and describe everything after them; `mbook` writes only from a `Book`. |
| Sources | `Read` and `&[u8]` | Paths | A caller opens a file in one line; a stream has no path. |
| | | One entry point over a `Source` enum | `&[u8]` is itself a `Read`, so a conversion from both cannot be chosen by a trait. |
| Sources that could seek | Buffered like any `Read` | A third entry point for `Read + Seek` | Decided by the format author: the API stays at two kinds of source. A caller that minds the copy maps the file into memory and passes the slice. |
| Finding the format | From the content | From a file extension | Streams have no names, and extensions are wrong often enough. |
| A disabled format | The variant stays; `Error::FormatDisabled` | Variants behind `cfg` | A `match` on `Format` would change with the feature set, and the error could not say what is missing. |
| Pictures | Encoded by this crate | An encoder supplied by the caller | Works without setup. A trait for other encoders can be added without breaking the API. |
| AVIF encoder | `rav1e`, through `ravif` and the `avif` feature of `image`; pure Rust | `libavif` or `libaom` | Decided by the format author. No C toolchain, and one crate both decodes the source pictures and encodes the result; slower than the C encoders, which are under BSD licenses as well. |
| Constructs that cannot be kept | Dropped with a warning | An error | A book without one table is more useful than no book. |
| Links to elements that are not headings | Kept, as anchor definitions in the target chapter | Sent to the nearest heading | Decided by the format author, who changed the format for it ([ADR-0008](../rfcs/adr/0008-chapter-anchor-tables.md), [ADR-0009](../rfcs/adr/0009-anchor-definitions.md)). |
| Missing title or language | A stand-in value and a warning | An error | The caller can correct a `Book`; it cannot correct a failed conversion. |
| Final validation | `Book::check` of `mbook` | Checks of this crate | One definition of a valid book. |
| Export input | A borrowed `Book` | A second set of stream and slice entry points | A caller can export an edited in-memory book. `Book::open` or `Book::read` can load a saved one. |
| Export writers | `epub-builder`, `docx-rs`, `krilla`, `xmlwriter` and the Markdown serializers of `pulldown-cmark` and `pulldown-cmark-to-cmark` | Hand-built ZIP and PDF document packages or shelling out to a converter | The libraries handle the formats and work without an installed converter. |
| Nested items of a Word list | Before `undoc` renders the document, the level of every item in its model is set to the one whose two spaces a level reach the text of the parent item: one more level under `- `, two under `1. ` or `10. ` | Reindenting the Markdown `undoc` renders; rendering the lists without `undoc`; leaving it to `undoc` | CommonMark nests an item only when it is indented as far as the text of its parent, and `undoc` indents two spaces a level, which is enough under a bullet only, so an item under a number joined the list of its parent and a bullet under a number split that list in three. In the rendered text an item cannot be told from a line of text that starts with spaces, and rendering lists here would repeat the escaping and the runs of `undoc`. The model is public and the level is used only for the indent. The cost is a dependence on the two spaces a level of `undoc` 0.29, which the tests of the importer check. |
| Lists in an exported Word document | Word numbering: one abstract numbering for each list of the book, with its start in the level of the list, and every paragraph of an item a paragraph of its own | Paragraphs with the marker as text; one numbering for all lists, restarted with an override of the level | With the marker as text an item had no indent, a tight nested list ran into the paragraph of its parent, and a document read back had text where the lists were. Word continues counting across the numberings of one abstract numbering unless it is overridden, and `undoc` does not read the override, so a list that starts at 3 would come back from 1. One numbering for each list costs a few hundred bytes of `numbering.xml` per list. |
| Landmarks in an EPUB | The writer replaces the `landmarks` navigation and adds to the guide in the archive `epub-builder` generated, rewriting those two documents with `zip` and copying every other entry as it is | The `reftype` of `epub-builder`; writing the EPUB package without `epub-builder` | A `reftype` belongs to a whole document of the spine and takes one of 17 fixed types, so the anchor of a landmark, a second landmark of one chapter and six kinds would be lost, and `epub-builder` does not make its archive trait public to change the files it writes. Writing the package by hand would replace the library for one table. The cost is a second pass over the archive in memory and a dependence on the paths and the `<guide>` that `epub-builder` 0.8 writes, which the tests of the writer check. |
| CBZ pages | Rasterize the paginated edition with `unpdf` | Export only embedded pictures | A text-only book also produces readable pages. |
| Encoding pictures in parallel | Behind the opt-in `rayon` feature: the pictures the chapters show are found first, encoded on the `rayon` pool in runs of at most 32,000,000 declared pixels, and stored in the order of first use | Always in parallel; encoding as each chapter shows a picture, as without the feature; `std::thread::scope` | Decided by the format author to put it behind a feature. Measured on a 7 MB EPUB with 45 pictures (25.5 megapixels in all, the largest 1.9) on an Apple M-series processor with the NEON assembly of `rav1e`: 3.9 s before, 2.6 s with it on eight threads, 5.6 s and 4.3 s on four; the bytes are the same. `rav1e` already works on the pool of `rayon`, so one pool shares the cores between pictures and within one; threads of their own would compete with it. The budget keeps the memory of a run near that of one picture at the limit of `max_picture_pixels`; it is a constant, not an option, and was chosen without a measurement of memory. The price is a second walk over each chapter to find its pictures. |
| Export feature | One opt-in feature for all writers | An export switch for each format or writing enabled by default | One dependency setting enables every output while the import-only build keeps its dependency set and minimum Rust version. |
| Export picture decoding | `avif-decode` 3 on rav1d | `image` with `avif-native` or `avif-decode` 1 with libaom | The feature shares the decoder with the application. `avif-native` links to system dav1d, and the older decoder builds libaom with CMake. |
| Diagrams and formulas in an export | Drawn as SVG and written as PNG pictures | Their alternative text, the earlier behaviour; SVG pictures; Mermaid fences and `$…$` in Markdown | Decided by the format author on 2026-11-22: figures go out as pictures. FictionBook and many Word and EPUB readers show no SVG, and an SVG diagram depends on the fonts of the reader for its labels; PNG looks the same everywhere. |
| Where figures are drawn | The public `figure` module of this crate, used by export and by `mbook-ui` | A private copy for export beside the one of `mbook-ui`; a crate of its own | Decided by the format author on 2026-11-22. One renderer, so an export looks like the reader; the cost is four public items under `export`. A crate of its own would be published and versioned for two users. |
| A figure that cannot be drawn | Its alternative text, and the export goes on | An export error | RFC 0001 lets a reader show the alternative text; a book with one broken formula is still worth exporting. A picture that cannot be decoded still fails the export, as before. |
| The size of a figure in an export | Scaled to the body text of the output, drawn at two pixels per unit | The pixels of the drawing as they are | A formula drawn for 20-pixel text would stand out of a 16-pixel line, and a single pixel per unit blurs on a dense screen. |

## Prior art

| Tool | Taken from it | Left behind |
| --- | --- | --- |
| Calibre `ebook-convert` | Input plugins that produce one intermediate book; one output stage | An intermediate of XHTML and CSS |
| Pandoc | Readers, one document model, writers | A model that covers every construct of every format |
| `unpdf`, `undoc`, MarkItDown | Markdown as the result of extraction; pictures as separate resources | Raw HTML as a fallback |
| `mbook` | `open`-like entry points, `Limits`, non-exhaustive options and errors | |

## Settled and open questions

The questions of the first draft were settled by the format author on
2026-11-21; see [What is settled](#what-is-settled). Two were settled by the
implementation:

| Question | Outcome |
| --- | --- |
| Which crate writes Markdown for FB2 and text? | None: escaping is a function of this crate, tested against `markdown` 1.0.0 ([Dependencies](#dependencies)). |
| How does this crate keep its `markdown` configuration equal to that of `mbook`? | Both call `markdown::ParseOptions::gfm()`, and every converted book passes `Book::check`, which parses each chapter with the configuration of `mbook`. |

Still *proposed*: the names of the public items, and every front-end crate
marked so in [Formats and features](api.md#formats-and-features).

## Future possibilities

- **Conversion in bounded memory**: a writer in `mbook` that spools
  encoded resources to a temporary file and writes the tables last, fed by
  front ends that emit sections as they parse.
- **Progress and cancellation**, for the minutes a large comic takes.
- **A caller-supplied picture encoder**, for hardware or C encoders.
- **More formats** when a suitable crate exists: Mobipocket and KF8/AZW3
  (announced for `rbook`, which already reads EPUB here), DjVu, RTF, CBR.
- **SVG and JPEG 2000 pictures**, rendered or decoded before encoding.
- **A strict mode** that turns chosen warnings into errors.
- **Metadata from the front matter of a Markdown document**, and pictures
  given to a Markdown or HTML document as `data:` addresses or by a caller.
- **A command-line tool** on top of this crate and `mbook`.
