# mbook-converter

Convert books between other formats and [mbook](https://m-compressors.github.io/mbook/)
files: Markdown chapters and AVIF pictures in one indexed container.

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

A source is any `Read` or a byte slice. The result is an mbook file written
to any `Write`, or a `Book` of
[`mbook`](https://crates.io/crates/mbook) that can still be
changed:

```rust
let mut converted = Converter::new().load_slice(&bytes)?;
converted.book.meta_mut().set_title("The Example")?;
converted.book.save("example.mbook")?;
```

## Exporting a book

```rust
use mbook_converter::{Converter, Format, mbook::Book};

let book = Book::open("example.mbook")?;
let mut epub = Vec::new();
Converter::export(&book, Format::Epub, &mut epub)?;
```

Add `features = ["export"]` to the converter dependency to export in any of
the eight formats, including pictures where supported. Use any `Write` as
the destination. An export-only build can set `default-features = false`. [RFC 0003](https://m-compressors.github.io/mbook/0003-mbook-converter-api.html#export)
describes what each output preserves.

## Formats

Each import format has a Cargo feature enabled by default. The optional
`export` feature enables every output format.

| Feature | Format | Read by |
| --- | --- | --- |
| `epub` | EPUB 2 and 3 | `rbook`, `htmd` |
| `fb2` | FictionBook 2, plain or inside a ZIP | `quick-xml` |
| `pdf` | PDF; a scan becomes one picture per page | `unpdf` |
| `cbz` | Comic book archive | `zip`, `alphanumeric-sort` |
| `docx` | Word documents | `undoc` |
| `html` | One HTML document | `htmd` |
| `md` | One Markdown document | `markdown` |
| `txt` | Plain text | |

The format of a source is detected from its content. Markdown and plain text
have nothing to be recognised by and must be named:

```rust
use mbook_converter::{ConvertOptions, Converter, Format};

let mut options = ConvertOptions::default();
options.format = Some(Format::Markdown);
let converter = Converter::with_options(options);
```

## What a conversion does

- Chapters become GitHub Flavored Markdown without raw HTML.
- Pictures in JPEG, PNG, GIF, WebP or BMP are encoded as AVIF; AVIF pictures
  are kept as they are.
- Links inside the book keep their target, down to the paragraph.
- Whatever cannot be kept (a table with merged cells, a picture that cannot
  be decoded, a link that leads nowhere) is reported as a `Warning`.

The design is described by
[RFC 0003](https://m-compressors.github.io/mbook/0003-mbook-converter-api.html).

Requires **Rust 1.89 or later**. The optional `export` feature
currently requires **Rust 1.98 or later** and uses NASM on x86 and x86-64.
Its minimum Rust version can increase separately from the default build.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
