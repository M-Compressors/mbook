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
| EPUB | EPUB 3 with XHTML chapters in reading order, contents, links, footnotes, metadata and a PNG cover and illustrations. |
| DOCX | Editable Word paragraphs and headings, basic inline styles, PNG pictures and core metadata. Tables and lists are flattened into paragraphs. |
| FB2 | FictionBook 2 XML with chapter sections, basic inline styles, metadata and Base64-encoded PNG pictures. |
| PDF | A paginated document with selectable text, PNG illustrations and embedded Noto Sans. Glyph coverage is limited to the bundled font. |
| CBZ | The paginated edition rasterized as numbered PNG pages, with a `ComicInfo.xml` metadata entry. |
| HTML | One self-contained document with contents, local links and PNG data URLs. |
| Markdown | One document with namespaced anchors and footnotes, and AVIF data URLs. Support for inline HTML anchors and data URLs varies across Markdown viewers. |
| Plain text | UTF-8 text in reading order with picture alternative text. |

Chapter links and anchors are retained in EPUB, HTML and Markdown. Other
writers flatten links to their visible text. Diagram and formula references
become alternative text. When an output needs PNG, the decoder uses the
coded AVIF pixels. AVIF container crop, rotation and mirror properties are
not applied by these writers. Markdown embeds the original AVIF. Export does
not fetch network resources. EPUB, DOCX, PDF and CBZ buffer the output
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
