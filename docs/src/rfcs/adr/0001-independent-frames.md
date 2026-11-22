# ADR-0001: Independent frames in an indexed container

**Status:** Accepted\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

A book is read in pieces: the title and cover in a library view, one chapter
at a time while reading, pictures as they scroll into view. The file must
give each piece without decoding the others, over a local disk or over HTTP
range requests. The descriptive data (metadata, contents) is tabular.

## Decision

An mbook file is one mopenzl *framed* container. Every part of the book
(header, each table, each chapter, each picture) is its own resource, and
every resource is exactly one ordinary OpenZL frame.

## Options considered

| Dimension | A: framed container, one frame per resource | B: one solid frame | C: ZIP archive |
| --- | --- | --- | --- |
| Reader complexity | One library: container and codecs together | Lowest | ZIP reader plus a codec |
| Random access | Whole resource, through a checked directory | None: everything before the target is decoded | Whole entry, through the central directory |
| File size | Text compresses per chapter; 74–76 B per stored resource | Best text compression | Text compresses per entry; about 76 B plus the name twice per entry |
| Typed data | Native typed inputs per frame | Native, but all in one frame | None; entries are bytes |
| Compatibility | Frames are standard OpenZL; the outer layout has one implementation | Any OpenZL decoder | Any ZIP tool |
| Evolution | New resource kinds are new rows | Layout changes break the whole stream | New entries are ignored by old readers |

**Chosen: A.**

## Trade-off analysis

- **B** gives the smallest text but cannot open a chapter or a picture without
  decoding what precedes it, which rules out lazy loading.
- **C** matches A on random access and is readable by every tool, but its
  entries are opaque bytes: tables would need a text format and a parser
  ([ADR-0004](0004-typed-columns.md)).
- **A** costs text compression (each chapter starts with an empty window; the
  loss is estimated at 15–20% of the text) and depends on a container with a
  single implementation. In return it has typed frames, per-resource graphs
  and checksums, and a seek index, with no extra library.

## Consequences

- Easier: lazy loading, HTTP range reads, per-resource integrity checks,
  storing pictures uncompressed next to compressed text.
- Harder: matching the text compression of a solid archive.
- Harder: third-party implementations must implement the `MOZFRM` layout.
- To revisit: a shared text dictionary
  ([Future possibilities](../0001-mbook-format.md#future-possibilities)), and
  the container's default limits, which are too small for picture-heavy books.

## Action items

1. [ ] Wrap `FramedSeekReader` and `FramedWriter` in `mbook`.
2. [ ] Choose and document mbook's own `FramingConfig` limits.
3. [ ] Measure text compression per chapter against a solid frame on real books.
