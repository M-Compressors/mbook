//! Changing a contents list the book keeps, entry by entry.
//!
//! The format wants the rows of the contents in reading order, and each at
//! most one level under the row before it (RFC 0001, "Resource 3: `toc`").
//! A list that follows both rules still follows them after any change made
//! here, so nobody has to order or number entries by hand: an entry is put
//! where its place is in the book, and the entries under one that moves in,
//! out or away move with it. The entries of a chapter also follow it when it
//! moves in the reading order or gets another path, and go with it when it
//! is removed.

use super::command::{ContentsChange, EntryDraft};
use super::error::ContentsError;
use super::outline::Fault;
use mbook::{Book, Chapter, TocItem};
use std::collections::HashMap;

/// Where an entry leads: the position of its chapter in the reading order,
/// and the offset in the text of that chapter.
type Position = (usize, u32);

/// The places of a book that an entry of the contents can lead to: the start
/// of a chapter and its headings.
pub(super) struct Places<'a> {
    chapters: HashMap<&'a str, (usize, &'a Chapter)>,
}

impl<'a> Places<'a> {
    /// The places of this book.
    pub(super) fn of(book: &'a Book) -> Self {
        Self {
            chapters: book
                .chapters()
                .iter()
                .enumerate()
                .map(|(index, chapter)| (chapter.path(), (index, chapter)))
                .collect(),
        }
    }

    /// Where a chapter, or a heading of it, is. `None` when the book has no
    /// such chapter or the chapter no heading with this name; a line that an
    /// anchor definition names is not a place of the contents.
    fn position(&self, chapter: &str, anchor: Option<&str>) -> Option<Position> {
        let (index, chapter) = self.chapters.get(chapter)?;
        let offset = match anchor {
            None => 0,
            Some(name) => {
                chapter
                    .anchors()
                    .iter()
                    .find(|anchor| anchor.name == name)?
                    .offset
            }
        };
        Some((*index, offset))
    }

    /// Where an entry leads.
    fn of_item(&self, item: &TocItem) -> Option<Position> {
        self.position(&item.chapter, item.anchor.as_deref())
    }
}

/// What `Book::check` would say about where each entry leads, entry by
/// entry: the same walk as the one that writes the contents.
pub(super) fn faults(items: &[TocItem], places: &Places<'_>) -> Vec<Option<Fault>> {
    let mut previous = (0, 0);
    items
        .iter()
        .map(|item| {
            let Some(place) = places.of_item(item) else {
                return Some(Fault::Nowhere);
            };
            let fault = (place < previous).then_some(Fault::OutOfOrder);
            previous = place;
            fault
        })
        .collect()
}

/// Whether the entries are in the order of the book. An entry that leads
/// nowhere has no place to be out of.
pub(super) fn in_order(items: &[TocItem], places: &Places<'_>) -> bool {
    !faults(items, places).contains(&Some(Fault::OutOfOrder))
}

/// Makes the entries that lead into the chapter at `from` lead into the
/// chapter at `to`: the chapter got another path.
pub(super) fn follow_path(items: &mut [TocItem], from: &str, to: &str) {
    for item in items.iter_mut().filter(|item| item.chapter == from) {
        to.clone_into(&mut item.chapter);
    }
}

/// Takes out the entries that lead into a chapter, because the chapter is
/// gone, and says how many there were. The entries under each of them move
/// one level up, as they do when a single entry is removed.
pub(super) fn forget_chapter(items: &mut Vec<TocItem>, path: &str) -> usize {
    let before = items.len();
    // The levels of the entries taken out that still enclose the one at hand.
    let mut gone: Vec<u8> = Vec::new();
    items.retain_mut(|item| {
        while gone.last().is_some_and(|level| *level >= item.level) {
            gone.pop();
        }
        if item.chapter == path {
            gone.push(item.level);
            return false;
        }
        let lifted = u8::try_from(gone.len()).unwrap_or(u8::MAX);
        item.level = item.level.saturating_sub(lifted).max(1);
        true
    });
    before - items.len()
}

/// Makes one change to a list.
///
/// # Errors
///
/// The list has no entry at the position, the title is empty, the place is
/// not one of the book, or the entry cannot move in or out. The list is then
/// as it was.
pub(super) fn change(
    items: &mut Vec<TocItem>,
    change: ContentsChange,
    places: &Places<'_>,
) -> Result<(), ContentsError> {
    match change {
        ContentsChange::Add(draft) => {
            let position = place_of(&draft, places)?;
            insert(items, draft, None, position, places);
        }
        ContentsChange::Set { index, entry } => set(items, index, entry, places)?,
        ContentsChange::Shift { index, deeper } => shift(items, index, deeper)?,
        ContentsChange::Remove { index } => {
            remove(items, index)?;
        }
        ContentsChange::InBookOrder => put_in_order(items, places),
    }
    Ok(())
}

