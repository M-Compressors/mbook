//! Reading the pixel size of a picture from its AVIF container.
//!
//! An AVIF file is a sequence of ISO base media boxes. The size of the
//! picture as displayed follows from the properties of its primary item:
//! `ispe` gives the coded size, `clap` crops it and `irot` rotates it. No
//! pixel data is decoded.

use crate::types::Size;
use bytes::Buf;

const TRUNCATED: &str = "the AVIF file is truncated";

/// A box: its four-character type and its body.
type Found<'a> = ([u8; 4], &'a [u8]);

/// The next box of `data` as its type and its body; `None` at the end.
fn next_box<'a>(data: &mut &'a [u8]) -> Result<Option<Found<'a>>, &'static str> {
    if data.is_empty() {
        return Ok(None);
    }
    let all = data.len();
    let size = u64::from(data.try_get_u32().map_err(|_| TRUNCATED)?);
    let mut kind = [0; 4];
    data.try_copy_to_slice(&mut kind).map_err(|_| TRUNCATED)?;
    let size = match size {
        // The box runs to the end of its container.
        0 => all as u64,
        // The size is in the next eight bytes.
        1 => data.try_get_u64().map_err(|_| TRUNCATED)?,
        size => size,
    };
    let header = all - data.len();
    let body = usize::try_from(size)
        .ok()
        .and_then(|size| size.checked_sub(header))
        .ok_or("an AVIF box has an invalid size")?;
    let (body, rest) = data.split_at_checked(body).ok_or(TRUNCATED)?;
    *data = rest;
    Ok(Some((kind, body)))
}

/// The version and the flags that start a full box.
fn full_box(data: &mut &[u8]) -> Result<(u8, u32), &'static str> {
    let word = data.try_get_u32().map_err(|_| TRUNCATED)?;
    Ok((word.to_be_bytes()[0], word & 0x00ff_ffff))
}

/// An item id: 16 bits in version 0 of a box, 32 bits later.
fn item_id(data: &mut &[u8], version: u8) -> Result<u32, &'static str> {
    if version == 0 {
        data.try_get_u16().map(u32::from).map_err(|_| TRUNCATED)
    } else {
        data.try_get_u32().map_err(|_| TRUNCATED)
    }
}

/// The first child box of a type.
fn child(mut body: &[u8], kind: [u8; 4]) -> Result<Option<&[u8]>, &'static str> {
    while let Some((found, body)) = next_box(&mut body)? {
        if found == kind {
            return Ok(Some(body));
        }
    }
    Ok(None)
}

/// The size of the primary image item as displayed, after its crop and
/// rotation. Mirroring does not change the size.
///
/// The error is the reason the bytes are not a usable AVIF file.
pub(crate) fn displayed_size(avif: &[u8]) -> Result<Size, &'static str> {
    let brands = next_box(&mut &*avif)?.filter(|(kind, _)| kind == b"ftyp");
    let (_, brands) = brands.ok_or("the file does not start with an AVIF header")?;
    if !is_avif(brands) {
        return Err("the file is not an AVIF file");
    }
    let mut meta = child(avif, *b"meta")?.ok_or("the AVIF file has no metadata")?;
    full_box(&mut meta)?;

    let mut primary = child(meta, *b"pitm")?.ok_or("the AVIF file has no primary image")?;
    let (version, _) = full_box(&mut primary)?;
    let primary = item_id(&mut primary, version)?;

    let properties = child(meta, *b"iprp")?.ok_or("the AVIF file has no image properties")?;
    let container = child(properties, *b"ipco")?.ok_or("the AVIF file has no image properties")?;
    let mut size = None;
    for index in associations(properties, primary)? {
        apply(property(container, index)?, &mut size)?;
    }
    match size {
        Some(size) if size.width != 0 && size.height != 0 => Ok(size),
        Some(_) => Err("the primary image has no pixels"),
        None => Err("the primary image has no size"),
    }
}

/// Tells whether a `ftyp` box names AVIF as its major or a compatible brand.
fn is_avif(brands: &[u8]) -> bool {
    let is_avif = |brand: &[u8]| brand == b"avif" || brand == b"avis";
    // The major brand, four bytes of minor version, then compatible brands.
    let major = brands.get(..4).is_some_and(is_avif);
    let (compatible, _) = brands.get(8..).unwrap_or_default().as_chunks::<4>();
    major || compatible.iter().any(|brand| is_avif(brand))
}

