//! The Contents section: what a reader is shown as the contents of the book,
//! where it comes from, the way to each entry, and the controls that change
//! a list the book keeps.

use std::rc::Rc;

use gpui_kit::assets::IconName;
use gpui_kit::component::button::ButtonVariants as _;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::EditorPage;
use super::commands::Origin;
use super::entry_prompt::{EntryRequest, ask_entry, suggested_titles};
use super::place_picker::{Place, heading_choices};
use super::prompts::{Question, confirm};
use super::sections::{control, list_row};
use super::session::{Control, chapter_choice};
use crate::features::edit_book::command::{Command, ContentsChange};
use crate::features::edit_book::outline::{ContentsEntry, ContentsOutline, Fault};
use crate::shared::ui::{icon_button, status_line};

/// What the section says about contents that follow the headings.
pub(super) const AUTOMATIC_HINT: &str = "Readers see these contents. They follow the headings of \
     the chapters: change a heading to rename its entry, add a heading to add one.";
/// What the section says about contents the book keeps as a list of its own.
pub(super) const STORED_HINT: &str = "Readers see these contents. The book keeps them as a list \
     of its own, so they do not change when you edit a heading. Change an entry from the menu at \
     its end.";
/// What the section says when no chapter has a heading.
pub(super) const NO_HEADINGS: &str =
    "No chapter has a heading yet. Give a chapter a heading and it appears here.";
/// The control that makes the contents follow the headings, wherever it is.
pub(super) const FOLLOW_HEADINGS: &str = "Build the contents from the headings";
/// The control that adds an entry to the list.
pub(super) const ADD_ENTRY: &str = "Add an entry…";
/// The control that sorts the list by where its entries lead.
pub(super) const IN_BOOK_ORDER: &str = "Put the entries in the order of the book";
/// The menu item that moves an entry under the one above it.
pub(super) const NEST: &str = "Nest under the entry above";
/// The menu item that moves an entry one level out.
pub(super) const UNNEST: &str = "Move out one level";
/// What a question about the last entry of the list says.
const LAST_ENTRY: &str = "It is the last entry of the list. Without a list of its own, the book \
     shows the headings of its chapters as its contents.";

/// How far an entry is set in for each level under the first, in pixels.
const INDENT: f32 = 20.0;

/// What the section says about entries that lead nowhere.
pub(super) fn nowhere_words(count: usize) -> String {
    if count == 1 {
        "1 entry leads nowhere: its heading is gone. Give it another place, or remove it."
            .to_owned()
    } else {
        format!(
            "{count} entries lead nowhere: their headings are gone. Give each another place, or \
             remove it."
        )
    }
}

/// What the section says about entries that are not in the order of the book.
pub(super) fn order_words(count: usize) -> String {
    if count == 1 {
        "1 entry is not where its place is in the book.".to_owned()
    } else {
        format!("{count} entries are not where their places are in the book.")
    }
}

/// The question before an entry is removed. For the last entry of the list
/// it says what the contents become without a list.
fn remove_question(title: &str, last: bool) -> Question {
    Question {
        title: format!("Remove the entry “{title}”?"),
        detail: if last {
            LAST_ENTRY
        } else {
            "The chapter it leads to stays in the book. This cannot be undone."
        }
        .to_owned(),
        ok: "Remove",
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
            AUTOMATIC_HINT
        } else {
            STORED_HINT
        };
        let mut list = v_flex()
            .id("contents-list")
            .role(Role::Group)
            .aria_label("Contents")
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
                    .then(|| status_line("no-contents", NO_HEADINGS)),
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
                        control("add-entry", ADD_ENTRY)
                            .disabled(pending || self.session.chapters().is_empty())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ask_contents_entry(None, window, cx);
                            })),
                    )
                    .child(
                        control("follow-headings", FOLLOW_HEADINGS).on_click(cx.listener(
                            |_, _, window, cx| {
                                confirm(
                                    Question {
                                        title: "Replace the contents?".to_owned(),
                                        detail:
                                            "The list the book keeps is replaced by the headings \
                                             of its chapters. This cannot be undone."
                                                .to_owned(),
                                        ok: "Replace",
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
                        control("contents-in-order", IN_BOOK_ORDER)
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
            (Some(chapter), None) => format!("in {chapter}"),
            (Some(chapter), Some(Fault::OutOfOrder)) => {
                format!("in {chapter}, not in the order of the book")
            }
            (None, _) | (_, Some(Fault::Nowhere)) => "leads nowhere".to_owned(),
        };
        let name = format!("{}, level {}, {leads}", entry.title, entry.level);
        let tag = entry.fault.map(|fault| match fault {
            Fault::Nowhere => "leads nowhere",
            Fault::OutOfOrder => "out of order",
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
            format!("Entry actions, {title}"),
        )
        .disabled(disabled)
        .dropdown_menu(move |menu, _, _| {
            let shift = |label: &'static str, deeper: bool, allowed: bool| {
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
            menu.item(PopupMenuItem::new("Edit…").on_click(move |_, window, cx| {
                edit_page
                    .update(cx, |this, cx| {
                        this.ask_contents_entry(Some(index), window, cx);
                    })
                    .ok();
            }))
            .separator()
            .item(shift(NEST, true, moves.nest))
            .item(shift(UNNEST, false, moves.unnest))
            .separator()
            .item(
                PopupMenuItem::new("Remove…").on_click(move |_, window, cx| {
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
    use super::{LAST_ENTRY, nowhere_words, order_words, remove_question};

    #[test]
    fn the_question_names_the_entry_and_says_what_stays() {
        let question = remove_question("Opening", false);
        assert_eq!(question.title, "Remove the entry “Opening”?");
        assert!(question.detail.contains("stays in the book"));
        assert_eq!(question.ok, "Remove");
    }

    #[test]
    fn the_question_about_the_last_entry_says_what_the_contents_become() {
        assert_eq!(remove_question("Opening", true).detail, LAST_ENTRY);
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
