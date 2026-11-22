# Using mbook

Every task below takes a few lines. The examples leave out the surrounding
function; each `?` returns an [`mbook::Error`](api.md#errors). The same
examples are compiled as the doc tests of the crate, where they are complete.

## Showing a book in a library

```rust
use mbook::BookReader;

let mut book = BookReader::open("example.mbook")?;
let meta = book.meta();
println!("{} by {}", meta.title(), meta.authors().collect::<Vec<_>>().join(", "));

if let Some(avif) = book.read_cover()? {
    // hand the bytes to an AVIF decoder
}
```

`open` reads the footer, the directory and resources 0 to 4. `read_cover`
reads one more frame.

## Reading a chapter

```rust
use mbook::{BookReader, Destination};

for entry in book.toc() {
    println!("{}{}", "  ".repeat(entry.level as usize - 1), entry.title);
}

let first = book.chapters().next().expect("a book with text").id;
let text = book.read_chapter(first)?;

// While rendering: what does this link or picture destination point to?
match book.resolve(first, "img/fig-1.avif") {
    Ok(Destination::Picture(id)) => {
        let size = book.picture(id).map(|p| p.size);   // reserve space first
        let avif = book.read_picture(id)?;             // decode when visible
    }
    Ok(Destination::Diagram(id)) => {
        let mermaid = book.read_diagram(id)?;          // draw it, or show the alt text
    }
    Ok(Destination::Formula(id)) => {
        let katex = book.read_formula(id)?;            // set it, or show the alt text
    }
    Ok(Destination::Chapter(position)) => { /* jump to position */ }
    Ok(Destination::ExtraAnchor(chapter)) => { /* a place in `chapter`; see below */ }
    Ok(Destination::External) => { /* open only when the user asks */ }
    Err(_) => { /* show the link text or the alternative text */ }
}
```

`resolve` reads nothing, so it can be called for every link of a chapter
while it is laid out. A link to a heading, or to the start of a chapter, is
answered in full. A link to another named line, such as
`ch01.md#fig-1`, is answered with `Destination::ExtraAnchor`: the name may be
in the anchor table of that chapter, which is not read when a book is opened.
When the user follows the link, `read_resolve` reads that table:

```rust
match book.read_resolve(first, "ch01.md#fig-1")? {
    Destination::Chapter(position) => { /* jump to position */ }
    _ => { /* as above */ }
}
```

Methods that only look into the tables take `&self` and cannot fail with an
I/O error. Methods that touch the source start with `read_` and take
`&mut self`.

A chapter, a picture, a diagram and a formula have different id types, so
`read_chapter` cannot be called with the id of a picture. Ids come from the reader. To keep one outside
the program, for a bookmark, store its number and ask the reader for the id
again:

```rust
let saved = (position.chapter.number(), position.offset);
// … later, with the same book open
let chapter = book.chapter_id(saved.0);   // None: not a chapter of this book
```

## Reading from a source that cannot seek

```rust
use mbook::{Book, BookReader};
use std::io;

// The same lazy reader, over a buffer that holds the compressed file.
let mut book = BookReader::buffered(io::stdin().lock())?;

// Or everything decoded at once.
let book = Book::read(io::stdin().lock())?;
for chapter in book.chapters() {
    println!("{}: {} bytes", chapter.path(), chapter.text().len());
}
```

## Editing a book

```rust
use mbook::Book;

let mut book = Book::open("example.mbook")?;

book.meta_mut().set_title("The Example, 2nd edition")?;
book.meta_mut().add_author("B. Editor")?;

if let Some(chapter) = book.chapter_mut("ch02.md") {
    let fixed = chapter.text().replace("teh", "the");
    chapter.set_text(fixed)?;
}
book.remove_picture("img/unused.avif");

book.save("example.mbook")?;
```

`save` writes a temporary file next to the target and renames it into place,
so an interrupted save never damages the original. The byte offsets that the
contents and the anchors need are recomputed for `ch02.md`; nothing has to be
rebuilt by hand.

There is no rename. A resource moves to another path by being removed and
added again; links to the old path are then reported when the book is saved.

```rust
let index = book.chapters().iter().position(|c| c.path() == "ch02.md").unwrap();
let old = book.remove_chapter("ch02.md").unwrap();
book.insert_chapter(index, "part1/ch02.md", old.text())?;
```

## Creating a book

```rust
use mbook::{Book, Toc, TocItem};
use std::fs;

let mut book = Book::new("The Example", "en")?;
book.meta_mut().add_author("A. Writer")?;

book.set_cover("img/cover.avif", fs::read("img/cover.avif")?)?;
book.add_chapter("ch01.md", fs::read_to_string("ch01.md")?)?;
book.add_chapter("ch02.md", fs::read_to_string("ch02.md")?)?;
book.add_picture("img/fig-1.avif", fs::read("img/fig-1.avif")?)?;

book.save("example.mbook")?;
```

- `Book::new` gives the book a new UUID version 7 as its `id`.
- Chapters are in reading order in the order they are added.
- CRLF and CR line endings become line feeds and a byte order mark is dropped;
  `chapter.text()` returns the text as it will be stored.
- The pixel size of a picture is read from the AVIF file.
- By default the contents are built from the headings. To choose the entries,
  name them the way a link would:

```rust
book.set_toc(Toc::Explicit(vec![
    TocItem::new(1, "Arrival", "ch01.md"),
    TocItem::new(2, "The station", "ch01.md#the-station"),
    TocItem::new(1, "The letter", "ch02.md"),
]));
```

## Diagrams and formulas

A diagram is added as its description in Mermaid notation and a formula in
the notation of KaTeX. A chapter shows either the way it shows a picture:

```rust
book.add_diagram("dia/route.mmd", "flowchart LR\n  station --> letter\n")?;
book.add_formula("math/speed.tex", r"v = \frac{s}{t}")?;
book.add_chapter("ch03.md", "\
![From the station to the letter](dia/route.mmd \"The way\")

The train kept ![v = s / t](math/speed.tex) all the way.
")?;

// Later: correct the formula without touching the chapter.
book.formula_mut("math/speed.tex").unwrap().set_source(r"v = \frac{\Delta s}{\Delta t}")?;
```

- The source is stored as text, with the line endings of a chapter. The
  notation is not checked: a reading application shows the alternative text
  of what it cannot render.
- A reference with the syntax of a picture resolves when it names a picture,
  a diagram or a formula. A link to one of them does not.

## Naming a line that is not a heading

Headings get their anchors by themselves. Any other line gets a name from one
line of Markdown in front of it, an *anchor definition*:

```rust
book.add_chapter("ch01.md", "\
# Chapter 1

[fig-3]: #fig-3
![View of the pond](img/fig-3.avif \"Fig. 3\")
")?;

// In another chapter: [the figure](ch01.md#fig-3)
```

A Markdown renderer shows nothing for `[fig-3]: #fig-3`. The name is part of
the text, so it is still there after the text is edited:

```rust
let chapter = book.chapter_mut("ch01.md").unwrap();
chapter.set_text(chapter.text().replace("teh", "the"))?;
assert_eq!(chapter.extra_anchors()[0].name, "fig-3");
```

## When something is wrong

Rules about one item are checked when the item is given, so the error points
at the call that caused it. Rules that connect items are checked when the book
is written, because a book under construction is allowed to be incomplete.

```rust
use mbook::{Error, ProblemKind};

match book.save("example.mbook") {
    Err(Error::Invalid(problems)) => {
        for p in problems.iter() {
            if let ProblemKind::UnresolvedLink(dest) = &p.kind {
                eprintln!("{:?}, byte {:?}: link to {dest} leads nowhere", p.path, p.offset);
            }
        }
    }
    other => other?,
}
```

`Book::check` runs the same checks without writing and reports every problem,
not only the first.
