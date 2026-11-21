# ADR-0002: Pictures are stored AVIF files

**Status:** Accepted\
**Date:** 2026-11-21\
**Deciders:** Format author

## Context

Pictures are AVIF. OpenZL allows custom codecs, so AVIF could be a codec
inside the frame's graph instead of opaque bytes. Two things constrain the
choice:

- An OpenZL codec must regenerate its input exactly; the frame's content
  checksum (on by default) verifies it.
- A frame that uses a custom codec can only be decoded by an application that
  registers the same codec.

## Decision

A picture resource holds a complete AVIF file, written with the OpenZL *store*
graph. Version 1 defines no custom codec. Converting a source picture to AVIF
is a step of the packer, done before the frame is written.

## Options considered

| Dimension | A: stored AVIF file | B: lossy pixel codec in the graph | C: lossless AVIF repacking codec |
| --- | --- | --- | --- |
| Reader complexity | Hand the bytes to any AVIF decoder | Must bundle an AV1 decoder inside the OpenZL codec; output is raw pixels | Must register the codec; output is an AVIF file |
| File size | AVIF size plus 50–52 B of headers | Same payload | Saves an estimated few hundred bytes per picture |
| Compatibility | Any OpenZL decoder extracts the file | Breaks the lossless contract; rejected by the content checksum | Frames unreadable without the codec |
| Evolution | Nothing to version | Codec becomes part of the format | Codec identifier becomes part of the format |

**Chosen: A.**

## Trade-off analysis

- **B is not viable.** Pixels in, AV1 out, different pixels back is not a
  reversible transform. It would only work with content checksums disabled,
  and decoding would yield tens of megabytes of raw pixels per page instead of
  a file the platform can decode, tile and colour-manage itself.
- **C is viable but pays little.** It removes the ISOBMFF boxes and rebuilds
  them on decoding. That matters only for books with thousands of pictures of
  a few hundred bytes each, and it costs the property that every frame is
  standard OpenZL.
- **A** keeps the format free of custom codecs. Because a frame always decodes
  to "an AVIF file", C can be added later without changing what readers
  receive.

## Consequences

- Easier: readers use the platform's AVIF decoder; frames stay decodable by a
  stock OpenZL decoder.
- Harder: nothing is saved on AVIF container overhead for tiny pictures.
- To revisit: option C, if measurements on formula-heavy books justify it.

## Action items

1. [ ] Packer: convert source pictures to AVIF before writing frames.
2. [ ] Packer: record displayed `width` and `height` in the resource table.
3. [ ] Measure AVIF container overhead on a book with many small pictures.
