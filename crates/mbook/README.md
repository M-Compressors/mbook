# mbook

**Read, edit and create mbook files: books a quarter of the size that open
in a tenth of a millisecond.**

[![crates.io](https://img.shields.io/crates/v/mbook.svg)](https://crates.io/crates/mbook)
[![docs.rs](https://docs.rs/mbook/badge.svg)](https://docs.rs/mbook)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)

An mbook file holds a book as Markdown chapters and AVIF pictures in one
indexed container, compressed with OpenZL. Measured on one illustrated
manual, [as described in the format's RFC][measured]:

- **4× smaller.** A 7.0 MB EPUB became a 1.6 MB mbook file.
- **80 µs to open.** Title, contents and the list of every resource come
  from five small typed tables: no XML, no JSON, nothing to unpack.
- **1 GB/s.** Each chapter is its own frame and decodes on its own; the
  longest took 0.1 ms.
- **No web engine.** Chapters are GitHub Flavored Markdown without raw
  HTML; a reader is a Markdown view and an AVIF decoder.
- **Safe Rust.** This crate and its container, `mopenzl`, forbid `unsafe`,
  and every length read from a file is bounded before memory is allocated.

**[Guide][guide]** · [API reference][api] · [Design][design] · [docs.rs](https://docs.rs/mbook) · [File format][format]

## Install

```sh
cargo add mbook
```

Requires Rust 1.89 or later.

## The two types

| | `BookReader<R>` | `Book` |
| --- | --- | --- |
| What it is | A lazy, read-only view of a file | A whole book in memory that can be changed |
| Holds | The tables and the source | Every chapter, picture, diagram and formula, decoded |
| Memory | Kilobytes for a novel | The decoded size of the book |
| Addresses a resource by | A typed id: `ChapterId`, `PictureId`, `DiagramId`, `FormulaId` | Its path |
| Typical user | A reading application | A converter or an editor |

A source that cannot seek is served by both: `BookReader::buffered` keeps
the compressed bytes in memory and still decodes on demand, and `Book::read`
decodes everything in one pass.

## Examples

The examples leave out the surrounding function; each `?` returns an
`mbook::Error`.

### Show a book on a shelf

Opening reads the footer, the directory and five small tables; nothing else
is decoded until it is asked for.

```rust
use mbook::BookReader;

let mut book = BookReader::open("example.mbook")?;

let meta = book.meta();
println!("{} by {}", meta.title(), meta.authors().collect::<Vec<_>>().join(", "));

if let Some(avif) = book.read_cover()? {
    // hand the bytes to an AVIF decoder
}
```

### Read a chapter and what it refers to

`resolve` reads nothing, so it can be called for every link and picture of
a chapter while it is laid out.

```rust
use mbook::{BookReader, Destination};

let mut book = BookReader::open("example.mbook")?;

for entry in book.toc() {
    println!("{}{}", "  ".repeat(usize::from(entry.level) - 1), entry.title);
}

let Some(first) = book.chapters().next().map(|chapter| chapter.id) else {
    return Ok(());
};
let markdown = book.read_chapter(first)?;

match book.resolve(first, "img/fig-1.avif") {
    Ok(Destination::Picture(id)) => {
        let size = book.picture(id).map(|picture| picture.size); // reserve space first
        let avif = book.read_picture(id)?;                       // decode when visible
    }
    Ok(Destination::Diagram(id)) => {
        let mermaid = book.read_diagram(id)?;
    }
    Ok(Destination::Formula(id)) => {
        let katex = book.read_formula(id)?;
    }
    Ok(Destination::Chapter(position)) => { /* jump to position */ }
    Ok(Destination::ExtraAnchor(_)) => {
        // A named line that is not a heading: its table is read on request.
        let destination = book.read_resolve(first, "ch01.md#fig-1")?;
    }
    Ok(_) => { /* an external link: open it only when the user asks */ }
    Err(_) => { /* show the link text or the alternative text */ }
}
```

### Read from a stream

```rust
use mbook::{Book, BookReader};
use std::io;

// The same lazy reader, over a buffer that holds the compressed file.
let mut book = BookReader::buffered(io::stdin().lock())?;
println!("{}", book.meta().title());
```

```rust
use mbook::Book;
use std::io;

// Or everything decoded at once.
let book = Book::read(io::stdin().lock())?;
for chapter in book.chapters() {
    println!("{}: {} bytes", chapter.path(), chapter.text().len());
}
```

### Create a book

```rust
use mbook::{Book, Toc, TocItem};
use std::fs;

let mut book = Book::new("The Example", "en")?;
book.meta_mut().add_author("A. Writer")?;

book.set_cover("img/cover.avif", fs::read("img/cover.avif")?)?;
book.add_chapter("ch01.md", "# Arrival\n\n## The station\n\nText.\n")?;
book.add_chapter("ch02.md", "# The letter\n\n![The pond](img/fig-1.avif)\n")?;
book.add_picture("img/fig-1.avif", fs::read("img/fig-1.avif")?)?;

// Optional: by default the contents are built from the headings.
book.set_toc(Toc::Explicit(vec![
    TocItem::new(1, "Arrival", "ch01.md"),
    TocItem::new(2, "The station", "ch01.md#the-station"),
    TocItem::new(1, "The letter", "ch02.md"),
]));

book.save("example.mbook")?;
```

### Edit a book

Editing always goes through a `Book`; `save` writes a temporary file next
to the target and renames it into place.

```rust
use mbook::Book;

let mut book = Book::open("example.mbook")?;

book.meta_mut().set_title("The Example, 2nd edition")?;
if let Some(chapter) = book.chapter_mut("ch02.md") {
    let fixed = chapter.text().replace("teh", "the");
    chapter.set_text(fixed)?;
}
book.remove_picture("img/unused.avif");

book.save("example.mbook")?;
```

### Diagrams, formulas and named lines

A diagram is stored as Mermaid source and a formula as KaTeX source; a
chapter shows either the way it shows a picture. Any line gets a name from
an *anchor definition* in front of it.

```rust
use mbook::Book;

let mut book = Book::new("The Example", "en")?;
book.add_diagram("dia/route.mmd", "flowchart LR\n  station --> letter\n")?;
book.add_formula("math/speed.tex", r"v = \frac{s}{t}")?;
book.add_chapter("ch01.md", "\
# Chapter 1

[route]: #route
![From the station to the letter](dia/route.mmd \"The way\")

The train kept ![v = s / t](math/speed.tex) all the way.
")?;

// In another chapter: [the route](ch01.md#route)
```

### When something is wrong

Rules about one argument are checked by the call that receives it. Rules
that connect several parts of a book are checked when it is written, and
reported together.

```rust
use mbook::{Book, Error, ProblemKind};

let book = Book::open("example.mbook")?;
match book.save("example.mbook") {
    Err(Error::Invalid(problems)) => {
        for problem in problems.iter() {
            if let ProblemKind::UnresolvedLink(destination) = &problem.kind {
                eprintln!("{:?}: link to {destination} leads nowhere", problem.path);
            }
        }
    }
    other => other?,
}
```

`Book::check` runs the same checks without writing.

## What to know

- **Lookups and reads are told apart by name.** A call that only looks into
  the tables takes `&self` and cannot fail with an I/O error. A call that
  touches the source starts with `read_` and takes `&mut self`.
- **Ids come from the reader.** A `ChapterId` cannot be passed where a
  `PictureId` is wanted, and neither has a public constructor. An id is a
  number inside one file; the stable name of a resource is its path.
- **Navigation is computed from the text.** Heading anchors, named lines
  and the offsets of the contents are derived when a chapter is added or
  changed, so an edit cannot leave them stale.
- **An invalid book cannot be written.** `write` and `save` run `check`
  first and write nothing when it fails.
- **Text is normalised, not repaired.** CRLF and CR become line feeds and a
  byte order mark is dropped. Raw HTML and control characters are reported.
- **Rendering is the application's job.** Chapters are returned as
  Markdown, pictures as AVIF bytes, diagrams and formulas as their source.
- **No dependency type is public.** `mopenzl`, `markdown` and `uuid` stay
  behind the crate's own types and errors.

### Limits

Every length and count read from a file is bounded before memory is
allocated. `ReadOptions` and `WriteOptions` carry the limits.

| Limit | `Limits::default()` |
| --- | ---: |
| `max_resources` | 1,000,000 |
| `max_resource_bytes`, the decoded size of one resource | 64 MiB |
| `max_file_bytes` | 16 GiB |

### Cargo features

| Feature | Default | Enables |
| --- | --- | --- |
| `read` | yes | `BookReader`, `Book::read`, `Book::open` |
| `write` | yes | `Book::new` and every call that changes, checks or saves a `Book` |

At least one of them must be enabled. A reading application that builds
with `read` alone gets no Markdown parser from this crate.

## Status

Implements version 1.0 of the format as drafted: lazy reading, loading into
memory, creating, editing, checking and writing. The format is still a draft
and may change, and the API with it.

## Documentation

- [Guide][guide]: every task with its code
- [API reference][api]: each type and call, what it reads and when it fails
- [Design][design]: dependencies, costs and the alternatives that were
  rejected
- [The file format][format]
- [`mbook-converter`](https://crates.io/crates/mbook-converter) turns EPUB,
  FB2, PDF and five other formats into mbook, and back

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

[guide]: https://m-compressors.github.io/mbook/mbook/guide.html
[api]: https://m-compressors.github.io/mbook/mbook/api.html
[design]: https://m-compressors.github.io/mbook/mbook/design.html
[format]: https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html
[measured]: https://m-compressors.github.io/mbook/rfcs/0001-mbook-format.html#a-converted-book
