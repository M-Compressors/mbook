# mbook-core

**Read, edit and create mbook files.**

An mbook file holds a book as Markdown chapters and AVIF pictures in one
indexed container. `mbook-core` is the Rust library for it:

- `BookReader` reads a book lazily from anything that can seek. Opening it
  reads the tables; each chapter and picture is decoded when asked for.
- `Book` is a whole book in memory. It is how a book is created and how an
  existing one is changed.

```rust
use mbook_core::Book;

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
- [The design of this API](https://m-compressors.github.io/mbook/0002-mbook-core-api.html)

## License

Licensed under either of the Apache License, Version 2.0, or the MIT license,
at your option.