/// Where a draft leads, when it has a title and its place is in the book.
fn place_of(draft: &EntryDraft, places: &Places<'_>) -> Result<Position, ContentsError> {
    if draft.title.trim().is_empty() {
        return Err(ContentsError::NoTitle);
    }
    places
        .position(&draft.chapter, draft.anchor.as_deref())
        .ok_or_else(|| ContentsError::NoSuchPlace {
            title: draft.title.clone(),
        })
}

/// Puts an entry after the last one that leads to the same place or an
/// earlier one.
///
/// `level` is the level it had; a new entry takes the level of the entry
/// above it. Either is brought between what the entry above allows and what
/// the entry below needs.
fn insert(
    items: &mut Vec<TocItem>,
    draft: EntryDraft,
    level: Option<u8>,
    position: Position,
    places: &Places<'_>,
) {
    let index = items
        .iter()
        .rposition(|item| places.of_item(item).is_some_and(|at| at <= position))
        .map_or(0, |last| last + 1);
    let above = level_before(items, index);
    let shallowest = items
        .get(index)
        .map_or(1, |below| below.level.saturating_sub(1))
        .max(1);
    let level = level
        .unwrap_or(above)
        .max(shallowest)
        .min(above.saturating_add(1));
    items.insert(
        index,
        TocItem {
            level,
            title: draft.title,
            chapter: draft.chapter,
            anchor: draft.anchor,
        },
    );
}

/// The level of the entry above a position; 0 above the first.
fn level_before(items: &[TocItem], index: usize) -> u8 {
    index
        .checked_sub(1)
        .and_then(|above| items.get(above))
        .map_or(0, |above| above.level)
}

/// Gives an entry another title or place. An entry that leads elsewhere
/// moves to where that place is in the book and leaves the entries under it
/// behind, one level up.
fn set(
    items: &mut Vec<TocItem>,
    index: usize,
    draft: EntryDraft,
    places: &Places<'_>,
) -> Result<(), ContentsError> {
    let position = place_of(&draft, places)?;
    let entry = items
        .get_mut(index)
        .ok_or(ContentsError::NoSuchEntry(index))?;
    if places.of_item(entry) == Some(position) {
        entry.title = draft.title;
        entry.chapter = draft.chapter;
        entry.anchor = draft.anchor;
    } else {
        let level = remove(items, index)?.level;
        insert(items, draft, Some(level), position, places);
    }
    Ok(())
}

/// Moves an entry, with the entries under it, one level in or out.
fn shift(items: &mut [TocItem], index: usize, deeper: bool) -> Result<(), ContentsError> {
    let level = items
        .get(index)
        .ok_or(ContentsError::NoSuchEntry(index))?
        .level;
    if deeper && level > level_before(items, index) {
        return Err(ContentsError::TooDeep);
    }
    if !deeper && level <= 1 {
        return Err(ContentsError::TopLevel);
    }
    let family = items
        .iter_mut()
        .skip(index)
        .enumerate()
        .take_while(|(nth, item)| *nth == 0 || item.level > level);
    for (_, item) in family {
        item.level = if deeper {
            item.level.saturating_add(1)
        } else {
            item.level.saturating_sub(1)
        };
    }
    Ok(())
}

/// Takes an entry out. The entries under it move one level up, so that none
/// is left deeper than the entry above it allows.
fn remove(items: &mut Vec<TocItem>, index: usize) -> Result<TocItem, ContentsError> {
    if index >= items.len() {
        return Err(ContentsError::NoSuchEntry(index));
    }
    let removed = items.remove(index);
    items
        .iter_mut()
        .skip(index)
        .take_while(|item| item.level > removed.level)
        .for_each(|item| item.level = item.level.saturating_sub(1));
    Ok(removed)
}

/// Sorts the entries by where they lead. Entries that lead to one place keep
/// their order; an entry that leads nowhere stays behind the entry it
/// followed. A level the new order does not allow is brought up.
pub(super) fn put_in_order(items: &mut Vec<TocItem>, places: &Places<'_>) {
    let mut last = (0, 0);
    let mut keyed: Vec<(Position, TocItem)> = items
        .drain(..)
        .map(|item| {
            if let Some(at) = places.of_item(&item) {
                last = at;
            }
            (last, item)
        })
        .collect();
    keyed.sort_by_key(|(at, _)| *at);
    let mut above = 0u8;
    items.extend(keyed.into_iter().map(|(_, mut item)| {
        item.level = item.level.clamp(1, above.saturating_add(1));
        above = item.level;
        item
    }));
}

