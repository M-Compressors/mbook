# mbook design documents

mbook is a file format for books: chapters in Markdown, pictures in AVIF, and
small typed tables that describe the book, all stored as independent
[OpenZL](https://github.com/facebook/openzl) frames inside one indexed
container.

| If you want to… | Read |
| --- | --- |
| see what a file contains and how it is opened | [RFC 0001, guide-level explanation](0001-mbook-format.md#guide-level-explanation) |
| implement a reader or a writer | [RFC 0001, reference-level explanation](0001-mbook-format.md#reference-level-explanation) |
| understand why the format looks this way | [Decision records](adr/overview.md) |
| read or write books from Rust | [RFC 0002, guide-level explanation](0002-mbook-core-api.md#guide-level-explanation) |
| know what is still open | [RFC 0001, unresolved questions](0001-mbook-format.md#unresolved-questions) |

## Documents

| Document | Title | Status |
| --- | --- | --- |
| [RFC 0001](0001-mbook-format.md) | The mbook file format, version 1 | Draft |
| [RFC 0002](0002-mbook-core-api.md) | The mbook-core API | Draft |
| [ADR-0001 … ADR-0007](adr/overview.md) | Decisions behind RFC 0001 | See the overview |

An **RFC** specifies behaviour: it is what an implementation is checked
against. A **decision record** (ADR) explains one choice the RFC makes: the
options that were compared, the trade-off, and what follows from it. The RFC
links to the records instead of repeating them.

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
appear in capitals. Everything outside a *Reference-level explanation* section
is informative.

## Building this book

The book needs [mdBook](https://github.com/rust-lang/mdBook) and
[mdbook-mermaid](https://github.com/badboy/mdbook-mermaid) for the diagrams.

Extract the diagram scripts once:

```sh
mdbook-mermaid install rfcs
```

Then serve the book with live reload:

```sh
mdbook serve rfcs --open
```

The HTML is written to `target/rfcs-book`.
