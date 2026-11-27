# mbook

**Books a quarter of the size that open before you notice.**

mbook is a file format for books: Markdown chapters, AVIF pictures, Mermaid
diagrams and KaTeX formulas in one indexed container, compressed with
[OpenZL]. A file opens without being read in full, decodes only what is on
screen, and needs neither a web engine nor an XML parser to show.

| 4× smaller | 0.1 ms to open | 1 GB/s | 8 formats in, 8 out |
| :---: | :---: | :---: | :---: |
| A 7.0 MB EPUB became a 1.6 MB mbook file | Title, contents and the list of every resource | Text decoding; a long chapter takes 0.1 ms | EPUB, FB2, PDF, DOCX, CBZ, HTML, Markdown, text |

One book, one machine: [how it was measured][measured].

This repository holds the format specification, `mbook`, the Rust library
that reads and writes it, `mbook-converter`, which turns books in other
formats into mbook and back, and `mbook-ui`, an application to read and edit
them.

[![Tests](https://github.com/M-Compressors/mbook/actions/workflows/ci.yml/badge.svg?branch=main)][ci]
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**[Documentation][site]** · [File format][format] · [Library][lib] · [Converter][converter] · [Application][app] · [Changelog](CHANGELOG.md)

## Why mbook

- **Small.** Pictures are AVIF and text is compressed by OpenZL, so a book
  needs no second packing step. The illustrated manual we measured went
  from 7.0 MB to 1.6 MB; your library takes a quarter of the disk and of the
  download.
- **Instant.** Opening a book reads a footer, a directory and five small
  tables: 80 µs until the title and a contents of 167 entries are ready. A
  library screen never unpacks a book to show its shelf.
- **Only what is on screen.** Every chapter, picture, diagram and formula is
  one independent frame. Jumping to chapter 40 decodes chapter 40, at more
  than a gigabyte per second.
- **Pages that do not jump.** The size of every picture is known before one
  is decoded, so the page is laid out once.
- **Formulas and diagrams that stay sharp.** They are stored as KaTeX and
  Mermaid source, not as pictures of it, and are set in the reader's own
  size and tone.
- **Simple to show.** Chapters are GitHub Flavored Markdown without raw
  HTML. No CSS, no scripts, no web engine: a reader is a Markdown view and
  an AVIF decoder.
- **Nothing to parse.** Metadata and navigation are typed columns, not
  JSON, XML or TOML.
- **Hard to break.** Every resource has a checksum, a book carries no code
  to run, and the library and its container are written in Rust without
  `unsafe`.
- **Nothing to lose by trying.** The converter imports eight formats and
  exports the same eight. Unpacked, a book is a folder any Markdown viewer
  can show, and every frame is an ordinary [OpenZL] frame that a stock
  decoder reads.

### What gets in the way elsewhere

| Format | What gets in the way |
| --- | --- |
| EPUB | A ZIP of XHTML and CSS: showing it correctly needs a web engine. |
| FB2 | One XML document: nothing is readable without parsing all of it, and pictures are Base64 text. |
| PDF | Fixed pages: the text does not reflow to the screen it is read on. |
| Markdown in a folder | Easy to write and to render, but not one file, with no metadata and no index. |

mbook keeps the Markdown-and-pictures model and packs it into the *framed*
container of [mopenzl], which gives random access, a checksum per resource
and typed columns.

### Measured

| | EPUB | mbook |
| --- | ---: | ---: |
| File | 7,001,213 B | 1,608,479 B |
| Pictures | 6,705,021 B | 1,382,138 B |
| Text, metadata and navigation | 247,429 B | 226,341 B |
| Title and contents ready | 164 µs with `rbook` | 80 µs |
| Largest chapter decoded | not measured | 108 µs for 175 KB |

A technical book of 11 chapters and 45 pictures, converted at the defaults
of `mbook-converter` on an Apple M5 Pro. The pictures shrink because AVIF at
quality 80 is lossy; a book without pictures shrinks far less. The full
table and the conditions are in [RFC 0001][measured].

## Try it

- **Read.** Install the application from
  [GitHub Releases](https://github.com/M-Compressors/mbook/releases) and
  import an EPUB, FB2 or PDF: it becomes an mbook file in your library.
- **Convert.** A few lines of Rust turn a book into mbook; see
  [The converter](#the-converter).
- **Build on it.** `cargo add mbook` and read a book lazily; see
  [The library](#the-library).

## Status

The format and APIs are drafts and may still change.

| Part | State |
| --- | --- |
| File format, version 1.0 | Draft: [RFC 0001][format], with its [decision records][adr] |
| `mbook` | Implements the whole of RFC 0001 and of [its API][lib-api]: lazy reading, loading into memory, creating, editing, checking and writing, including chapter anchor tables and diagram and formula sources. Tested against files built by hand and against real AVIF pictures. Available on [crates.io](https://crates.io/crates/mbook). |
| `mbook-ui` | A first version of [the application][app]: library and book creation, filtered import and export from the book menu, sharing a book's file through the system share sheet, a scrolling reader with optional pagination that opens a new book where its text begins and lists its landmarks beside the contents, page-fitted pictures and a full-window picture viewer with zoom, local appearance settings, an interface in English and Russian that follows the language of the system or the one chosen in the settings, bundled offline fonts, and whole-book search, an editor with chapter search and a chapter list by title, with a default Visual view for tables, lists, code, pictures, diagrams and formulas, dialogs that link to a place of the book and put a picture into a chapter, a Contents section that shows the contents and edits a stored list entry by entry, a Landmarks section that marks where the text begins and the other places a reader may look for, a picture gallery with encoding controls, syntax-highlighted diagram and formula editing and previews, settings, release updates with a banner below the title bar, and an MCP server for agents, off until turned on in the settings, through which an agent writes and edits the books of the library without looking at the screen ([Agents (MCP)][app-mcp]). Building for x86 or x86-64 requires NASM. Tested in headless windows and on macOS with an imported book. An Android build, on the platform of gpui-mobile, starts and shows the library on a phone; picking, exporting and sharing books there are untried. Remaining platform and accessibility checks are listed in [its status page][app-status]. Installers for macOS and Windows and an APK for Android are available from [GitHub Releases](https://github.com/M-Compressors/mbook/releases). |
| `mbook-converter` | Implements [its API][converter-api]: imports EPUB, FB2, PDF, comic archives, Word, HTML, Markdown and plain text. Imported pictures are encoded as AVIF. The optional `export` feature writes all eight formats, with Mermaid diagrams and KaTeX formulas as PNG pictures, the landmarks of a book in EPUB and lists numbered by Word in DOCX, and is enabled in the app; it also draws figures as SVG and PNG for the app. `inline::extract` moves Mermaid fences and `$…$` of a Markdown text into diagram and formula resources; the Markdown import does not use it yet. The limits of each format are listed in [its conversion rules][converter-status]. Tested against sources its tests build in memory; not yet against a collection of real books. Available on [crates.io](https://crates.io/crates/mbook-converter). |

## What is in a file

```text
resource 0   format      "MBOOK\0", major, minor
resource 1   meta        key, value
resource 2   resources   kind, path, width, height      one row per resource
resource 3   toc         level, title, resource, offset
resource 4   anchors     resource, offset, name         heading anchors
resource 5…  landmarks, when the book has any; the cover picture; then
             chapters, then pictures, diagrams and formulas;
             a chapter that names other lines follows its own anchor table
index + footer                                          provided by the container
```

The complete rules are in [RFC 0001][format].

## The library

`mbook` has two central types. `BookReader` reads a book lazily from
anything that can seek; `Book` is a whole book in memory that can be changed
and written out. More examples are in its [README](crates/mbook/README.md)
and its [guide][lib-guide]; every type and call is in the
[API reference][lib-api].

```rust
use mbook::BookReader;

let mut book = BookReader::open("example.mbook")?;
println!("{}", book.meta().title());

for entry in book.toc() {
    println!("{}{}", "  ".repeat(usize::from(entry.level) - 1), entry.title);
}
let first = book.chapters().next().map(|chapter| chapter.id);
if let Some(first) = first {
    let markdown = book.read_chapter(first)?;
}
```

```rust
use mbook::Book;

let mut book = Book::new("The Example", "en")?;
book.meta_mut().add_author("A. Writer")?;
book.set_cover("img/cover.avif", std::fs::read("img/cover.avif")?)?;
book.add_chapter("ch01.md", std::fs::read_to_string("ch01.md")?)?;
book.save("example.mbook")?;
```

| Cargo feature | Default | Enables |
| --- | --- | --- |
| `read` | yes | `BookReader`, `Book::read`, `Book::open` |
| `write` | yes | `Book::new` and every call that changes, checks or saves a `Book` |

Requires **Rust 1.89 or later**.

## The converter

`mbook-converter` takes a book as any `Read` or as a byte slice and writes an
mbook file, or hands back a `Book` that can still be changed. More examples
are in its [README](crates/mbook-converter/README.md) and its
[guide][converter-guide]; what a conversion keeps and drops is in the
[conversion rules][converter-rules].

```rust
use mbook_converter::Converter;
use std::fs::File;
use std::io::BufWriter;

let source = File::open("book.epub")?;
let sink = BufWriter::new(File::create("book.mbook")?);
let report = Converter::new().convert(source, sink)?;
for warning in &report.warnings {
    eprintln!("{warning}");
}
```

Every source format is one Cargo feature, all on by default: `epub`, `fb2`,
`pdf`, `cbz`, `docx`, `html`, `md`, `txt`. The optional `export` feature
writes a `Book` in any of the same eight formats.

## The application

`mbook-ui` is a library of books, a reader and an editor in one window.
Install it from [GitHub Releases](https://github.com/M-Compressors/mbook/releases)
or run it from source; its [README](crates/mbook-ui/README.md) and its
[guide][app-guide] say what each screen does.

```sh
cargo run -p mbook-ui
```

## Documentation

The [site] is built from [`docs/`](docs/). It keeps two kinds of document
apart:

| | Where | What it is |
| --- | --- | --- |
| **The format** | [RFC 0001][format] and its [decision records][adr] | The specification: what a file must contain, and why. It binds every implementation. |
| **The crates** | [`mbook`][lib], [`mbook-converter`][converter], [`mbook-ui`][app] | A guide, a reference and the design of each crate. They describe this code and change with it. |

The README of each crate holds its examples and a summary of those pages.

## Repository layout

| Path | Contents |
| --- | --- |
| [`docs/`](docs/) | The documentation, an mdBook published at the [site]: the format under [`rfcs/`](docs/src/rfcs/), and one folder of pages per crate. |
| [`crates/mbook/`](crates/mbook/) | The library. |
| [`crates/mbook-converter/`](crates/mbook-converter/) | The converter from and to EPUB, FB2, PDF and other formats. |
| [`crates/mbook-ui/`](crates/mbook-ui/) | The reading application, built with GPUI Kit. |
| [`crates/mbook-android/`](crates/mbook-android/) | The Android shell of the application and the script that builds its APK. |

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
mdbook serve docs --open
```

The documentation needs [mdBook] and [mdbook-mermaid]; run
`mdbook-mermaid install docs` once before the first build.

## Contributing

Bug reports, format feedback and code are welcome. See
[CONTRIBUTING.md](CONTRIBUTING.md) for how changes are made and checked, the
[Code of Conduct](CODE_OF_CONDUCT.md), and [SECURITY.md](SECURITY.md) for
reporting vulnerabilities privately.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Unless you explicitly state otherwise, any contribution intentionally
submitted for inclusion in the work by you, as defined in the Apache-2.0
license, shall be dual licensed as above, without any additional terms or
conditions.

[ci]: https://github.com/M-Compressors/mbook/actions/workflows/ci.yml
[site]: https://m-compressors.github.io/mbook/
[format]: https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html
[adr]: https://m-compressors.github.io/mbook/rfcs/adr/overview.html
[measured]: https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html#a-converted-book
[lib]: https://m-compressors.github.io/mbook/mbook/index.html
[lib-guide]: https://m-compressors.github.io/mbook/mbook/guide.html
[lib-api]: https://m-compressors.github.io/mbook/mbook/api.html
[converter]: https://m-compressors.github.io/mbook/mbook-converter/index.html
[converter-guide]: https://m-compressors.github.io/mbook/mbook-converter/guide.html
[converter-api]: https://m-compressors.github.io/mbook/mbook-converter/api.html
[converter-rules]: https://m-compressors.github.io/mbook/mbook-converter/conversion.html
[converter-status]: https://m-compressors.github.io/mbook/mbook-converter/conversion.html#what-is-implemented
[app]: https://m-compressors.github.io/mbook/mbook-ui/index.html
[app-guide]: https://m-compressors.github.io/mbook/mbook-ui/guide.html
[app-status]: https://m-compressors.github.io/mbook/mbook-ui/status.html#what-is-implemented
[app-mcp]: https://m-compressors.github.io/mbook/mbook-ui/mcp.html
[mopenzl]: https://github.com/M-Compressors/mopenzl
[OpenZL]: https://github.com/facebook/openzl
[mdBook]: https://github.com/rust-lang/mdBook
[mdbook-mermaid]: https://github.com/badboy/mdbook-mermaid
