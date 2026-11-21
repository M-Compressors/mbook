# ADR-0005: References by path through a resource table

**Status:** Proposed\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

Markdown must point at pictures and at other chapters, and each of those is a
frame in the container. The container's directory lists only offsets and
sizes; a resource's name, if any, is in its own record header.

## Decision

Markdown uses ordinary relative paths (`img/fig-3.avif`, `ch01.md`). Resource
2 is a table with one row per resource: `kind`, `path`, and the pixel size of
pictures. A reader resolves a path to a row, and the row number is the
resource number.

## Options considered

| Dimension | A: paths and a resource table | B: frame numbers in Markdown | C: paths matched against record headers |
| --- | --- | --- | --- |
| Reader complexity | One table lookup | None | One header read per resource to build the map |
| File size | 1–12 B per resource | 0 | A name per record header, uncompressed |
| Compatibility | Unpacked chapters are valid Markdown with working links | Links mean nothing outside the container | Same as A |
| Evolution | Repacking and reordering keep links valid | Any reordering rewrites chapter text | Same as A |
| Layout before decoding | Picture sizes are in the table | Not available | Not available |

**Chosen: A.**

## Trade-off analysis

- **B** is the smallest, but it ties chapter text to the physical layout and
  gives up the property that a book unpacks into a folder a Markdown viewer
  can show.
- **C** avoids a table but needs a read per resource before the first link
  can be resolved, which is slow over a network, and has no place for picture
  sizes.
- **A** adds a table that is small in every measured case and is read once at
  open. It also lists kinds, so readers know the reading order and can
  reserve space for pictures before decoding them.

## Consequences

- Easier: pack from and unpack to a plain folder; reorder resources freely.
- Easier: readers lay out text before pictures are decoded.
- Harder: writers must keep the table and the container directory in step.
- To revisit: hash-like names compress poorly (11.7 B per row against 3.6 B
  for sequential names); writers should generate sequential names.

## Action items

1. [ ] Implement destination resolution exactly as the RFC specifies.
2. [ ] Validate paths (no `..`, no leading `/`, unique) in the packer.
3. [ ] Implement unpack-to-folder and pack-from-folder as a round trip test.
