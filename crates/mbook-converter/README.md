# mbook-converter

**Bring your library to mbook, and take it back whenever you like.**

[![crates.io](https://img.shields.io/crates/v/mbook-converter.svg)](https://crates.io/crates/mbook-converter)
[![docs.rs](https://docs.rs/mbook-converter/badge.svg)](https://docs.rs/mbook-converter)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

Convert books between other formats and [mbook](https://m-compressors.github.io/mbook/)
files: Markdown chapters and AVIF pictures in one indexed container.

- **Eight formats in, eight out.** EPUB, FB2, PDF, comic archives, Word,
  HTML, Markdown and plain text are read; with the `export` feature the
  same eight are written, so a converted book is never locked in.
- **A quarter of the size.** A 7.0 MB EPUB of an illustrated manual became
  a 1.6 MB mbook file at the default settings, in three seconds. Pictures
  are encoded as AVIF, which is lossy at the default quality of 80.
  [How it was measured][measured].
- **Nothing is lost silently.** Whatever a book cannot keep is reported as
  a `Warning`, and a converted book is checked before a byte is written.

**[Guide][guide]** · [API reference][api] · [Conversion rules][rules] · [Design][design] · [docs.rs](https://docs.rs/mbook-converter)

## Install

```sh
cargo add mbook-converter
```

Requires Rust 1.89 or later. The optional `export` feature currently
requires Rust 1.98 or later and uses NASM on x86 and x86-64; its minimum
Rust version can increase separately from the default build.

## Examples

One type does the work, `Converter`. The examples leave out the surrounding
function; each `?` returns an `mbook_converter::Error`.

| | To an mbook file, written to any `Write` | To a `Book` in memory |
| --- | --- | --- |
| **From any `Read`** | `convert` | `load` |
| **From a byte slice** | `convert_slice` | `load_slice` |

### Convert a file

The source is never asked to seek, and nothing is written to the sink
unless the whole conversion succeeded.

```rust
use mbook_converter::Converter;
use std::fs::File;
use std::io::BufWriter;

let source = File::open("book.epub")?;
let sink = BufWriter::new(File::create("book.mbook")?);

let report = Converter::new().convert(source, sink)?;
println!("converted from {}", report.format);
for warning in &report.warnings {
    eprintln!("{warning}");
}
```

### Convert bytes that are already in memory

A slice can be read at any position, so formats that need random access use
it in place where a `Read` has to be buffered first.

```rust
use mbook_converter::Converter;

let bytes = std::fs::read("book.fb2")?; // or an upload, a memory-mapped file
let mut mbook = Vec::new();
Converter::new().convert_slice(&bytes, &mut mbook)?;
```

### Correct the book before it is written

`load` and `load_slice` hand back an ordinary `Book` of
[`mbook`](https://crates.io/crates/mbook), which is re-exported as
`mbook_converter::mbook`.

```rust
use mbook_converter::Converter;
use std::io;

let mut converted = Converter::new().load(io::stdin().lock())?;

let book = &mut converted.book;
book.meta_mut().set_title("The Example")?;
book.meta_mut().set_language("en")?;
book.save("example.mbook")?;
```

### Name the format and tune the pictures

The format is detected from the content. Markdown and plain text have
nothing to be recognised by and must be named.

```rust
use mbook_converter::{ConvertOptions, Converter, Format};

let mut options = ConvertOptions::default();
options.format = Some(Format::Markdown); // do not detect
options.pictures.quality = 60;           // smaller pictures; the default is 80

let converter = Converter::with_options(options);
```

### Tell a missing feature from a broken source

A source in a format whose feature is off is still recognised, so the error
names it.

```rust
use mbook_converter::{Converter, Error};

let bytes = std::fs::read("book.pdf")?;
let mut mbook = Vec::new();
match Converter::new().convert_slice(&bytes, &mut mbook) {
    Err(Error::FormatDisabled(format)) => eprintln!("built without support for {format}"),
    other => {
        other?;
    }
}
```

### Export a book

Add `features = ["export"]` to the dependency. An export-only build can set
`default-features = false`.

```rust
use mbook_converter::{Converter, Format, mbook::Book};
use std::fs::File;
use std::io::BufWriter;

let book = Book::open("example.mbook")?;
let sink = BufWriter::new(File::create("example.epub")?);
Converter::export(&book, Format::Epub, sink)?;
```

### Draw a diagram or a formula

```rust
use mbook_converter::figure::{self, Palette, Setting};

let formula = figure::formula(r"v = \frac{s}{t}", Setting::Block, Palette::default())?;
std::fs::write("speed.svg", formula.svg())?;
```

## Formats

Each import format has a Cargo feature enabled by default. The optional
`export` feature enables every output format.

| Feature | Format | Read by | Exported as |
| --- | --- | --- | --- |
| `epub` | EPUB 2 and 3 | `rbook`, `htmd` | EPUB 3 with contents, landmarks, links, footnotes and PNG pictures |
| `fb2` | FictionBook 2, plain or inside a ZIP | `quick-xml` | FictionBook 2 with Base64 PNG pictures |
| `pdf` | PDF; a scan becomes one picture per page | `unpdf` | Pages with selectable text and embedded Noto Sans |
| `cbz` | Comic book archive | `zip`, `alphanumeric-sort` | The paginated edition as numbered PNG pages |
| `docx` | Word documents | `undoc` | Editable paragraphs and headings, lists numbered by Word; tables flattened |
| `html` | One HTML document | `htmd` | One self-contained document |
| `md` | One Markdown document | `markdown` | One document with AVIF data URLs |
| `txt` | Plain text | | UTF-8 text in reading order |

The opt-in `rayon` feature encodes the pictures of a source on all cores at
once; the book is the same, and it is written sooner.

Every output but plain text shows the Mermaid diagrams and KaTeX formulas
of a book as PNG pictures, drawn by the `figure` module, which the `export`
feature enables as well; plain text keeps their alternative text.

```toml
# Only EPUB and FB2
mbook-converter = { version = "0.3", default-features = false, features = ["epub", "fb2"] }
```

## What a conversion does

```text
source ─▶ detect ─▶ front end ─▶ draft ─▶ assembler ─▶ Book ─▶ .mbook
          format    one crate    in the    the rules     checked
                    per format   source's  of mbook,
                                 terms     once
```

- **Chapters.** One top-level division of the source becomes one chapter,
  in GitHub Flavored Markdown without raw HTML. A chapter above 256 KiB is
  split at its headings.
- **Pictures.** JPEG, PNG, GIF, WebP and BMP are encoded as AVIF; AVIF
  pictures are kept as they are. A picture is stored once, however often it
  is used, and nothing is fetched from a network.
- **Links.** A link inside the book keeps its target down to the block: a
  heading by its own anchor, any other line by an anchor definition the
  converter writes in front of it. A note reference becomes a footnote.
- **Metadata.** Title, language, authors, series, dates and the cover come
  from the source. A missing title or language gets `Untitled` or `und`,
  and a missing identifier a UUID computed from the bytes of the source, so
  converting the same file twice gives the same `id`.
- **Contents and landmarks.** The outline of the source becomes the
  contents; without one they are built from the headings. The landmarks of
  an EPUB are carried over.
- **The check.** The assembled book passes `Book::check` before it is
  returned or written.

### What is lost

A conversion that succeeds may still have dropped something. Each loss is
one `Warning` in the `Report`, with the place in the source when it is
known.

| `WarningKind` | Reported when |
| --- | --- |
| `MarkupDropped` | Raw HTML, a table with merged cells, subscript or superscript could not be kept |
| `PictureDropped` | A picture is missing, cannot be decoded, or is named by a network address |
| `LinkRetargeted` | A contents entry led to something that is not a heading, and now leads to the heading above it |
| `LinkDropped` | A link, a contents entry or a landmark led to nothing that was converted |
| `MetadataDropped` | A value of the source was refused by `mbook` |
| `MetadataMissing` | A title or a language got a stand-in value |

### Options and limits

| Setting | Default |
| --- | ---: |
| `pictures.quality`, 1 to 100 | 80 |
| `pictures.speed`, 1 (smallest file) to 10 (fastest) | 6 |
| `limits.max_source_bytes` | 2 GiB |
| `limits.max_picture_pixels` | 100,000,000 |
| `limits.book` | `mbook::Limits::default()` |

Peak memory is about the source plus the converted book: conversion is
streaming at its edges, a `Read` in and a `Write` out, and holds the book
in between. A protected source is refused with `Error::Protected`.

## Documentation

- [Guide][guide]: every task with its code
- [API reference][api]: types, features, options, errors, and what each
  export format keeps
- [Conversion rules][rules]: what happens to chapters, links, pictures and
  metadata, format by format, and
  [what each format cannot do yet][status]
- [Design][design]: the crate behind each format and the ones ruled out

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

[guide]: https://m-compressors.github.io/mbook/mbook-converter/guide.html
[api]: https://m-compressors.github.io/mbook/mbook-converter/api.html
[rules]: https://m-compressors.github.io/mbook/mbook-converter/conversion.html
[status]: https://m-compressors.github.io/mbook/mbook-converter/conversion.html#what-is-implemented
[design]: https://m-compressors.github.io/mbook/mbook-converter/design.html
[measured]: https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html#a-converted-book
