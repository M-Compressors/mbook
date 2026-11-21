//! The contents of the example book of RFC 0001.

/// The first chapter: two headings, a figure and a footnote.
pub(crate) const CH01: &str = "# Chapter 1. Arrival

The train was late.

![The station at night](img/fig-1.avif \"Fig. 1. The station\")

## The station

It was empty[^n1].

[^n1]: Almost.
";

/// The second chapter: a link into the first one and an external link.
pub(crate) const CH02: &str = "# Chapter 2. The letter

See [the station](ch01.md#the-station), or <https://example.org>.
";

/// The identifier of the example book.
pub(crate) const ID: &str = "urn:uuid:3f1c2a7e-8a54-4d0b-9c1e-6b2f0d9a7e11";

/// A real AVIF picture of 24 by 16 pixels.
pub(crate) const PLAIN: &[u8] = include_bytes!("../fixtures/plain.avif");
/// The same picture with a quarter turn: 16 by 24 as displayed.
pub(crate) const ROTATED: &[u8] = include_bytes!("../fixtures/rotated.avif");
/// The same picture cropped to 16 by 8.
#[cfg(all(feature = "read", feature = "write"))]
pub(crate) const CROPPED: &[u8] = include_bytes!("../fixtures/cropped.avif");
/// The same picture mirrored: still 24 by 16.
#[cfg(all(feature = "read", feature = "write"))]
pub(crate) const MIRRORED: &[u8] = include_bytes!("../fixtures/mirrored.avif");

/// Where the second heading of the first chapter starts.
#[cfg(feature = "read")]
pub(crate) fn station_offset() -> u32 {
    CH01.find("## The station")
        .and_then(|offset| u32::try_from(offset).ok())
        .unwrap_or(0)
}
