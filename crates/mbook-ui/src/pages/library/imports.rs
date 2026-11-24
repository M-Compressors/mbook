//! The imports that are running or have finished, as a list of rows: in the
//! sidebar of an expanded window and in a row above the bar of a compact one.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, TestSupportExt as _, Window,
    div, px,
};
use rust_i18n::t;

use crate::features::import_book::{ImportEntry, ImportQueue, ImportStatus};
use crate::shared::ui::{icon_button, status_line};

/// The id of the list of imports.
pub(super) const LIST_ID: &str = "imports";

/// The list of imports, or `None` when there are none.
///
/// `max_height` is how tall the list may grow before it scrolls.
pub(super) fn imports_panel(
    queue: &Entity<ImportQueue>,
    max_height: f32,
    cx: &App,
) -> Option<AnyElement> {
    let entries = queue.read(cx).entries();
    if entries.is_empty() {
        return None;
    }
    let theme = cx.theme();
    let finished = queue.read(cx).finished_count();
    let heading = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .child(
            div()
                .id("imports-heading")
                .role(Role::Heading)
                .aria_level(2)
                .aria_label(t!("library.imports"))
                .test_support()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .child(SharedString::from(t!("library.imports"))),
        )
        .children((finished > 0).then(|| {
            let queue = queue.clone();
            Button::new("imports-clear")
                .ghost()
                .small()
                .label(t!("library.clear_finished"))
                .cursor_pointer()
                .on_click(move |_, _, cx| {
                    queue.update(cx, ImportQueue::clear_finished);
                })
                .into_any_element()
        }));
    Some(
        v_flex()
            .id(LIST_ID)
            .role(Role::Group)
            .aria_label(t!("library.imports"))
            .test_support()
            .w_full()
            .gap_2()
            .p_2()
            .border_t_1()
            .border_color(theme.border)
            .child(heading)
            .child(
                v_flex()
                    .id("imports-rows")
                    .w_full()
                    .max_h(px(max_height))
                    .overflow_y_scroll()
                    .gap_2()
                    .children(entries.iter().map(|entry| import_row(entry, queue, cx))),
            )
            .into_any_element(),
    )
}

/// One import: what it is, where it is, and what can be done with it.
fn import_row(entry: &ImportEntry, queue: &Entity<ImportQueue>, cx: &App) -> AnyElement {
    let theme = cx.theme();
    let id = entry.id();
    let name = entry.display_name().to_owned();
    let status = entry.status();
    let failed = matches!(status, ImportStatus::Failed { .. });
    let warnings = match status {
        ImportStatus::Done { warnings, .. } if !warnings.is_empty() => warnings.clone(),
        _ => Vec::new(),
    };
    h_flex()
        .id(format!("import-{id}"))
        .w_full()
        .items_start()
        .gap_2()
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_sm().truncate().child(name.clone()))
                .child(
                    status_line(format!("import-status-{id}"), status.label())
                        .text_xs()
                        .text_color(if failed {
                            theme.danger
                        } else {
                            theme.muted_foreground
                        }),
                ),
        )
        .children(status.is_running().then(|| {
            div()
                .flex_none()
                .pt_1()
                .child(Spinner::new().small())
                .into_any_element()
        }))
        .children((!warnings.is_empty()).then(|| {
            let count = warnings.len();
            let shown = name.clone();
            Button::new(format!("import-warnings-{id}"))
                .ghost()
                .small()
                .label(crate::shared::i18n::count_text("library.warnings", count))
                .accessibility_label(t!("library.show_warnings", name = name))
                .cursor_pointer()
                .on_click(move |_, window, cx| show_warnings(&shown, &warnings, window, cx))
                .into_any_element()
        }))
        .children(status.is_finished().then(|| {
            let queue = queue.clone();
            icon_button(
                format!("import-dismiss-{id}"),
                IconName::Close,
                t!("library.dismiss"),
            )
            .accessibility_label(t!("library.dismiss_import", name = entry.display_name()))
            .on_click(move |_, _, cx| {
                queue.update(cx, |queue, cx| {
                    if let Some(index) = queue.entries().iter().position(|e| e.id() == id) {
                        queue.dismiss(index, cx);
                    }
                });
            })
            .into_any_element()
        }))
        .into_any_element()
}

/// Lists what the conversion could not keep.
pub(super) fn show_warnings(name: &str, warnings: &[String], window: &mut Window, cx: &mut App) {
    let title = SharedString::from(t!("library.warnings_for", name = name));
    let warnings = warnings.to_vec();
    window.open_alert_dialog(cx, move |alert, _, _| {
        alert
            .title(title.clone())
            .description(
                v_flex()
                    .id("warnings-list")
                    .max_h(px(240.0))
                    .overflow_y_scroll()
                    .gap_1()
                    .children(warnings.iter().map(|warning| div().child(warning.clone()))),
            )
            .ok_text(t!("library.close"))
    });
}
