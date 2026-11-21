# Decision records

Each record explains one choice made by
[RFC 0001](../0001-mbook-format.md): the situation, the options compared, the
trade-off, and the consequences.

| ADR | Decision | Status |
| --- | --- | --- |
| [0001](0001-independent-frames.md) | One mopenzl framed container; one OpenZL frame per resource | Accepted |
| [0002](0002-stored-avif.md) | Pictures are AVIF files stored without a custom codec | Accepted |
| [0003](0003-format-header.md) | Resource 0 is an 8-byte format header | Accepted |
| [0004](0004-typed-columns.md) | Tables are typed OpenZL columns, one table per frame | Accepted |
| [0005](0005-paths-and-resource-table.md) | Markdown refers to resources by path; a table maps paths to frames | Proposed |
| [0006](0006-markdown-dialect.md) | Chapters are GitHub Flavored Markdown without raw HTML | Accepted |
| [0007](0007-positions-and-anchors.md) | Positions are byte offsets; heading anchors are stored in a table | Accepted |
| [0008](0008-chapter-anchor-tables.md) | Names of lines that are not headings are stored in a table before their chapter | Accepted |

## Status values

| Status | Meaning |
| --- | --- |
| Proposed | Written into the RFC draft, not yet confirmed by the format author. |
| Accepted | Confirmed by the format author. Details marked *proposed* inside a record are still open. |
| Superseded | Replaced by a later record, which is linked from the old one. |

## How the options are compared

A file format has no servers or request rates, so the records replace the
usual service dimensions with the ones that matter here:

| Dimension | Question |
| --- | --- |
| Reader complexity | How much code does a reader need, and which libraries? |
| File size | What does the choice cost in bytes? |
| Compatibility | Do existing tools and decoders keep working? |
| Evolution | How does the choice age when the format grows? |

Sizes and timings quoted in the records were measured on synthetic books with
mopenzl 0.2.5; they show orders of magnitude, not guarantees.
