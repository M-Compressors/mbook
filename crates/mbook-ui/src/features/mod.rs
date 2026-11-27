//! What a user does: importing, editing.

pub(crate) mod agents;
pub(crate) mod appearance;
#[cfg(feature = "self-update")]
pub(crate) mod auto_update;
pub(crate) mod edit_book;
pub(crate) mod import_book;
pub(crate) mod reading_position;
pub(crate) mod search_library;
pub(crate) mod search_text;

/// The update banner for the row below a page header.
#[cfg(feature = "self-update")]
pub(crate) fn update_banner(cx: &gpui_kit::App) -> Option<gpui_kit::AnyElement> {
    auto_update::UpdateBanner::try_global(cx).map(gpui_kit::IntoElement::into_any_element)
}

#[cfg(not(feature = "self-update"))]
pub(crate) fn update_banner(_: &gpui_kit::App) -> Option<gpui_kit::AnyElement> {
    None
}
