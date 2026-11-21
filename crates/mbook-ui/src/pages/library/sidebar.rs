//! The sidebar: the search field, the sections and the imports.
//!
//! One entity serves both places the sidebar can be: docked beside the shelf
//! of an expanded window, and inside the sheet of a medium one.

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::sidebar::{Sidebar, SidebarMenu};
use gpui_kit::component::{
    ActiveTheme as _, Icon, Selectable as _, WindowExt as _, h_flex, v_flex,
};
use gpui_kit::{
    Context, Entity, InteractiveElement as _, IntoElement, ParentElement as _, Render, Role,
    StatefulInteractiveElement as _, Styled as _, Subscription, TestSupportExt as _, WeakEntity,
    Window, div, px,
};

use super::LibraryPage;
use super::imports::imports_panel;
use crate::features::import_book::ImportQueue;
use crate::features::search_library::Section;
use crate::shared::ui::target_size;

/// The width of the docked sidebar, in pixels.
pub(super) const WIDTH: f32 = 280.0;

/// How tall the list of imports may grow in the sidebar, in pixels.
const IMPORTS_HEIGHT: f32 = 240.0;

/// The sidebar of the library.
pub(super) struct LibrarySidebar {
    page: WeakEntity<LibraryPage>,
    search: Entity<InputState>,
    queue: Entity<ImportQueue>,
    _subscriptions: Vec<Subscription>,
}

impl LibrarySidebar {
    /// A sidebar for `page` that follows the page and the imports.
    pub(super) fn new(
        page: WeakEntity<LibraryPage>,
        search: Entity<InputState>,
        queue: Entity<ImportQueue>,
        cx: &mut Context<'_, Self>,
    ) -> Self {
        let mut subscriptions = vec![cx.observe(&queue, |_, _, cx| cx.notify())];
        if let Some(page) = page.upgrade() {
            subscriptions.push(cx.observe(&page, |_, _, cx| cx.notify()));
        }
        Self {
            page,
            search,
            queue,
            _subscriptions: subscriptions,
        }
    }
}

/// The field that filters the shelf, in the sidebar and on the search tab of
/// a compact window.
pub(super) fn search_field(search: &Entity<InputState>) -> Input {
    Input::new(search)
        .aria_label("Search the library")
        .cleanable(true)
        .prefix(Icon::new(IconName::Search))
}

impl Render for LibrarySidebar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<'_, Self>) -> impl IntoElement {
        let Some(page) = self.page.upgrade() else {
            return div().into_any_element();
        };
        let (current, counts) = {
            let page = page.read(cx);
            (page.section(), page.counts())
        };
        let sections = v_flex()
            .id("sections")
            .role(Role::Group)
            .aria_label("Sections")
            .test_support()
            .w_full()
            .gap_1()
            .children(Section::ALL.into_iter().map(|section| {
                let active = section == current;
                let count = counts.of(section);
                let label = section.label();
                let page = self.page.clone();
                Button::new(format!("section-{}", section_slug(section)))
                    .ghost()
                    .w_full()
                    .h(px(target_size()))
                    .selected(active)
                    .toggled(active)
                    .accessibility_label(format!(
                        "{label}, {count}{}",
                        if active { ", selected" } else { "" }
                    ))
                    .cursor_pointer()
                    .child(
                        h_flex().w_full().justify_between().child(label).child(
                            div()
                                .text_color(cx.theme().muted_foreground)
                                .child(count.to_string()),
                        ),
                    )
                    .on_click(move |_, window, cx| {
                        page.update(cx, |page, cx| page.set_section(section, cx))
                            .ok();
                        // A sheet has done its work once a section is chosen.
                        if window.has_active_sheet(cx) {
                            window.close_sheet(cx);
                        }
                    })
            }));
        let header = v_flex()
            .w_full()
            .gap_3()
            .child(search_field(&self.search))
            .child(sections);
        let sidebar = Sidebar::<SidebarMenu>::new("library-sidebar")
            .collapsible(false)
            .w(px(WIDTH))
            .header(header);
        match imports_panel(&self.queue, IMPORTS_HEIGHT, cx) {
            Some(imports) => sidebar.footer(imports),
            None => sidebar,
        }
        .into_any_element()
    }
}

/// The part of the id of a section's button that names the section.
pub(super) fn section_slug(section: Section) -> &'static str {
    match section {
        Section::All => "all",
        Section::Reading => "reading",
        Section::Finished => "finished",
    }
}
