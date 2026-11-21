//! The transformative properties of an AVIF picture: crop, rotation, mirror.
//!
//! `avif-decode` returns the coded pixels and ignores them, while `mbook`
//! reports the size as displayed ([`mbook::Picture::size`]). So that the
//! bitmap is what the book says it is, the properties of the primary item are
//! read here and applied in the order the file lists them. The container is
//! read as `mbook` reads it: `pitm`, then `ipma` for the properties of that
//! item, then `ipco` for the properties themselves.

/// One transformative property.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Transform {
    /// `clap`: the picture is cut to `width` by `height` around its centre,
    /// moved by the offsets.
    Crop {
        width: u32,
        height: u32,
        x_offset: i64,
        y_offset: i64,
    },
    /// `irot`: turned anticlockwise by this many quarter turns (0 to 3).
    Rotate(u8),
    /// `imir`: mirrored about the vertical axis (left and right swap) when
    /// `true`, about the horizontal axis (top and bottom swap) otherwise.
    Mirror { left_right: bool },
}

/// A cursor over the bytes of a box.
struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, len: usize) -> Option<&'a [u8]> {
        let (head, rest) = self.0.split_at_checked(len)?;
        self.0 = rest;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn u16(&mut self) -> Option<u16> {
        self.take(2)
            .map(|bytes| u16::from_be_bytes([bytes[0], bytes[1]]))
    }

    fn u32(&mut self) -> Option<u32> {
        self.take(4)
            .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
    }

    fn i32(&mut self) -> Option<i32> {
        self.u32()
            .map(|word| i32::from_be_bytes(word.to_be_bytes()))
    }

    /// The next box: its type and its body.
    fn next_box(&mut self) -> Option<([u8; 4], &'a [u8])> {
        if self.0.is_empty() {
            return None;
        }
        let all = self.0.len();
        let size = u64::from(self.u32()?);
        let kind: [u8; 4] = self.take(4)?.try_into().ok()?;
        let size = match size {
            0 => u64::try_from(all).ok()?,
            1 => {
                let high = u64::from(self.u32()?);
                (high << 32) | u64::from(self.u32()?)
            }
            size => size,
        };
        let header = all - self.0.len();
        let body = usize::try_from(size).ok()?.checked_sub(header)?;
        Some((kind, self.take(body)?))
    }

    /// The body of the first box of a type.
    fn child(mut self, kind: [u8; 4]) -> Option<&'a [u8]> {
        while let Some((found, body)) = self.next_box() {
            if found == kind {
                return Some(body);
            }
        }
        None
    }
}

/// A fraction as a whole number, rounded towards zero; `None` for a zero
/// denominator.
fn quotient(numerator: i64, denominator: i64) -> Option<i64> {
    numerator.checked_div(denominator)
}

/// The transformative properties of the primary item, in the order they apply.
/// Nothing when the container cannot be read: the decoder has failed on it
/// already, or the picture is shown as coded.
pub(super) fn transforms(avif: &[u8]) -> Vec<Transform> {
    read(avif).unwrap_or_default()
}

fn read(avif: &[u8]) -> Option<Vec<Transform>> {
    let meta = Cursor(avif).child(*b"meta")?;
    // The version and the flags of the full box `meta`.
    let meta = meta.get(4..)?;

    let mut pitm = Cursor(Cursor(meta).child(*b"pitm")?);
    let version = pitm.u32()?.to_be_bytes()[0];
    let primary = item_id(&mut pitm, version)?;

    let properties = Cursor(meta).child(*b"iprp")?;
    let container = Cursor(properties).child(*b"ipco")?;
    let mut found = Vec::new();
    for index in associations(properties, primary)? {
        // Index 0 stands for no property.
        let Some(skipped) = index.checked_sub(1) else {
            continue;
        };
        let mut boxes = Cursor(container);
        for _ in 0..skipped {
            boxes.next_box()?;
        }
        let (kind, body) = boxes.next_box()?;
        if let Property::Transform(transform) = property(kind, body)? {
            found.push(transform);
        }
    }
    Some(found)
}

fn item_id(cursor: &mut Cursor<'_>, version: u8) -> Option<u32> {
    if version == 0 {
        cursor.u16().map(u32::from)
    } else {
        cursor.u32()
    }
}

/// The one-based indices of the properties of an item.
fn associations(properties: &[u8], item: u32) -> Option<Vec<usize>> {
    let mut boxes = Cursor(properties);
    let mut indices = Vec::new();
    while let Some((kind, body)) = boxes.next_box() {
        if &kind != b"ipma" {
            continue;
        }
        let mut body = Cursor(body);
        let word = body.u32()?;
        let (version, flags) = (word.to_be_bytes()[0], word & 0x00ff_ffff);
        for _ in 0..body.u32()? {
            let id = item_id(&mut body, version)?;
            for _ in 0..body.u8()? {
                let index = if flags & 1 == 0 {
                    usize::from(body.u8()? & 0x7f)
                } else {
                    usize::from(body.u16()? & 0x7fff)
                };
                if id == item {
                    indices.push(index);
                }
            }
        }
    }
    Some(indices)
}

