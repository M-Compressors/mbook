# mbook-ui

A reading application for [mbook](https://github.com/M-Compressors/mbook)
files: a library of books, a reader and an editor, built with GPUI Kit.

- **Your books, smaller.** Import an EPUB, FB2, PDF, comic archive, Word,
  HTML, Markdown or text file and it becomes an mbook file; an illustrated
  7.0 MB EPUB [became 1.6 MB][measured]. Export gives any of the same
  formats back.
- **No waiting.** A book opens from five small tables and each chapter is
  decoded on its own, off the main thread.
- **No web engine inside.** Chapters are Markdown, pictures are AVIF,
  formulas and diagrams are drawn from their source in the size and tone of
  the page.
- **Reader and editor in one.** What the editor previews is what the reader
  shows.

The design is [RFC 0004](https://m-compressors.github.io/mbook/0004-mbook-ui.html).
The crate is under construction; the status table of the repository's README
says what exists.

```sh
cargo run -p mbook-ui
```

Requires **Rust 1.100 or later**. The AVIF decoder requires NASM at build
time on x86 and x86-64.

## License

Licensed under either of the Apache License, Version 2.0 or the MIT license,
at your option.

[measured]: https://m-compressors.github.io/mbook/0001-mbook-format.html#a-converted-book
