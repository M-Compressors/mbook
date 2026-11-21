# ADR-0003: A format header as the first frame

**Status:** Accepted (the 8-byte layout and the pinned encoding are proposed)\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

The container's own signature, `MOZFRM`, says "mopenzl framed container", not
"book". A file manager, a library scanner or `file(1)` needs to recognise an
mbook file, and a reader needs the format version before it interprets
anything else.

## Decision

Resource 0 is a format header: the prefix `MBOOK` and a zero byte, then
`major` and `minor`, 8 bytes in total. It is written with the *store* graph
and a pinned encoding, so its bytes appear unchanged at file offset 47.

## Options considered

| Dimension | A: stored 8-byte frame | B: metadata field on resource 0 | C: version key in the `meta` table |
| --- | --- | --- | --- |
| Reader complexity | Decode one tiny frame | Read a record header | Decode and search a table |
| Detection without OpenZL | Yes, fixed offset 47 | Yes, fixed offset 32 | No |
| Integrity | Frame checksums cover the version | Header checksum covers the field | Frame checksums cover the table |
| Compatibility | Ordinary frame | Uses the container's metadata for format semantics | Ordinary frame |
| Evolution | Fixed size; growth needs a new `major` | Free-form value | Free-form value |

**Chosen: A.**

## Trade-off analysis

- **C** hides the version inside compressed data whose shape depends on the
  version being read.
- **B** is the smallest change, but it puts format semantics into the
  container's metadata layer, and the header cannot be handled like every
  other resource.
- **A** costs 56 bytes and one constraint on writers: store graph, OpenZL
  format version 27, both checksums on, no metadata. Without the pinned
  encoding a different frame header could move the prefix. In return the
  first 53 bytes of every file are constant.

## Consequences

- Easier: one-line detection rules; a version that is decoded and checksummed
  like everything else.
- Harder: writers must reproduce the pinned encoding exactly.
- To revisit: the pinned OpenZL format version, when OpenZL defines a newer
  one.

## Action items

1. [ ] Add the 64-byte prefix of a 1.0 file as a test vector in `mbook-core`.
2. [ ] Implement detection by fixed offsets and full validation by decoding.
3. [ ] Test that a file with a greater `major` is rejected and a greater
   `minor` is accepted.