#[cfg(test)]
mod tests {
    use super::{Places, change, faults, follow_path, forget_chapter, in_order};
    use crate::features::edit_book::command::{ContentsChange, EntryDraft};
    use crate::features::edit_book::error::ContentsError;
    use crate::features::edit_book::outline::Fault;
    use mbook::{Book, TocItem};

    type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

    /// Three chapters; the first has two headings under its title and a
    /// line with a name of its own.
    fn book() -> Result<Book> {
        let mut book = Book::new("T", "en")?;
        book.add_chapter(
            "a.md",
            "# One\n\n## Early\n\n[fig]: #fig\ntext\n\n## Late\n\ntext\n",
        )?;
        book.add_chapter("b.md", "# Two\n")?;
        book.add_chapter("c.md", "# Three\n")?;
        Ok(book)
    }

    fn draft(title: &str, href: &str) -> EntryDraft {
        let item = TocItem::new(1, title, href);
        EntryDraft {
            title: item.title,
            chapter: item.chapter,
            anchor: item.anchor,
        }
    }

    /// The list as lines of `level title -> destination`.
    fn lines(items: &[TocItem]) -> Vec<String> {
        items
            .iter()
            .map(|item| {
                let anchor = item
                    .anchor
                    .as_ref()
                    .map_or_else(String::new, |anchor| format!("#{anchor}"));
                format!("{} {} -> {}{anchor}", item.level, item.title, item.chapter)
            })
            .collect()
    }

    /// One, with Early and Late under it, then Two.
    fn list() -> Vec<TocItem> {
        vec![
            TocItem::new(1, "One", "a.md"),
            TocItem::new(2, "Early", "a.md#early"),
            TocItem::new(3, "Late", "a.md#late"),
            TocItem::new(1, "Two", "b.md"),
        ]
    }

    fn changed(mut items: Vec<TocItem>, what: ContentsChange) -> Result<Vec<String>> {
        let book = book()?;
        change(&mut items, what, &Places::of(&book))?;
        Ok(lines(&items))
    }

    fn refused(mut items: Vec<TocItem>, what: ContentsChange) -> Result<Option<ContentsError>> {
        let book = book()?;
        let before = items.clone();
        let error = change(&mut items, what, &Places::of(&book)).err();
        assert_eq!(items, before, "a refused change leaves the list as it was");
        Ok(error)
    }

    #[test]
    fn a_new_entry_goes_where_its_place_is_at_the_level_of_the_entry_above() -> Result {
        let lines = changed(
            vec![
                TocItem::new(1, "One", "a.md"),
                TocItem::new(2, "Early", "a.md#early"),
                TocItem::new(1, "Two", "b.md"),
            ],
            ContentsChange::Add(draft("Late", "a.md#late")),
        )?;
        assert_eq!(
            lines,
            [
                "1 One -> a.md",
                "2 Early -> a.md#early",
                "2 Late -> a.md#late",
                "1 Two -> b.md"
            ]
        );
        Ok(())
    }

    #[test]
    fn a_new_entry_before_every_other_is_at_the_first_level() -> Result {
        let lines = changed(
            vec![TocItem::new(1, "Two", "b.md")],
            ContentsChange::Add(draft("One", "a.md")),
        )?;
        assert_eq!(lines, ["1 One -> a.md", "1 Two -> b.md"]);
        Ok(())
    }

    #[test]
    fn a_new_entry_follows_the_entries_that_lead_to_the_same_place() -> Result {
        let lines = changed(
            vec![TocItem::new(1, "Two", "b.md")],
            ContentsChange::Add(draft("Two again", "b.md")),
        )?;
        assert_eq!(lines, ["1 Two -> b.md", "1 Two again -> b.md"]);
        Ok(())
    }

    #[test]
    fn a_new_entry_is_deep_enough_for_the_entry_below_it() -> Result {
        let lines = changed(
            vec![
                TocItem::new(1, "One", "a.md"),
                TocItem::new(2, "Sub", "a.md"),
                TocItem::new(3, "Late", "a.md#late"),
            ],
            ContentsChange::Add(draft("Early", "a.md#early")),
        )?;
        assert_eq!(lines[2], "2 Early -> a.md#early");
        Ok(())
    }

