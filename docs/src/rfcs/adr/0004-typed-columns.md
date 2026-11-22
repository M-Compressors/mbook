# ADR-0004: Tables are typed columns

**Status:** Accepted (one table per frame is proposed)\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

Four parts of a book are tables: metadata (key, value), the resource list,
the table of contents and the heading anchors. They are read at every open
and can have thousands of rows. An OpenZL frame can carry several typed
inputs (integers of a fixed width, strings with lengths) and returns them
with their types.

## Decision

Each table is one frame. Each column is one typed input, in a fixed order.
Later minor versions add columns only at the end, and readers ignore columns
they do not know.

## Options considered

| Dimension | A: typed columns, one table per frame | B: JSON or TOML in a serial frame | C: typed columns, all tables in one frame |
| --- | --- | --- | --- |
| Reader complexity | No parser; slices of integers and strings | Needs a text parser and a schema layer | Same as A |
| File size | 849 B for a 120-row contents table | 957 B for the same table | Saves about 76 B per merged table |
| Compatibility | Any OpenZL decoder returns the columns | Any tool reads the text | Same as A |
| Evolution | Append columns per table | Add keys freely | Appending to one table renumbers the others |
| Inspectability | Needs a dump tool | Human-readable after decoding | Needs a dump tool |

**Chosen: A.**

## Trade-off analysis

- The size difference between A and B is small. The deciding factor is the
  parser: columns need none, which keeps the container layer small and usable
  without the standard library.
- **B** is easier to inspect and to extend with nested values. Flat key/value
  rows cover the metadata needed so far.
- **C** saves a few hundred bytes per book and makes column positions global:
  no table could grow without shifting another. One frame per table keeps
  each table's positions private.

## Consequences

- Easier: zero-dependency readers; large tables stay compact (a 100,000-row
  resource table measured 362 KB and decoded in under 3 ms).
- Harder: no nested values; a debugging tool is needed to look inside.
- Harder: column order is fixed forever within a major version.
- To revisit: nested metadata, if a use case needs more than key/value rows.

## Action items

1. [ ] Define table readers and builders in `mbook`.
2. [ ] Reject a table whose columns differ in row count, type or width.
3. [ ] Add a `dump` command that prints tables as text.
