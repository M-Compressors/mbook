# ADR-0006: GitHub Flavored Markdown without raw HTML

**Status:** Accepted (GFM); the exclusion of raw HTML is proposed\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

"Markdown" is a family of dialects. Two readers must render the same chapter
the same way, so the format has to name one dialect. The dialect also decides
what a reader must contain: Markdown that permits HTML needs an HTML engine.

## Decision

Chapters are CommonMark with the GitHub Flavored Markdown extensions (tables,
task list items, strikethrough, extended autolinks) and GitHub-style
footnotes. Raw HTML, network pictures and link schemes other than `http`,
`https` and `mailto` are not allowed.

## Options considered

| Dimension | A: GFM without raw HTML | B: CommonMark with heading attributes | C: full GFM including HTML |
| --- | --- | --- | --- |
| Reader complexity | Markdown parser only | Markdown parser only | Markdown parser and an HTML engine |
| File size | Same | Same | Same |
| Compatibility | Renders the same on GitHub and in GFM viewers | `{#id}` shows as literal text on GitHub | Accepts any GitHub document |
| Evolution | Additions (math, alerts) degrade to text | Same | Anything can be smuggled in as HTML |
| Safety | No active content | No active content | Scripts, styles and remote content must be filtered |

**Chosen: A.**

## Trade-off analysis

- **B** was the first draft. It gives authors explicit anchor names, but in a
  syntax GitHub does not have, and it is a dialect nobody writes by default.
- **C** accepts every existing document and brings the web engine that the
  format exists to avoid. It also reopens tracking and script injection.
- **A** is a strict subset of what GitHub renders: every chapter is valid GFM,
  but not every GFM document is a valid chapter. Documents that use `<br>`,
  `<sub>` or `<img width>` must be converted when packed.

Two details follow from choosing GFM:

- A single tilde now marks strikethrough, so packers escape literal tildes.
- Extended autolinks have uneven library support. Of the two Rust parsers
  tried, `markdown` 1.0.0 implements the whole dialect and reports byte
  offsets; `pulldown-cmark` 0.13.5 does not turn bare URLs into links.

## Consequences

- Easier: authors use a dialect they know; books can be previewed on GitHub.
- Easier: readers need no HTML, CSS or script handling.
- Harder: layout that Markdown cannot express cannot be stored.
- Harder: anchors are no longer chosen by the author
  ([ADR-0007](0007-positions-and-anchors.md)). Later, a name for a line that
  is not a heading was given back in plain GFM, as a link reference
  definition ([ADR-0009](0009-anchor-definitions.md)).
- To revisit: math and alert blocks; the exact CommonMark version; a formal
  definition of footnotes.

## Action items

1. [ ] Packer: reject or convert raw HTML and disallowed schemes.
2. [ ] Packer: escape literal tildes when importing prose.
3. [ ] Build a conformance set of chapters and expected parse results.