    #[test]
    fn an_entry_without_a_title_is_refused() -> Result {
        let error = refused(list(), ContentsChange::Add(draft("  ", "b.md")))?;
        assert_eq!(error, Some(ContentsError::NoTitle));
        Ok(())
    }

    #[test]
    fn an_entry_that_leads_to_no_chapter_or_heading_is_refused() -> Result {
        for href in ["gone.md", "a.md#gone", "a.md#fig"] {
            let error = refused(list(), ContentsChange::Add(draft("Lost", href)))?;
            assert_eq!(
                error,
                Some(ContentsError::NoSuchPlace {
                    title: "Lost".to_owned()
                }),
                "{href}"
            );
        }
        Ok(())
    }

    #[test]
    fn a_new_title_keeps_the_entry_where_it_is() -> Result {
        let lines = changed(
            list(),
            ContentsChange::Set {
                index: 1,
                entry: draft("Sooner", "a.md#early"),
            },
        )?;
        assert_eq!(lines[1], "2 Sooner -> a.md#early");
        assert_eq!(lines[2], "3 Late -> a.md#late");
        Ok(())
    }

    #[test]
    fn an_entry_that_leads_elsewhere_moves_there_and_leaves_its_entries_behind() -> Result {
        let lines = changed(
            list(),
            ContentsChange::Set {
                index: 1,
                entry: draft("Three", "c.md"),
            },
        )?;
        assert_eq!(
            lines,
            [
                "1 One -> a.md",
                "2 Late -> a.md#late",
                "1 Two -> b.md",
                "2 Three -> c.md"
            ]
        );
        Ok(())
    }

    #[test]
    fn an_entry_that_led_nowhere_is_put_in_its_place_when_it_gets_one() -> Result {
        let lines = changed(
            vec![
                TocItem::new(1, "Lost", "gone.md"),
                TocItem::new(1, "One", "a.md"),
            ],
            ContentsChange::Set {
                index: 0,
                entry: draft("Two", "b.md"),
            },
        )?;
        assert_eq!(lines, ["1 One -> a.md", "1 Two -> b.md"]);
        Ok(())
    }

    #[test]
    fn changing_an_entry_the_list_does_not_have_is_refused() -> Result {
        let changes = [
            ContentsChange::Set {
                index: 4,
                entry: draft("Two", "b.md"),
            },
            ContentsChange::Shift {
                index: 4,
                deeper: false,
            },
            ContentsChange::Remove { index: 4 },
        ];
        for what in changes {
            assert_eq!(refused(list(), what)?, Some(ContentsError::NoSuchEntry(4)));
        }
        Ok(())
    }

    #[test]
    fn an_entry_moves_in_with_the_entries_under_it() -> Result {
        let lines = changed(
            vec![
                TocItem::new(1, "One", "a.md"),
                TocItem::new(1, "Early", "a.md#early"),
                TocItem::new(2, "Late", "a.md#late"),
                TocItem::new(1, "Two", "b.md"),
            ],
            ContentsChange::Shift {
                index: 1,
                deeper: true,
            },
        )?;
        assert_eq!(
            lines,
            [
                "1 One -> a.md",
                "2 Early -> a.md#early",
                "3 Late -> a.md#late",
                "1 Two -> b.md"
            ]
        );
        Ok(())
    }

    #[test]
    fn an_entry_moves_out_with_the_entries_under_it() -> Result {
        let lines = changed(
            list(),
            ContentsChange::Shift {
                index: 1,
                deeper: false,
            },
        )?;
        assert_eq!(
            lines,
            [
                "1 One -> a.md",
                "1 Early -> a.md#early",
                "2 Late -> a.md#late",
                "1 Two -> b.md"
            ]
        );
        Ok(())
    }

    #[test]
    fn an_entry_cannot_move_in_under_nothing_or_twice_under_the_entry_above() -> Result {
        for index in [0, 1] {
            let error = refused(
                list(),
                ContentsChange::Shift {
                    index,
                    deeper: true,
                },
            )?;
            assert_eq!(error, Some(ContentsError::TooDeep), "entry {index}");
        }
        Ok(())
    }

    #[test]
    fn an_entry_of_the_first_level_cannot_move_out() -> Result {
        let error = refused(
            list(),
            ContentsChange::Shift {
                index: 3,
                deeper: false,
            },
        )?;
        assert_eq!(error, Some(ContentsError::TopLevel));
        Ok(())
    }

