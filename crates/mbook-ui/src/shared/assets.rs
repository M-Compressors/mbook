//! The icons and reading fonts embedded in the application.

use std::borrow::Cow;

use gpui_kit::assets::icon_assets;
use gpui_kit::{App, AssetSource, Result, SharedString};

// The default bundle of GPUI Kit already holds `ArrowLeft`, `Plus`,
// `Settings`, `Search`, `BookOpen`, `CircleAlert`, `Loader`, `Ellipsis`,
// `ChevronLeft`/`Right`, `Check` and `Info`. Name here an icon a page needs
// that the bundle lacks; never copy an SVG into the repository.
icon_assets!(
    AppIcons,
    [
        Library,
        Pencil,
        Trash,
        FilePlus,
        Download,
        List,
        AArrowDown,
        AArrowUp,
        Code,
        Columns2,
        FilePenLine,
        FoldVertical,
        UnfoldVertical,
        SunMoon,
        Type,
        Link,
        ImagePlus,
        Images,
        FileCode,
        Bold,
        Italic,
        Strikethrough,
        TextQuote,
        ListIndentDecrease,
        ListIndentIncrease,
        ZoomIn,
        ZoomOut,
        Scan
    ]
);

/// The asset source of the application: its own icons first, then the default
/// bundle of GPUI Kit.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some(icon) = AppIcons.load(path)? {
            return Ok(Some(icon));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

/// Registers local faces before any reading view is constructed.
pub(crate) fn register_fonts(cx: &App) {
    macro_rules! face {
        ($name:literal) => {
            include_bytes!(concat!("../../assets/fonts/", $name, ".ttf")).as_slice()
        };
    }
    for font in [
        face!("Literata-Regular"),
        face!("Literata-Medium"),
        face!("Literata-Bold"),
        face!("Literata-Italic"),
        face!("Literata-MediumItalic"),
        face!("Literata-BoldItalic"),
        face!("NotoSans-Regular"),
        face!("NotoSans-Medium"),
        face!("NotoSans-Bold"),
        face!("NotoSans-Italic"),
        face!("NotoSans-MediumItalic"),
        face!("NotoSans-BoldItalic"),
    ] {
        // A failed face leaves the platform's fallback available.
        cx.text_system().add_fonts(vec![Cow::Borrowed(font)]).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_of_the_application_is_found() {
        assert!(Assets.load("icons/pencil.svg").unwrap().is_some());
    }

    #[test]
    fn every_redesign_icon_is_embedded() {
        for name in [
            "a-arrow-down",
            "a-arrow-up",
            "code",
            "columns-2",
            "file-pen-line",
            "fold-vertical",
            "unfold-vertical",
            "sun-moon",
            "a-large-small",
            "arrow-up",
            "arrow-down",
            "circle-check",
            "eye",
            "sort-ascending",
        ] {
            assert!(
                Assets.load(&format!("icons/{name}.svg")).unwrap().is_some(),
                "{name}"
            );
        }
    }

    #[test]
    fn an_icon_of_the_default_bundle_is_found() {
        assert!(Assets.load("icons/arrow-left.svg").unwrap().is_some());
    }

    #[test]
    fn the_visual_editing_icons_are_embedded() {
        for name in [
            "type",
            "link",
            "image-plus",
            "images",
            "ellipsis",
            "plus",
            "file-code",
            "undo-2",
            "redo-2",
            "bold",
            "italic",
            "strikethrough",
            "text-quote",
            "list-indent-decrease",
            "list-indent-increase",
        ] {
            assert!(
                Assets.load(&format!("icons/{name}.svg")).unwrap().is_some(),
                "{name}"
            );
        }
    }
}
