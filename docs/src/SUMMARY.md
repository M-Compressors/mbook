# Summary

[Introduction](introduction.md)

# The format

- [RFC 0001: The mbook file format, version 1](rfcs/0001-mbook-format.md)
- [Decision records](rfcs/adr/overview.md)
  - [ADR-0001: Independent frames in an indexed container](rfcs/adr/0001-independent-frames.md)
  - [ADR-0002: Pictures are stored AVIF files](rfcs/adr/0002-stored-avif.md)
  - [ADR-0003: A format header as the first frame](rfcs/adr/0003-format-header.md)
  - [ADR-0004: Tables are typed columns](rfcs/adr/0004-typed-columns.md)
  - [ADR-0005: References by path through a resource table](rfcs/adr/0005-paths-and-resource-table.md)
  - [ADR-0006: GitHub Flavored Markdown without raw HTML](rfcs/adr/0006-markdown-dialect.md)
  - [ADR-0007: Byte offsets and a stored anchor table](rfcs/adr/0007-positions-and-anchors.md)
  - [ADR-0008: Anchors that are not headings live in a table before their chapter](rfcs/adr/0008-chapter-anchor-tables.md)
  - [ADR-0009: A line is named by a link reference definition in its chapter](rfcs/adr/0009-anchor-definitions.md)
  - [ADR-0010: Landmarks are a table of their own, present only when a book has them](rfcs/adr/0010-landmarks.md)
  - [ADR-0011: Diagrams and formulas are resources of their own kinds](rfcs/adr/0011-diagrams-and-formulas.md)

# Libraries

- [mbook](mbook/index.md)
  - [Guide](mbook/guide.md)
  - [API reference](mbook/api.md)
  - [Design](mbook/design.md)
- [mbook-converter](mbook-converter/index.md)
  - [Guide](mbook-converter/guide.md)
  - [API reference](mbook-converter/api.md)
  - [Conversion rules](mbook-converter/conversion.md)
  - [Design](mbook-converter/design.md)

# Application

- [mbook-ui](mbook-ui/index.md)
  - [Using the application](mbook-ui/guide.md)
  - [Architecture](mbook-ui/architecture.md)
  - [Agents (MCP)](mbook-ui/mcp.md)
  - [Appearance and accessibility](mbook-ui/appearance.md)
  - [Android](mbook-ui/android.md)
  - [Status and verification](mbook-ui/status.md)
  - [Design](mbook-ui/design.md)
