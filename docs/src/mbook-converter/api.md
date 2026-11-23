# API reference

The signatures below are the public surface of the crate as implemented. The
item-by-item documentation is on
[docs.rs](https://docs.rs/mbook-converter).

## Public surface

```rust
pub struct Converter { /* options */ }

impl Converter {
    pub fn new() -> Self;                                   // also Default
    pub fn with_options(options: ConvertOptions) -> Self;

    // Source to file.
    pub fn convert(&self, source: impl Read, sink: impl Write) -> Result<Report>;
    pub fn convert_slice(&self, source: &[u8], sink: impl Write) -> Result<Report>;

    // Source to model.
    pub fn load(&self, source: impl Read) -> Result<Converted>;
    pub fn load_slice(&self, source: &[u8]) -> Result<Converted>;

    #[cfg(feature = "export")]
    pub fn export(book: &mbook::Book, format: Format, sink: impl Write) -> Result<()>;
}

#[cfg(feature = "export")]
pub mod figure {
    pub fn diagram(source: &str, palette: Palette) -> Result<Figure, FigureError>;
    pub fn formula(source: &str, setting: Setting, palette: Palette)
        -> Result<Figure, FigureError>;

    pub enum Setting { Block, Line }
    pub struct Palette { pub ink: u32, pub page: u32 }      // also Default
    pub struct Figure { /* SVG and measures */ }            // svg, into_svg, width, height, baseline, em
    #[non_exhaustive]
    pub enum FigureError { Empty, Diagram(String), Formula(String), Renderer }
}

#[non_exhaustive]
pub struct Converted { pub book: mbook::Book, pub report: Report }

#[non_exhaustive]
pub struct Report { pub format: Format, pub warnings: Vec<Warning> }

#[non_exhaustive]
pub enum Format { Epub, Fb2, Pdf, Cbz, Docx, Html, Markdown, Text }

impl Format {
    pub const ALL: &'static [Format];
    pub const fn is_enabled(self) -> bool;                  // was its feature compiled in?
    #[cfg(feature = "export")]
    pub const fn extension(self) -> &'static str;           // e.g. "md"
}
impl Display for Format { /* "epub", "fb2", … */ }
impl FromStr for Format { /* the same names, and common file extensions */ }

// The four structs of options and limits; see "Options and limits".

pub use mbook;
```

- `convert(source, sink)` is `load(source)` followed by
  `book.write_with(sink, &options.write)`. Nothing is written to the sink
  unless the whole conversion succeeded.
- A `Converter` holds only its options. It is `Send + Sync`, and one value
  serves any number of conversions.
- `Format` has every variant whatever features are enabled, so a `match` on
  it does not depend on the feature set.
- `mbook` is re-exported: `Book`, `WriteOptions`, `Limits` and
  `mbook::Error` appear in this API. No type of any other dependency
  does; the rule of [`mbook`](../mbook/index.md#goals) applies to this crate as well.

## Export

`Converter::export` takes an existing `Book` and a target format. The single
`export` feature enables the public export API, all eight writers, their
AVIF decoder, and the `mbook` read API. It works independently of the import
features, including in a build with only `export` enabled.

The call returns `Error::Io` if the sink fails or `Error::Export` with the
target and source error if encoding fails. The decoder bounds both the
primary and alpha coded dimensions at 100 million pixels per plane. The
caller owns the destination and can use a temporary file to publish it
atomically.

The `export` feature currently needs Rust 1.98. Its minimum can increase
independently of the Rust 1.89 minimum for the default import build.

| Format | Output |
| --- | --- |
| EPUB | EPUB 3 with XHTML chapters in reading order, contents, [landmarks](#landmarks-in-an-epub), links, footnotes, metadata, a PNG cover and illustrations, and diagrams and formulas as PNG resources. |
| DOCX | Editable Word paragraphs and headings, basic inline styles, PNG pictures, diagrams and formulas, and core metadata. Tables and lists are flattened into paragraphs. |
| FB2 | FictionBook 2 XML with chapter sections, basic inline styles, metadata and Base64-encoded PNG pictures, diagrams and formulas. |
| PDF | A paginated document with selectable text, PNG illustrations, diagrams and formulas, and embedded Noto Sans. Glyph coverage is limited to the bundled font. A formula inside a line is drawn on a line of its own. |
| CBZ | The paginated edition rasterized as numbered PNG pages, with a `ComicInfo.xml` metadata entry. |
| HTML | One self-contained document with contents, local links and PNG data URLs. |
| Markdown | One document with namespaced anchors and footnotes, AVIF data URLs for pictures, and `img` elements with PNG data URLs for diagrams and formulas. Support for inline HTML and data URLs varies across Markdown viewers. |
| Plain text | UTF-8 text in reading order with the alternative text of pictures, diagrams and formulas. |

Chapter links and anchors are retained in EPUB, HTML and Markdown. Other
writers flatten links to their visible text. When an output needs PNG, the
decoder uses the coded AVIF pixels. AVIF container crop, rotation and mirror properties are
not applied by these writers. Markdown embeds the original AVIF. Export does
not fetch network resources.

### Landmarks in an EPUB

The [landmarks](../rfcs/0001-mbook-format.md#landmarks-1) of the book are
listed in the `landmarks` navigation of the EPUB, after the cover page when
the book has a cover, and, for the kinds that EPUB 2 has a type for, in the
guide of the package as well. No other writer keeps them.

- A kind is written as its type of the EPUB structural semantics
  vocabulary: `start` as `bodymatter`, `notes` as `endnotes`,
  `acknowledgements` as `acknowledgments`, `title-page` as `titlepage`,
  `list-of-figures` and `list-of-tables` as `loi` and `lot`, `copyright` as
  `copyright-page`, and every other kind by its own name. In the guide,
  `start` is `text`, `notes` is `notes`, and `introduction`, `prologue`,
  `epilogue`, `conclusion`, `afterword` and `appendix` have no reference.
- A landmark leads to the document of its chapter, and to the anchor it
  names in it.
- A landmark without a title gets the name of its kind in English, such as
  `Start` or `Index`, since a link of the navigation must have text.
- Left out are `about-the-author` and the kinds for private use (`x-…`),
  which EPUB has no type for, and a landmark whose chapter or anchor the
  book does not have.
- A book with neither landmarks nor a cover has no `landmarks` navigation.

An EPUB exported this way gives the same landmarks when it is
[imported](conversion.md#landmarks) again, except that a landmark without a
title gets the name it was written with.

Diagrams and formulas are drawn by [`figure`](#diagrams-and-formulas) in
dark ink on a white page and written as PNG pictures, except in plain text.
A figure is scaled so that its text has the size of the body text of the
output: 16 CSS pixels in EPUB, HTML and Markdown, 16 pixels in FictionBook,
11 points in DOCX and PDF. It is drawn at two pixels per unit of that size,
three per point in PDF, and one in FictionBook, which has no size for a
picture; a figure that would take more than 16,777,216 pixels is drawn less
sharply. A diagram is drawn on the colour of the page, a formula on a
transparent background. A formula alone in its paragraph is a display
formula, and any other formula is set in its line (RFC 0001,
"Rendering"). EPUB, HTML and Markdown write a figure as an `img` element:
one alone in its paragraph is centred at its width, and one inside a line
takes the height of its drawing and is lowered to the baseline of the line.
A diagram or a formula that cannot be drawn is written as its alternative
text and does not fail the export. The labels of a diagram use the fonts
of the system, with the bundled Noto Sans for any family the system lacks.

## Diagrams and formulas

`figure::diagram` draws a diagram in Mermaid notation with `merman`, and
`figure::formula` sets a formula in the notation of KaTeX with RaTeX. Each
gives a `Figure`: an SVG document, its width and height, the baseline of a
formula (the height for a diagram), and `em`, the size of text it was drawn
for (16 for a diagram, 20 for a formula). A page scales a figure by the
ratio of its own text size to `em`. The colours are a `Palette` of ink and
page; the default is `#1b1b1b` on white. `Setting::Block` sets a display
formula and `Setting::Line` one in the style of running text; a diagram is
the same in both.

- Leading and trailing white space of a source is not part of it; an empty
  source is `FigureError::Empty`.
- A source that is not a diagram or a formula is `FigureError::Diagram` or
  `FigureError::Formula`, with the message of the renderer as text, so the
  error can be cloned and kept with a cache entry.
- A panic of a renderer is caught and reported as `FigureError::Renderer`.
- Mermaid runs in strict mode, so a `click` does nothing, and labels are
  written as SVG text without `foreignObject`. A remote image in a diagram
  and the target of `\href` in a formula are not part of the drawing.
- The glyphs of a formula are outlines. A letter that the KaTeX fonts lack,
  such as a Cyrillic one, is drawn with a system font and given the room of
  its real glyph.

`mbook-ui` draws the figures of its pages with the same calls. EPUB, DOCX, PDF and CBZ buffer the output
document in memory before writing it to the sink.

## Formats and features

| Feature | Source format | Front-end crate | Status |
| --- | --- | --- | --- |
| `epub` | EPUB 2 and 3 | `rbook` | Confirmed format; crate *proposed* |
| `fb2` | FictionBook 2, plain and inside a ZIP (`.fb2.zip`) | `quick-xml`, `zip` | Confirmed |
| `pdf` | PDF | `unpdf` | Confirmed |
| `cbz` | Comic book archive: a ZIP of pictures | `zip`, `alphanumeric-sort` | Confirmed |
| `docx` | Word documents | `undoc` | Confirmed format; crate *proposed* |
| `html` | One HTML document | `htmd` | Confirmed format; crate *proposed* |
| `md` | One Markdown document | `markdown`, `github-slugger` | Confirmed |
| `txt` | Plain text | none | Confirmed |

- `default` enables every feature of the table.
- A build must enable an import format or `export`; otherwise compilation fails
  with a message, as in `mbook`.
- A feature enables only the dependencies its format needs. Shared mechanisms
  have no feature of their own: the HTML-to-Markdown crate comes with `epub`
  and `html`, the picture codecs with every format that carries
  pictures. The build script of the crate tells the compiler which of these
  groups is in use, so that the list of formats in a group is written once.
- `zip` and `markdown` are dependencies of every build: the first is needed
  to tell the formats that are ZIP archives apart, the second reads every
  chapter.
- Every single-feature build must compile without warnings, including its
  documentation.
- `mbook` is used with its `write` feature only.

Not in the first version, for lack of a maintained crate that meets the
license and the minimum Rust version: Mobipocket, KF8/AZW3, DjVu, RTF, CBR
([Future possibilities](design.md#future-possibilities)).

## Options and limits

```rust
#[non_exhaustive]
pub struct ConvertOptions {
    pub format: Option<Format>,             // None: detect
    pub pictures: PictureOptions,
    pub limits: ConvertLimits,
    pub write: mbook::WriteOptions,    // used by convert and convert_slice
}

#[non_exhaustive]
pub struct PictureOptions {
    pub quality: u8,                        // 1 to 100
    pub speed: u8,                          // 1 (smallest file) to 10 (fastest)
}

#[non_exhaustive]
pub struct ConvertLimits {
    pub max_source_bytes: u64,
    pub max_picture_pixels: u64,
    pub book: mbook::Limits,
}
```

| Setting | Default | Reason |
| --- | ---: | --- |
| `pictures.quality` | 80 | Decided by the format author; not measured yet. |
| `pictures.speed` | 6 | Decided by the format author; not measured yet. |
| `limits.max_source_bytes` | 2 GiB | *Proposed.* Bounds the buffer of a source that cannot seek. |
| `limits.max_picture_pixels` | 100,000,000 | *Proposed.* A page scanned at 600 dpi is about 35 million pixels. |
| `limits.book` | `Limits::default()` | The same limits the written file is read with. |

- All three structs implement `Default` and are changed by assigning their
  fields, like `ReadOptions` of `mbook`.
- A value outside its range is brought into the range.
- `limits.book.max_resource_bytes` also bounds one unpacked entry of an
  archive and one decoded Base64 picture; `limits.book.max_file_bytes` bounds
  the sum of everything unpacked.

## Errors and warnings

One `Error` enum, non-exhaustive, built with `thiserror`, and
`Result<T> = Result<T, Error>`.

| Variant | Meaning |
| --- | --- |
| `Io` | The source or the sink failed. |
| `UnknownFormat` | The content matches no format of [Detection](conversion.md#detection). |
| `FormatDisabled(Format)` | The format was recognised or named, and its feature is off. |
| `Source { format, .. }` | The front-end crate could not parse the source. Its error is the `source()`, as a boxed `dyn Error`. |
| `Export { format, .. }` | The writer or picture codec failed. Its error is the `source()`, as a boxed `dyn Error`. |
| `Protected(Format)` | The source is encrypted or under rights management. |
| `Empty` | Nothing in the source could be converted. |
| `Limit` | An import limit or the export pixel limit was exceeded. |
| `Book` | `mbook` refused a call, the final check or the write. Its error is the `source()`. |

```rust
#[non_exhaustive]
pub struct Warning {
    pub kind: WarningKind,
    pub location: Option<String>,   // the source's name for the place, when known
}

#[non_exhaustive]
pub enum WarningKind {
    MarkupDropped(String),          // what: "sub", "table with merged cells", …
    PictureDropped(String),         // the source's name of the picture
    LinkRetargeted(String),         // a contents entry: its destination in the source
    LinkDropped(String),
    MetadataDropped(String),        // the key
    MetadataMissing(String),        // the key that got a stand-in value
}

impl Display for Warning { /* one line, without a trailing full stop */ }
```

Warnings are in the order in which the assembler met them.
