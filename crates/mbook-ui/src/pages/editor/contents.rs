//! The Contents section: what a reader is shown as the contents of the book,
//! where it comes from, the way to each entry, and the controls that change
//! a list the book keeps.

use std::borrow::Cow;
use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use rust_i18n::t;

use super::EditorPage;
use super::commands::Origin;
use super::entry_prompt::{EntryRequest, ask_entry, suggested_titles};
use super::place_picker::{Place, heading_choices};
use super::prompts::{Question, confirm};
use super::sections::{control, list_row};
use super::session::{Control, chapter_choice};
use crate::features::edit_book::command::{Command, ContentsChange};
use crate::features::edit_book::outline::{ContentsEntry, ContentsOutline, Fault};
use crate::shared::i18n::count_text;
use crate::shared::ui::{icon_button, status_line};

/// What the section says about contents that follow the headings.
pub(super) fn automatic_hint() -> Cow<'static, str> {
    t!("editor.contents.automatic_hint")
}
/// What the section says about contents the book keeps as a list of its own.
pub(super) fn stored_hint() -> Cow<'static, str> {
    t!("editor.contents.stored_hint")
}
/// What the section says when no chapter has a heading.
pub(super) fn no_headings() -> Cow<'static, str> {
    t!("editor.contents.no_headings")
}
/// The control that makes the contents follow the headings, wherever it is.
pub(super) fn follow_headings() -> Cow<'static, str> {
    t!("editor.contents.follow_headings")
}
/// The control that adds an entry to the list.
pub(super) fn add_entry() -> Cow<'static, str> {
    t!("editor.contents.add_entry")
}
/// The control that sorts the list by where its entries lead.
pub(super) fn in_book_order() -> Cow<'static, str> {
    t!("editor.contents.in_book_order")
}
/// The menu item that moves an entry under the one above it.
pub(super) fn nest() -> Cow<'static, str> {
    t!("editor.contents.nest")
}
/// The menu item that moves an entry one level out.
pub(super) fn unnest() -> Cow<'static, str> {
    t!("editor.contents.unnest")
}
/// What a question about the last entry of the list says.
fn last_entry() -> Cow<'static, str> {
    t!("editor.contents.last_entry")
}

/// How far an entry is set in for each level under the first, in pixels.
const INDENT: f32 = 20.0;

/// What the section says about entries that lead nowhere.
pub(super) fn nowhere_words(count: usize) -> String {
    count_text("editor.contents.nowhere", count)
}

/// What the section says about entries that are not in the order of the book.
pub(super) fn order_words(count: usize) -> String {
    count_text("editor.contents.out_of_order", count)
}

/// The question before an entry is removed. For the last entry of the list
/// it says what the contents become without a list.
fn remove_question(title: &str, last: bool) -> Question {
    Question {
        title: t!("editor.contents.remove_title", title = title).into_owned(),
        detail: if last {
            last_entry()
        } else {
            t!("editor.contents.remove_detail")
        }
        .into_owned(),
        ok: t!("prompt.remove.ok").into(),
    }
}

/// What a stored entry can do, as its menu offers it.
#[derive(Debug, Clone, Copy)]
struct EntryMoves {
    /// Its position in the list.
    index: usize,
    /// It can move one level in, under the entry above it.
    nest: bool,
    /// It can move one level out.
    unnest: bool,
    /// It is the only entry of the list.
    last: bool,
}

impl EditorPage {
    /// The Contents section.
    pub(super) fn render_contents(&self, cx: &Context<'_, Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let contents = self.session.contents().cloned().unwrap_or_default();
        let hint = if contents.automatic {
            automatic_hint()
        } else {
            stored_hint()
        };
        let mut list = v_flex()
            .id("contents-list")
            .role(Role::Group)
            .aria_label(t!("editor.section.contents"))
            .gap_1();
        for (index, entry) in contents.entries.iter().enumerate() {
            list = list.child(self.render_entry(index, entry, &contents, cx));
        }
        v_flex()
            .id("contents")
            .gap_3()
            .p_4()
            .max_w(px(640.0))
            .w_full()
            .child(status_line("contents-hint", hint).text_color(theme.muted_foreground))
            .children(
                (contents.automatic && contents.entries.is_empty())
                    .then(|| status_line("no-contents", no_headings())),
            )
            .children((!contents.automatic).then(|| self.render_list_controls(&contents, cx)))
            .children(
                self.session
                    .error_text(&Control::Contents)
                    .map(|message| status_line("contents-error", message).text_color(theme.danger)),
            )
            .child(list)
            .into_any_element()
    }