/// What a property of the container is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Property {
    /// A crop, a rotation or a mirror.
    Transform(Transform),
    /// Anything else: the size, the colour, the codec.
    Other,
}

/// What a property is; `None` for a damaged one.
fn property(kind: [u8; 4], body: &[u8]) -> Option<Property> {
    let mut body = Cursor(body);
    Some(match &kind {
        b"clap" => {
            let mut fraction = || {
                let numerator = i64::from(body.u32()?);
                let denominator = i64::from(body.u32()?);
                quotient(numerator, denominator)
            };
            let width = u32::try_from(fraction()?).ok()?;
            let height = u32::try_from(fraction()?).ok()?;
            let mut offset = || {
                let numerator = i64::from(body.i32()?);
                let denominator = i64::from(body.i32()?);
                quotient(numerator, denominator)
            };
            let x_offset = offset()?;
            let y_offset = offset()?;
            Property::Transform(Transform::Crop {
                width,
                height,
                x_offset,
                y_offset,
            })
        }
        b"irot" => Property::Transform(Transform::Rotate(body.u8()? & 3)),
        b"imir" => Property::Transform(Transform::Mirror {
            left_right: body.u8()? & 1 == 0,
        }),
        _ => Property::Other,
    })
}

#[cfg(test)]
mod tests {
    use super::{Property, Transform, property, transforms};
    use std::path::PathBuf;

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../mbook/tests/fixtures")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
    }

    #[test]
    fn plain_fixture_has_no_transform() {
        assert_eq!(transforms(&fixture("plain.avif")), []);
    }

    #[test]
    fn rotated_fixture_has_a_quarter_turn() {
        assert_eq!(transforms(&fixture("rotated.avif")), [Transform::Rotate(1)]);
    }

    #[test]
    fn mirrored_fixture_has_a_mirror() {
        assert_eq!(
            transforms(&fixture("mirrored.avif")),
            [Transform::Mirror { left_right: false }]
        );
    }

    #[test]
    fn cropped_fixture_has_a_crop() {
        assert_eq!(
            transforms(&fixture("cropped.avif")),
            [Transform::Crop {
                width: 16,
                height: 8,
                x_offset: 0,
                y_offset: -2
            }]
        );
    }

    #[test]
    fn garbage_has_no_transform() {
        assert_eq!(transforms(b"not an avif file at all"), []);
        assert_eq!(transforms(&[]), []);
    }

    #[test]
    fn truncated_container_has_no_transform() {
        let avif = fixture("rotated.avif");
        assert_eq!(transforms(&avif[..60]), []);
    }

    #[test]
    fn rotation_keeps_only_the_two_low_bits() {
        assert_eq!(
            property(*b"irot", &[7]),
            Some(Property::Transform(Transform::Rotate(3)))
        );
    }

    #[test]
    fn mirror_axis_zero_swaps_left_and_right() {
        assert_eq!(
            property(*b"imir", &[0]),
            Some(Property::Transform(Transform::Mirror { left_right: true }))
        );
    }

    #[test]
    fn crop_with_a_zero_denominator_is_damaged() {
        let body = [
            0, 0, 0, 16, 0, 0, 0, 0, // width 16 / 0
            0, 0, 0, 8, 0, 0, 0, 1, // height 8 / 1
            0, 0, 0, 0, 0, 0, 0, 1, // x offset 0 / 1
            0, 0, 0, 0, 0, 0, 0, 1, // y offset 0 / 1
        ];
        assert_eq!(property(*b"clap", &body), None);
    }

    #[test]
    fn crop_offsets_may_be_negative_fractions() {
        let body = [
            0, 0, 0, 16, 0, 0, 0, 1, //
            0, 0, 0, 8, 0, 0, 0, 1, //
            0xFF, 0xFF, 0xFF, 0xFC, 0, 0, 0, 2, // -4 / 2
            0, 0, 0, 3, 0, 0, 0, 1, //
        ];
        assert_eq!(
            property(*b"clap", &body),
            Some(Property::Transform(Transform::Crop {
                width: 16,
                height: 8,
                x_offset: -2,
                y_offset: 3
            }))
        );
    }

    #[test]
    fn other_property_is_not_a_transform() {
        assert_eq!(property(*b"ispe", &[0; 12]), Some(Property::Other));
    }

    #[test]
    fn short_property_is_damaged() {
        assert_eq!(property(*b"irot", &[]), None);
    }
}