/// The one-based indices of the properties of an item, in the order they are
/// applied.
fn associations(mut properties: &[u8], item: u32) -> Result<Vec<usize>, &'static str> {
    let mut indices = Vec::new();
    while let Some((kind, mut body)) = next_box(&mut properties)? {
        if &kind != b"ipma" {
            continue;
        }
        let (version, flags) = full_box(&mut body)?;
        for _ in 0..body.try_get_u32().map_err(|_| TRUNCATED)? {
            let id = item_id(&mut body, version)?;
            for _ in 0..body.try_get_u8().map_err(|_| TRUNCATED)? {
                // The top bit marks the property as essential.
                let index = if flags & 1 == 0 {
                    body.try_get_u8().map(|index| usize::from(index & 0x7f))
                } else {
                    body.try_get_u16().map(|index| usize::from(index & 0x7fff))
                };
                if id == item {
                    indices.push(index.map_err(|_| TRUNCATED)?);
                }
            }
        }
    }
    Ok(indices)
}

/// The property with a one-based index in the property container.
fn property(mut container: &[u8], index: usize) -> Result<Found<'_>, &'static str> {
    let mut at = 0;
    while let Some(found) = next_box(&mut container)? {
        at += 1;
        if at == index {
            return Ok(found);
        }
    }
    Err("an AVIF property is missing")
}