    /// The controls over a list the book keeps: the ones that add an entry
    /// and give the list up for the headings, and what is wrong with the
    /// entries, with the control that sorts them.
    fn render_list_controls(
        &self,
        contents: &ContentsOutline,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let pending = self.session.contents_pending;
        let count = |fault: Fault| {
            let found = contents.entries.iter();
            found.filter(|entry| entry.fault == Some(fault)).count()
        };
        let (nowhere, unordered) = (count(Fault::Nowhere), count(Fault::OutOfOrder));
        v_flex()
            .gap_2()
            .child(
                h_flex()
                    .gap_2()
                    .flex_wrap()
                    .child(
                        control("add-entry", add_entry())
                            .disabled(pending || self.session.chapters().is_empty())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ask_contents_entry(None, window, cx);
                            })),
                    )
                    .child(
                        control("follow-headings", follow_headings()).on_click(cx.listener(
                            |_, _, window, cx| {
                                confirm(
                                    Question {
                                        title: t!("editor.contents.replace_title").into_owned(),
                                        detail: t!("editor.contents.replace_detail").into_owned(),
                                        ok: t!("editor.contents.replace_ok").into(),
                                    },
                                    window,
                                    cx,
                                    Self::follow_headings,
                                );
                            },
                        )),
                    ),
            )
            .children((nowhere > 0).then(|| {
                status_line("contents-nowhere", nowhere_words(nowhere)).text_color(theme.danger)
            }))
            .children((unordered > 0).then(|| {
                v_flex()
                    .gap_2()
                    .child(
                        status_line("contents-order", order_words(unordered))
                            .text_color(theme.danger),
                    )
                    .child(
                        control("contents-in-order", in_book_order())
                            .self_start()
                            .disabled(pending)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.edit_contents(ContentsChange::InBookOrder, window, cx);
                            })),
                    )
            }))
    }

    /// One entry: its title, set in by its level, and what is wrong with
    /// where it leads. Choosing it opens the chapter it leads to. An entry of
    /// a list the book keeps ends with the menu that changes it.
    fn render_entry(
        &self,
        index: usize,
        entry: &ContentsEntry,
        contents: &ContentsOutline,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let theme = cx.theme().clone();
        let chapter = self.session.chapter_index(&entry.chapter).map(|at| {
            let heading = self.session.chapters()[at].heading.as_deref();
            chapter_choice(at + 1, heading)
        });
        let leads = match (&chapter, entry.fault) {
            (Some(chapter), None) => t!("editor.entry.in_chapter", chapter = chapter),
            (Some(chapter), Some(Fault::OutOfOrder)) => {
                t!("editor.entry.in_chapter_out_of_order", chapter = chapter)
            }
            (None, _) | (_, Some(Fault::Nowhere)) => t!("editor.entry.leads_nowhere"),
        };
        let name = t!(
            "editor.entry.name",
            title = entry.title,
            level = entry.level,
            leads = leads
        );
        let tag = entry.fault.map(|fault| match fault {
            Fault::Nowhere => t!("editor.entry.leads_nowhere"),
            Fault::OutOfOrder => t!("editor.entry.out_of_order"),
        });
        let indent = f32::from(entry.level.saturating_sub(1)) * INDENT;
        let path = entry.chapter.clone();
        let row = list_row(
            ("contents-entry", index),
            name,
            h_flex()
                .w_full()
                .gap_2()
                .pl(px(indent))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_left()
                        .child(entry.title.clone()),
                )
                .children(tag.map(|tag| {
                    div()
                        .flex_none()
                        .text_xs()
                        .text_color(theme.danger)
                        .child(tag)
                })),
        )
        .ghost()
        .disabled(chapter.is_none())
        .on_click(cx.listener(move |this, _, window, cx| {
            this.open_chapter(&path, window, cx);
        }));
        let above = index
            .checked_sub(1)
            .and_then(|above| contents.entries.get(above))
            .map_or(0, |above| above.level);
        let moves = EntryMoves {
            index,
            nest: entry.level <= above,
            unnest: entry.level > 1,
            last: contents.entries.len() == 1,
        };
        let pending = self.session.contents_pending;
        h_flex()
            .w_full()
            .gap_1()
            .child(div().flex_1().min_w_0().child(row))
            .children(
                (!contents.automatic)
                    .then(|| Self::entry_menu(moves, entry.title.clone(), pending, cx)),
            )
    }

    /// The menu of an entry of a list the book keeps: change it, move it in
    /// or out, remove it. It is `disabled` while a change of the list is in
    /// flight.
    fn entry_menu(
        moves: EntryMoves,
        title: String,
        disabled: bool,
        cx: &Context<'_, Self>,
    ) -> impl IntoElement + use<> {
        let page = cx.weak_entity();
        let index = moves.index;
        let title: Rc<str> = title.into();
        icon_button(
            ("entry-menu", index),
            IconName::Ellipsis,
            t!("editor.entry.actions", title = title),
        )
        .disabled(disabled)
        .dropdown_menu(move |menu, _, _| {
            let shift = |label: Cow<'static, str>, deeper: bool, allowed: bool| {
                let page = page.clone();
                PopupMenuItem::new(label)
                    .disabled(!allowed)
                    .on_click(move |_, window, cx| {
                        let change = ContentsChange::Shift { index, deeper };
                        page.update(cx, |this, cx| this.edit_contents(change, window, cx))
                            .ok();
                    })
            };
            let (edit_page, remove_page) = (page.clone(), page.clone());
            let title = title.clone();
            menu.item(
                PopupMenuItem::new(t!("editor.entry.edit")).on_click(move |_, window, cx| {
                    edit_page
                        .update(cx, |this, cx| {
                            this.ask_contents_entry(Some(index), window, cx);
                        })
                        .ok();
                }),
            )
            .separator()
            .item(shift(nest(), true, moves.nest))
            .item(shift(unnest(), false, moves.unnest))
            .separator()
            .item(
                PopupMenuItem::new(t!("editor.entry.remove")).on_click(move |_, window, cx| {
                    let question = remove_question(&title, moves.last);
                    remove_page
                        .update(cx, |_, cx| {
                            confirm(question, window, cx, move |this, window, cx| {
                                let change = ContentsChange::Remove { index };
                                this.edit_contents(change, window, cx);
                            });
                        })
                        .ok();
                }),
            )
        })
    }

    /// Opens the dialog of an entry: the one at `index`, or a new one that
    /// starts at the chapter being edited.
    pub(super) fn ask_contents_entry(
        &self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        let chapters = self.session.chapters();
        let titles = suggested_titles(chapters);
        let existing = index.and_then(|index| {
            let contents = self.session.contents()?;
            contents.entries.get(index)
        });
        let (title, place) = match (existing, index) {
            (Some(entry), _) => (
                entry.title.clone(),
                Place {
                    chapter: Some(entry.chapter.clone()),
                    anchor: entry.anchor.clone(),
                },
            ),
            (None, Some(_)) => return,
            (None, None) => {
                let start = self.session.open.as_ref();
                let Some(chapter) = start.or(chapters.first().map(|first| &first.path)) else {
                    return;
                };
                let place = Place {
                    chapter: Some(chapter.clone()),
                    anchor: None,
                };
                (titles.get(&place).cloned().unwrap_or_default(), place)
            }
        };
        ask_entry(
            EntryRequest {
                index,
                title,
                place,
                choices: heading_choices(chapters),
                titles: Rc::new(titles),
            },
            window,
            cx,
        );
    }

    /// Sends a change of the list. Until the book has answered, the entries
    /// shown are those from before it, so no other change is taken.
    pub(super) fn edit_contents(
        &mut self,
        change: ContentsChange,
        window: &mut Window,
        cx: &mut Context<'_, Self>,
    ) {
        if self.session.contents_pending {
            return;
        }
        self.session.contents_pending = true;
        self.flush_text(window, cx);
        self.send(Command::EditContents(change), Origin::Contents, window, cx);
        cx.notify();
    }

    /// Makes the contents follow the headings of the chapters.
    pub(super) fn follow_headings(&mut self, window: &mut Window, cx: &mut Context<'_, Self>) {
        self.flush_text(window, cx);
        self.send(Command::ContentsFromHeadings, Origin::Contents, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{last_entry, nowhere_words, order_words, remove_question};

    #[test]
    fn the_question_names_the_entry_and_says_what_stays() {
        let question = remove_question("Opening", false);
        assert_eq!(question.title, "Remove the entry “Opening”?");
        assert!(question.detail.contains("stays in the book"));
        assert_eq!(question.ok, "Remove");
    }

    #[test]
    fn the_question_about_the_last_entry_says_what_the_contents_become() {
        assert_eq!(remove_question("Opening", true).detail, last_entry());
    }

    #[test]
    fn the_entries_at_fault_are_counted_in_words() {
        assert!(nowhere_words(1).starts_with("1 entry leads nowhere: its heading"));
        assert!(nowhere_words(3).starts_with("3 entries lead nowhere: their headings"));
        assert_eq!(
            order_words(1),
            "1 entry is not where its place is in the book."
        );
        assert_eq!(
            order_words(2),
            "2 entries are not where their places are in the book."
        );
    }
}