    #[test]
    fn removing_an_entry_moves_the_entries_under_it_one_level_up() -> Result {
        let lines = changed(list(), ContentsChange::Remove { index: 0 })?;
        assert_eq!(
            lines,
            [
                "1 Early -> a.md#early",
                "2 Late -> a.md#late",
                "1 Two -> b.md"
            ]
        );
        Ok(())
    }

    #[test]
    fn the_order_of_the_book_sorts_the_entries_and_brings_levels_up() -> Result {
        let lines = changed(
            vec![
                TocItem::new(1, "Two", "b.md"),
                TocItem::new(2, "Lost", "gone.md"),
                TocItem::new(1, "One", "a.md"),
                TocItem::new(2, "Late", "a.md#late"),
                TocItem::new(2, "Early", "a.md#early"),
            ],
            ContentsChange::InBookOrder,
        )?;
        assert_eq!(
            lines,
            [
                "1 One -> a.md",
                "2 Early -> a.md#early",
                "2 Late -> a.md#late",
                "1 Two -> b.md",
                "2 Lost -> gone.md"
            ]
        );
        Ok(())
    }

    #[test]
    fn the_order_of_the_book_starts_at_the_first_level() -> Result {
        let lines = changed(
            vec![
                TocItem::new(1, "Two", "b.md"),
                TocItem::new(2, "Early", "a.md#early"),
            ],
            ContentsChange::InBookOrder,
        )?;
        assert_eq!(lines, ["1 Early -> a.md#early", "1 Two -> b.md"]);
        Ok(())
    }

    #[test]
    fn an_entry_is_at_fault_when_it_leads_nowhere_or_back() -> Result {
        let book = book()?;
        let items = [
            TocItem::new(1, "Two", "b.md"),
            TocItem::new(1, "Lost", "gone.md"),
            TocItem::new(1, "Named line", "a.md#fig"),
            TocItem::new(1, "One", "a.md"),
            TocItem::new(1, "Three", "c.md"),
        ];
        assert_eq!(
            faults(&items, &Places::of(&book)),
            [
                None,
                Some(Fault::Nowhere),
                Some(Fault::Nowhere),
                Some(Fault::OutOfOrder),
                None
            ]
        );
        Ok(())
    }

    #[test]
    fn a_list_in_the_order_of_the_book_has_no_faults() -> Result {
        let book = book()?;
        assert_eq!(faults(&list(), &Places::of(&book)), [None; 4]);
        assert!(in_order(&list(), &Places::of(&book)));
        Ok(())
    }

    #[test]
    fn an_entry_that_leads_nowhere_does_not_put_a_list_out_of_order() -> Result {
        let book = book()?;
        let places = Places::of(&book);
        let lost = [
            TocItem::new(1, "Two", "b.md"),
            TocItem::new(1, "Lost", "gone.md"),
        ];
        assert!(in_order(&lost, &places));
        let back = [
            TocItem::new(1, "Two", "b.md"),
            TocItem::new(1, "One", "a.md"),
        ];
        assert!(!in_order(&back, &places));
        Ok(())
    }

    #[test]
    fn the_entries_of_a_chapter_that_is_gone_are_taken_out() {
        let mut items = list();
        assert_eq!(forget_chapter(&mut items, "a.md"), 3);
        assert_eq!(lines(&items), ["1 Two -> b.md"]);
        assert_eq!(forget_chapter(&mut items, "a.md"), 0);
        assert_eq!(lines(&items), ["1 Two -> b.md"]);
    }

    #[test]
    fn the_entries_under_those_of_a_chapter_that_is_gone_move_up() {
        let mut items = vec![
            TocItem::new(1, "Part", "a.md"),
            TocItem::new(2, "One", "a.md#early"),
            TocItem::new(3, "Deep", "b.md"),
            TocItem::new(2, "Two", "b.md"),
            TocItem::new(3, "Late", "a.md#late"),
            TocItem::new(1, "Three", "c.md"),
        ];
        assert_eq!(forget_chapter(&mut items, "a.md"), 3);
        assert_eq!(
            lines(&items),
            ["1 Deep -> b.md", "1 Two -> b.md", "1 Three -> c.md"]
        );
    }

    #[test]
    fn the_entries_of_a_chapter_follow_it_to_another_path() {
        let mut items = list();
        follow_path(&mut items, "a.md", "start.md");
        assert_eq!(
            lines(&items),
            [
                "1 One -> start.md",
                "2 Early -> start.md#early",
                "3 Late -> start.md#late",
                "1 Two -> b.md"
            ]
        );
    }
}
