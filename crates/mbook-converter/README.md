# mbook-converter

Convert books in other formats into [mbook](https://m-compressors.github.io/mbook/)
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
[`mbook-core`](https://crates.io/crates/mbook-core) that can still be
changed:

```rust
let mut converted = Converter::new().load_slice(&bytes)?;
converted.book.meta_mut().set_title("The Example")?;
converted.book.save("example.mbook")?;
```

## Formats

Every source format is one Cargo feature, and all of them are on by default.

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

Requires **Rust 1.89 or later**.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.
