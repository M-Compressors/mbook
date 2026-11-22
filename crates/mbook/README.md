# mbook

**Read, edit and create mbook files: books a quarter of the size that open
in a tenth of a millisecond.**

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

`mbook` is the Rust library for the format:

- `BookReader` reads a book lazily from anything that can seek. Opening it
  reads the tables; each chapter and picture is decoded when asked for.
- `Book` is a whole book in memory. It is how a book is created and how an
  existing one is changed.

```rust
use mbook::BookReader;

let mut book = BookReader::open("example.mbook")?;
println!("{}", book.meta().title());
if let Some(first) = book.chapters().next().map(|chapter| chapter.id) {
    let markdown = book.read_chapter(first)?;
}
```

```rust
use mbook::Book;

let mut book = Book::new("The Example", "en")?;
book.meta_mut().add_author("A. Writer")?;
book.add_chapter("ch01.md", "# Chapter 1\n\nText.\n")?;
book.save("example.mbook")?;
```

## Status

Implements version 1.0 of the format as drafted: lazy reading, loading into
memory, creating, editing, checking and writing. The format is still a draft
and may change. Rendering Markdown and decoding AVIF are left to the
application: chapters are returned as text and pictures as AVIF bytes.

## Cargo features

| Feature | Default | Enables |
| --- | --- | --- |
| `read` | yes | `BookReader`, `Book::read`, `Book::open` |
| `write` | yes | `Book::new` and every call that changes, checks or saves a `Book` |

At least one of them must be enabled. Requires Rust 1.89 or later.

## Documentation

- [The file format](https://m-compressors.github.io/mbook/0001-mbook-format.html)
- [The design of this API](https://m-compressors.github.io/mbook/0002-mbook-api.html)
- [`mbook-converter`](https://crates.io/crates/mbook-converter) turns EPUB,
  FB2, PDF and five other formats into mbook, and back

[measured]: https://m-compressors.github.io/mbook/0001-mbook-format.html#a-converted-book

## License

Licensed under either of the Apache License, Version 2.0, or the MIT license,
at your option.
