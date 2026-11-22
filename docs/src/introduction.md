# mbook

mbook is a file format for books: chapters in Markdown, pictures in AVIF, and
small typed tables that describe the book, all stored as independent
[OpenZL](https://github.com/facebook/openzl) frames inside one indexed
container.

The pairing of OpenZL and AVIF is what makes a book small and quick: an
illustrated EPUB of 7.0 MB became an mbook file of 1.6 MB that opens in
80 µs and decodes its text at more than a gigabyte per second. The
conditions are in
[RFC 0001, A converted book](rfcs/0001-mbook-format.md#a-converted-book).

This site holds the specification of the format and the documentation of
the three crates built on it.

| Part | What it is | Start at |
| --- | --- | --- |
| **The format** | The specification every reader and writer is checked against, and the decisions behind it | [RFC 0001](rfcs/0001-mbook-format.md) |
| **`mbook`** | The Rust library that reads, edits and writes mbook files | [mbook](mbook/index.md) |
| **`mbook-converter`** | The Rust library that turns EPUB, FB2, PDF and five other formats into mbook, and back | [mbook-converter](mbook-converter/index.md) |
| **`mbook-ui`** | The application: a library of books, a reader and an editor | [mbook-ui](mbook-ui/index.md) |

## Where to start

| If you want to… | Read |
| --- | --- |
| see what the format gains over EPUB and FB2 | [RFC 0001, Motivation](rfcs/0001-mbook-format.md#motivation) |
| see what a file contains and how it is opened | [RFC 0001, guide-level explanation](rfcs/0001-mbook-format.md#guide-level-explanation) |
| implement a reader or a writer | [RFC 0001, reference-level explanation](rfcs/0001-mbook-format.md#reference-level-explanation) |
| understand why the format looks this way | [Decision records](rfcs/adr/overview.md) |
| read or write books from Rust | [Using mbook](mbook/guide.md) |
| look up a type or a call of the library | [mbook, API reference](mbook/api.md), or [docs.rs](https://docs.rs/mbook) |
| convert a book into mbook, or export one | [Using mbook-converter](mbook-converter/guide.md) |
| know what a conversion keeps and what it drops | [Conversion rules](mbook-converter/conversion.md) |
| read and edit books without writing code | [Using the application](mbook-ui/guide.md) |
| work on the application | [mbook-ui, Architecture](mbook-ui/architecture.md) |
| know what is still open | Unresolved questions of [RFC 0001](rfcs/0001-mbook-format.md#unresolved-questions), and "Settled and open questions" of [mbook](mbook/design.md#settled-and-open-questions), [mbook-converter](mbook-converter/design.md#settled-and-open-questions) and [mbook-ui](mbook-ui/design.md#settled-and-open-questions) |

## Two kinds of document

**The format is specified; the crates are described.**

| | The format | A crate |
| --- | --- | --- |
| Documents | [RFC 0001](rfcs/0001-mbook-format.md) and the [decision records](rfcs/adr/overview.md) | The pages of [mbook](mbook/index.md), [mbook-converter](mbook-converter/index.md) and [mbook-ui](mbook-ui/index.md) |
| Says | What a file **MUST** contain and what a reader **MUST** do with it | What the code does today, and why it is built this way |
| Binds | Every implementation of the format, in any language | This repository only |
| A choice between alternatives is recorded | As a decision record (ADR) | In the "Rationale and alternatives" table of the crate's Design page |
| Changes | When the format changes | With the code, in the same change |

An **RFC** specifies behaviour: it is what an implementation is checked
against. A **decision record** explains one choice the RFC makes: the options
that were compared, the trade-off, and what follows from it. The RFC links to
the records instead of repeating them.

The pages of a crate are not normative. They describe what exists, mark what
is not implemented, and where they say that something "must" hold, they state
an invariant of this code that a test or a review keeps. RFC 0002 to
RFC 0004, which described the three crates in earlier versions of this site,
became these pages; the old addresses lead to them.

| Page of a crate | What it answers |
| --- | --- |
| Overview | What the crate is, what it is for, its goals and non-goals. |
| Guide | How each task is done, with its code or its screen. |
| API reference, Conversion rules, Architecture | What exactly the crate does: its types, its rules, its parts. |
| Design | Its dependencies, what the design costs, the alternatives that were rejected, and what is still open. |

## Documents

| Document | Title | Status |
| --- | --- | --- |
| [RFC 0001](rfcs/0001-mbook-format.md) | The mbook file format, version 1 | Draft |
| [ADR-0001 … ADR-0011](rfcs/adr/overview.md) | Decisions behind RFC 0001 | See the overview |
| [mbook](mbook/index.md) | The library that reads and writes the format | Implements the whole of RFC 0001 |
| [mbook-converter](mbook-converter/index.md) | Import from eight formats, export to the same eight | Implemented; [limits per format](mbook-converter/conversion.md#what-is-implemented) |
| [mbook-ui](mbook-ui/index.md) | The reading and editing application | A first version; [what is implemented](mbook-ui/status.md#what-is-implemented) |

## Outline of an RFC

| Section | What it answers |
| --- | --- |
| Summary | What is proposed, in a paragraph. |
| Motivation | Why the format needs it; goals and non-goals. |
| Guide-level explanation | How it looks to someone packing or reading a book. |
| Reference-level explanation | The normative rules an implementation follows. |
| Drawbacks | What the design costs. |
| Rationale and alternatives | The decisions taken, with links to their records. |
| Prior art | Related formats and what was taken from them. |
| Unresolved questions | What must be settled before the RFC is accepted. |
| Future possibilities | What the design leaves room for. |

## Reading the normative text

The key words **MUST**, **MUST NOT**, **REQUIRED**, **SHOULD**, **SHOULD NOT**,
**RECOMMENDED**, **MAY** and **OPTIONAL** are to be interpreted as described in
[RFC 2119](https://www.rfc-editor.org/rfc/rfc2119) and
[RFC 8174](https://www.rfc-editor.org/rfc/rfc8174) when, and only when, they
appear in capitals. They are used in the *Reference-level explanation* of an
RFC and nowhere else: everything outside that section, and every page of a
crate, is informative.

## Building this site

The site is an [mdBook](https://github.com/rust-lang/mdBook) in the `docs`
folder of the
[repository](https://github.com/M-Compressors/mbook), with
[mdbook-mermaid](https://github.com/badboy/mdbook-mermaid) for the diagrams.
It is published to GitHub Pages from the `main` branch.

Extract the diagram scripts once:

```sh
mdbook-mermaid install docs
```

Then serve the book with live reload:

```sh
mdbook serve docs --open
```

The HTML is written to `target/book`.
