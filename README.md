# mbook

**A book file format: Markdown chapters and AVIF pictures in one indexed
container.**

An mbook file opens without being read in full, decodes only what is on
screen, and needs neither a web engine nor an XML parser to show. This
repository holds the format specification, `mbook-core`, the Rust library
that reads and writes it, and `mbook-converter`, which turns books in other
formats into mbook.

[![Tests](https://github.com/M-Compressors/mbook/actions/workflows/ci.yml/badge.svg?branch=main)][ci]
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

**[Design documents][site]** · [File format][format] · [Library API][api] · [Decisions][adr] · [Changelog](CHANGELOG.md)

## Status

mbook is not released yet, and the format may still change.

| Part | State |
| --- | --- |
| File format, version 1.0 | Draft: [RFC 0001][format] |
| Library API | Draft: [RFC 0002][api] |
| Converter API | Draft: [RFC 0003][converter] |
| `mbook-core` | Implements the whole of RFC 0001 and RFC 0002: lazy reading, loading into memory, creating, editing, checking and writing, including anchor tables of chapters. Tested against files built by hand and against real AVIF pictures. Not published to crates.io. |
| `mbook-converter` | Implements RFC 0003: EPUB, FB2, PDF, comic archives, Word, HTML, Markdown and plain text are converted into mbook, with pictures encoded as AVIF. The limits of each format are listed in [RFC 0003][converter-status]. Tested against sources its tests build in memory; not yet against a collection of real books. Not published to crates.io. |

## Why another book format

| Format | What gets in the way |
| --- | --- |
| EPUB | A ZIP of XHTML and CSS: showing it correctly needs a web engine. |
| FB2 | One XML document: nothing is readable without parsing all of it, and pictures are Base64 text. |
| Markdown in a folder | Easy to write and to render, but not one file, with no metadata and no index. |

mbook keeps the Markdown-and-pictures model and packs it into the *framed*
container of [mopenzl], which gives random access, a checksum per resource
and typed columns:

- A reader fetches the directory and five small tables, then shows the title,
  the cover and the contents.
- Every chapter and every picture is one independent [OpenZL] frame, decoded
  on its own.
- Metadata and navigation are typed columns, so there is no JSON, XML or TOML
  to parse.
- Chapters are GitHub Flavored Markdown without raw HTML; unpacked, a book is
  a folder any Markdown viewer can show.

## What is in a file

```text
resource 0   format      "MBOOK\0", major, minor
resource 1   meta        key, value
resource 2   resources   kind, path, width, height      one row per resource
resource 3   toc         level, title, resource, offset
resource 4   anchors     resource, offset, name         heading anchors
resource 5…  cover picture, then chapters, then pictures;
             a chapter that names other lines follows its own anchor table
index + footer                                          provided by the container
```

The complete rules are in [RFC 0001][format].

## The library

`mbook-core` has two central types. `BookReader` reads a book lazily from
anything that can seek; `Book` is a whole book in memory that can be changed
and written out. The API is described in [RFC 0002][api].

```rust
use mbook_core::BookReader;

let mut book = BookReader::open("example.mbook")?;
println!("{}", book.meta().title());

for entry in book.toc() {
    println!("{}{}", "  ".repeat(usize::from(entry.level) - 1), entry.title);
}
if let Some(first) = book.chapters().next().map(|chapter| chapter.id) {
    let markdown = book.read_chapter(first)?;
}
```

```rust
use mbook_core::Book;

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
mbook file, or hands back a `Book` that can still be changed. Its design is
[RFC 0003][converter].

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
`pdf`, `cbz`, `docx`, `html`, `md`, `txt`.

## Repository layout

| Path | Contents |
| --- | --- |
| [`rfcs/`](rfcs/) | The design documents, an mdBook published at the [site]. |
| [`crates/mbook-core/`](crates/mbook-core/) | The library. |
| [`crates/mbook-converter/`](crates/mbook-converter/) | The converter from EPUB, FB2, PDF and other formats. |

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
cargo test --workspace --all-features --locked
mdbook serve rfcs --open
```

The design documents need [mdBook] and [mdbook-mermaid]; run
`mdbook-mermaid install rfcs` once before the first build.

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
[format]: https://m-compressors.github.io/mbook/0001-mbook-format.html
[api]: https://m-compressors.github.io/mbook/0002-mbook-core-api.html
[converter]: https://m-compressors.github.io/mbook/0003-mbook-converter-api.html
[converter-status]: https://m-compressors.github.io/mbook/0003-mbook-converter-api.html#what-is-implemented
[adr]: https://m-compressors.github.io/mbook/adr/overview.html
[mopenzl]: https://github.com/M-Compressors/mopenzl
[OpenZL]: https://github.com/facebook/openzl
[mdBook]: https://github.com/rust-lang/mdBook
[mdbook-mermaid]: https://github.com/badboy/mdbook-mermaid
