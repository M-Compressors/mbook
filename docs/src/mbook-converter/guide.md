# Using mbook-converter

The examples leave out the surrounding function; each `?` returns an
[`mbook_converter::Error`](api.md#errors-and-warnings).

## Converting a file

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

The source may be anything that implements `Read`: a file, standard input, a
network stream, an entry of an archive. It is never asked to seek.

## Converting bytes that are already in memory

```rust
let bytes: &[u8] = /* an upload, a memory-mapped file, include_bytes! */;
let mut mbook = Vec::new();

Converter::new().convert_slice(bytes, &mut mbook)?;
```

`convert_slice` exists because a slice can be read at any position: formats
that need random access use it in place, where a `Read` has to be buffered
first ([Sources and memory](conversion.md#sources-and-memory)).

## Correcting the book before it is written

```rust
use std::io;

let mut converted = Converter::new().load(io::stdin().lock())?;

let book = &mut converted.book;
book.meta_mut().set_title("The Example")?;
book.meta_mut().set_language("en")?;
book.save("example.mbook")?;
```

`converted.book` is an ordinary [`Book`](../mbook/api.md#book): every
call of `mbook` works on it.

## Exporting a book

```rust
use mbook_converter::{Converter, Format, mbook::Book};
use std::fs::File;
use std::io::BufWriter;

let book = Book::open("example.mbook")?;
let sink = BufWriter::new(File::create("example.epub")?);
Converter::export(&book, Format::Epub, sink)?;
```

Enable the converter’s `export` feature to use this call. An application can
pass a book already in memory and use `Format::extension()` for the suggested
file name. `Format::is_enabled()` reports which import formats are enabled.
Diagrams and formulas are written as pictures; one that cannot be drawn
becomes its alternative text.

## Drawing a diagram or a formula

```rust
use mbook_converter::figure::{self, Palette, Setting};

let night = Palette { ink: 0xb8b8b8, page: 0x000000 };
let diagram = figure::diagram("flowchart LR\n  A --> B", night)?;
let formula = figure::formula(r"v = \frac{s}{t}", Setting::Line, night)?;

// Scale both to text of 18 pixels.
let width = formula.width() * 18.0 / formula.em();
std::fs::write("flow.svg", diagram.svg())?;
```

The same `export` feature enables `figure`. A reader gives the colours of
its page and scales the figure to its text; a formula inside a line is set
with `Setting::Line` and placed on the line at `baseline()`.

## Choosing formats and settings

```toml
# Cargo.toml: only EPUB and FB2
mbook-converter = { version = "0.3", default-features = false, features = ["epub", "fb2"] }
```

```rust
use mbook_converter::{ConvertOptions, Converter, Format};

let mut options = ConvertOptions::default();
options.format = Some(Format::Fb2);      // do not detect
options.pictures.quality = 60;           // smaller pictures

let converter = Converter::with_options(options);
```

A source in a format whose feature is off is still recognised, so the error
names it:

```rust
match converter.convert_slice(bytes, &mut mbook) {
    Err(Error::FormatDisabled(format)) => eprintln!("built without support for {format}"),
    other => { other?; }
}
```

## What is lost

A conversion that succeeds may still have dropped something: a table with
merged cells, a picture in a format that cannot be decoded, a link into the
middle of a paragraph. Each loss is one `Warning` in the report, with the
place in the source where the front end knows it. An application decides
whether to show them.
