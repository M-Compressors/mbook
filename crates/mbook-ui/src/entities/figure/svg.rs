//! The little of SVG the figures need: the declared size of a document.

use std::sync::Arc;

use gpui_kit::{Image, ImageFormat};

/// An SVG document as an image GPUI rasterises.
pub(super) fn image(document: String) -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Svg, document.into_bytes()))
}

/// The opening tag of the root element, without its `>`.
fn root(document: &str) -> Option<&str> {
    let tag = document.get(document.find("<svg")?..)?;
    tag.get(..tag.find('>')?)
}

/// The value of an attribute of a tag.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!(" {name}=\"");
    let value = tag.get(tag.find(&marker)? + marker.len()..)?;
    value.get(..value.find('"')?)
}

/// The size a document declares: its `width` and `height`, or the size of
/// its `viewBox` when it has neither. `None` for a document without a size
/// or with one that is not positive.
pub(super) fn size(document: &str) -> Option<(f32, f32)> {
    let tag = root(document)?;
    let length = |name: &str| {
        attribute(tag, name)?
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f32>()
            .ok()
    };
    let size = if let (Some(width), Some(height)) = (length("width"), length("height")) {
        (width, height)
    } else {
        let mut parts = attribute(tag, "viewBox")?
            .split([' ', ','])
            .filter(|part| !part.is_empty())
            .map(str::parse::<f32>);
        let (_, _, width, height) = (
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
            parts.next()?.ok()?,
        );
        (width, height)
    };
    (size.0.is_finite() && size.1.is_finite() && size.0 > 0.0 && size.1 > 0.0).then_some(size)
}

/// The document with another declared size. Its `viewBox` keeps what is
/// drawn; a document without one gets the size it declared as its `viewBox`.
pub(super) fn resized(document: &str, width: f32, height: f32) -> Option<String> {
    let tag = root(document)?;
    let start = document.find("<svg")?;
    let rest = document.get(start + tag.len()..)?;
    let view_box = if attribute(tag, "viewBox").is_some() {
        String::new()
    } else {
        let (width, height) = size(document)?;
        format!(" viewBox=\"0 0 {width} {height}\"")
    };
    let mut kept = String::with_capacity(tag.len());
    let mut attributes = tag;
    // Every attribute but the two that are replaced.
    for name in ["width", "height"] {
        let marker = format!(" {name}=\"");
        if let Some(at) = attributes.find(&marker) {
            let after = attributes.get(at + marker.len()..)?;
            let end = at + marker.len() + after.find('"')? + 1;
            kept.push_str(attributes.get(..at)?);
            attributes = attributes.get(end..)?;
        }
    }
    kept.push_str(attributes);
    Some(format!(
        "{}{kept}{view_box} width=\"{width}\" height=\"{height}\"{rest}",
        document.get(..start)?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_size_is_read_from_width_and_height() {
        let document = r#"<svg xmlns="x" width="120.5" height="40px" viewBox="0 0 1 1"></svg>"#;
        assert_eq!(size(document), Some((120.5, 40.0)));
    }

    #[test]
    fn the_size_falls_back_to_the_view_box() {
        let document = r#"<?xml version="1.0"?><svg viewBox="0 0 300 150" width="100%"></svg>"#;
        assert_eq!(size(document), Some((300.0, 150.0)));
        let commas = r#"<svg viewBox="0,0,30,15"></svg>"#;
        assert_eq!(size(commas), Some((30.0, 15.0)));
    }

    #[test]
    fn a_document_without_a_positive_size_has_none() {
        assert_eq!(size("<svg></svg>"), None);
        assert_eq!(size(r#"<svg width="0" height="10"></svg>"#), None);
        assert_eq!(size(r#"<svg viewBox="0 0 x 10"></svg>"#), None);
        assert_eq!(size("not a document"), None);
    }

    #[test]
    fn a_resized_document_keeps_its_view_box_and_its_content() {
        let document =
            r#"<svg xmlns="x" width="10" height="20" viewBox="0 0 10 20"><path d="M0"/></svg>"#;
        let resized = resized(document, 25.0, 50.0).unwrap();
        assert_eq!(size(&resized), Some((25.0, 50.0)));
        assert!(resized.contains(r#"viewBox="0 0 10 20""#));
        assert!(resized.ends_with(r#"><path d="M0"/></svg>"#));
        assert_eq!(resized.matches(" width=").count(), 1);
        assert_eq!(resized.matches(" height=").count(), 1);
    }

    #[test]
    fn a_document_without_a_view_box_gets_one_from_its_size() {
        let document = r#"<svg width="10" height="20"><g/></svg>"#;
        let resized = resized(document, 30.0, 60.0).unwrap();
        assert!(resized.contains(r#"viewBox="0 0 10 20""#));
        assert_eq!(size(&resized), Some((30.0, 60.0)));
    }

    #[test]
    fn what_is_no_document_is_not_resized() {
        assert_eq!(resized("plain text", 1.0, 1.0), None);
        assert_eq!(resized("<svg><g/></svg>", 1.0, 1.0), None);
    }

    #[test]
    fn an_image_holds_the_document() {
        let made = image("<svg/>".to_owned());
        assert_eq!(made.bytes(), b"<svg/>");
    }
}
