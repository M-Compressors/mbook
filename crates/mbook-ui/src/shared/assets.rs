//! What the application embeds: the icons of its controls and, later, the
//! typeface of books.

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
        Link2Off,
        Image,
        FileCode,
        Bold,
        Italic,
        Strikethrough,
        TextQuote,
        ListIndentDecrease,
        ListIndentIncrease
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

/// The font files the application embeds, as bytes.
///
/// None yet: Literata is embedded here once its files are in `assets/`.
fn embedded_fonts() -> Vec<Cow<'static, [u8]>> {
    // Literata is embedded here once its files are in assets/
    Vec::new()
}

/// Registers the embedded fonts with the text system. Until a font is
/// embedded this does nothing, and books are set in the typeface the
/// appearance falls back to.
pub(crate) fn register_fonts(cx: &App) {
    let fonts = embedded_fonts();
    if fonts.is_empty() {
        return;
    }
    // A font that fails to register leaves the fallback in place; nothing else
    // depends on it.
    cx.text_system().add_fonts(fonts).ok();
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
            "link-2-off",
            "image",
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

    #[test]
    fn no_font_is_embedded_yet() {
        assert_eq!(embedded_fonts().len(), 0);
    }
}