/// Applies one property to the size seen so far.
fn apply((kind, mut body): Found<'_>, size: &mut Option<Size>) -> Result<(), &'static str> {
    let word = |body: &mut &[u8]| body.try_get_u32().map_err(|_| TRUNCATED);
    match &kind {
        b"ispe" => {
            full_box(&mut body)?;
            *size = Some(Size {
                width: word(&mut body)?,
                height: word(&mut body)?,
            });
        }
        b"clap" => {
            let mut fraction = || -> Result<u32, &'static str> {
                let numerator = word(&mut body)?;
                let denominator = word(&mut body)?;
                numerator
                    .checked_div(denominator)
                    .ok_or("an AVIF crop has a zero denominator")
            };
            *size = Some(Size {
                width: fraction()?,
                height: fraction()?,
            });
        }
        b"irot" => {
            // A quarter turn or three swap the sides.
            let turns = body.try_get_u8().map_err(|_| TRUNCATED)?;
            if turns & 1 == 1
                && let Some(size) = size
            {
                std::mem::swap(&mut size.width, &mut size.height);
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::displayed_size;
    use crate::types::Size;

    fn boxed(kind: [u8; 4], body: &[u8]) -> Vec<u8> {
        let size = u32::try_from(body.len() + 8).unwrap();
        [&size.to_be_bytes()[..], &kind, body].concat()
    }

    fn full(kind: [u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
        let word = (u32::from(version) << 24) | flags;
        boxed(kind, &[&word.to_be_bytes()[..], body].concat())
    }

    fn words(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }

    /// The container of an AVIF file whose primary item has these properties.
    /// It holds no picture data: only what `displayed_size` reads.
    pub(crate) fn avif(properties: &[Vec<u8>]) -> Vec<u8> {
        let count = u8::try_from(properties.len()).unwrap();
        let mut association = vec![0, 0, 0, 1, 0, 1, count];
        association.extend(1..=count);
        let meta = [
            full(*b"hdlr", 0, 0, b"\0\0\0\0pict\0\0\0\0\0\0\0\0\0\0\0\0\0"),
            full(*b"pitm", 0, 0, &[0, 1]),
            boxed(
                *b"iprp",
                &[
                    boxed(*b"ipco", &properties.concat()),
                    full(*b"ipma", 0, 0, &association),
                ]
                .concat(),
            ),
        ];
        [
            boxed(*b"ftyp", b"avif\0\0\0\0avifmif1miaf"),
            full(*b"meta", 0, 0, &meta.concat()),
            boxed(*b"mdat", b"not a picture"),
        ]
        .concat()
    }

    pub(crate) fn ispe(width: u32, height: u32) -> Vec<u8> {
        full(*b"ispe", 0, 0, &words(&[width, height]))
    }

    fn clap(width: u32, height: u32) -> Vec<u8> {
        boxed(*b"clap", &words(&[width, 1, height, 1, 0, 1, 0, 1]))
    }

    fn size(width: u32, height: u32) -> Size {
        Size { width, height }
    }

    #[test]
    fn the_coded_size_is_the_displayed_size_of_a_plain_picture() {
        assert_eq!(
            displayed_size(&avif(&[ispe(1200, 1800)])),
            Ok(size(1200, 1800))
        );
    }

    #[test]
    fn a_crop_replaces_the_coded_size() {
        let file = avif(&[ispe(1920, 1088), clap(1920, 1080)]);
        assert_eq!(displayed_size(&file), Ok(size(1920, 1080)));
    }

    #[test]
    fn a_quarter_turn_swaps_the_sides() {
        let file = avif(&[ispe(1600, 900), boxed(*b"irot", &[1])]);
        assert_eq!(displayed_size(&file), Ok(size(900, 1600)));
    }

    #[test]
    fn a_half_turn_keeps_the_sides() {
        let file = avif(&[ispe(1600, 900), boxed(*b"irot", &[2])]);
        assert_eq!(displayed_size(&file), Ok(size(1600, 900)));
    }

    #[test]
    fn a_crop_is_applied_before_the_rotation() {
        let file = avif(&[ispe(1920, 1088), clap(1920, 1080), boxed(*b"irot", &[3])]);
        assert_eq!(displayed_size(&file), Ok(size(1080, 1920)));
    }

    #[test]
    fn mirroring_and_unknown_properties_change_nothing() {
        let file = avif(&[
            ispe(64, 48),
            boxed(*b"imir", &[1]),
            boxed(*b"pixi", &[3, 8, 8, 8]),
        ]);
        assert_eq!(displayed_size(&file), Ok(size(64, 48)));
    }

    #[test]
    fn a_property_of_another_item_is_ignored() {
        // Two properties, but only the second is associated with item 1.
        let mut file = avif(&[ispe(10, 10), ispe(640, 480)]);
        let association = file
            .windows(4)
            .position(|window| window == b"ipma")
            .unwrap();
        // version+flags (4), entry count (4), item id (2), count (1), indices.
        file[association + 4 + 4 + 4 + 2] = 1;
        file[association + 4 + 4 + 4 + 3] = 2;
        assert_eq!(displayed_size(&file), Ok(size(640, 480)));
    }

    #[test]
    fn an_image_sequence_brand_is_accepted() {
        let mut file = avif(&[ispe(32, 32)]);
        file[8..12].copy_from_slice(b"avis");
        assert_eq!(displayed_size(&file), Ok(size(32, 32)));
    }

    #[test]
    fn another_file_type_is_refused() {
        let mut file = avif(&[ispe(32, 32)]);
        file[8..12].copy_from_slice(b"heic");
        file[16..20].copy_from_slice(b"heic");
        assert_eq!(displayed_size(&file), Err("the file is not an AVIF file"));
    }

    #[test]
    fn bytes_that_are_not_boxes_are_refused() {
        assert_eq!(
            displayed_size(b"\x89PNG\r\n\x1a\n and then some more bytes"),
            Err("the AVIF file is truncated")
        );
    }

    #[test]
    fn an_empty_input_is_refused() {
        assert_eq!(
            displayed_size(&[]),
            Err("the file does not start with an AVIF header")
        );
    }

    #[test]
    fn a_file_cut_inside_its_metadata_is_refused() {
        let file = avif(&[ispe(32, 32)]);
        assert_eq!(
            displayed_size(&file[..60]),
            Err("the AVIF file is truncated")
        );
    }

    #[test]
    fn a_picture_without_a_size_property_is_refused() {
        let file = avif(&[boxed(*b"pixi", &[3, 8, 8, 8])]);
        assert_eq!(displayed_size(&file), Err("the primary image has no size"));
    }

    #[test]
    fn a_picture_of_zero_pixels_is_refused() {
        assert_eq!(
            displayed_size(&avif(&[ispe(0, 480)])),
            Err("the primary image has no pixels")
        );
    }

    #[test]
    fn a_crop_with_a_zero_denominator_is_refused() {
        let bad = boxed(*b"clap", &words(&[100, 0, 100, 1, 0, 1, 0, 1]));
        assert_eq!(
            displayed_size(&avif(&[ispe(100, 100), bad])),
            Err("an AVIF crop has a zero denominator")
        );
    }

    #[test]
    fn a_box_smaller_than_its_header_is_refused() {
        let mut file = avif(&[ispe(32, 32)]);
        file[..4].copy_from_slice(&4u32.to_be_bytes());
        assert_eq!(
            displayed_size(&file),
            Err("an AVIF box has an invalid size")
        );
    }
}
