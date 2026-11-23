//! The state of an editing session as the page sees it, without a window.
//!
//! The page holds an [`Outline`] of the book, which chapter is open, which
//! text is not yet sent or saved, the error of each control and the pictures
//! that are being encoded. Its transitions are plain methods, and the words
//! the views show are plain functions, so that both can be tested without a
//! window.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::error::Error as _;

use crate::entities::figure::FigureKind;
use crate::features::edit_book::command::MetaChange;
use crate::features::edit_book::error::EditError;
use crate::features::edit_book::outline::{
    ChapterOutline, ContentsOutline, FigureOutline, MetaOutline, Outline, PictureOutline,
    ProblemOutline,
};
use crate::shared::layout::SizeClass;

/// A part of the book the editor shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Section {
    /// The metadata form.
    Details,
    /// The cover picture.
    Cover,
    /// The contents a reader is shown, and where they come from.
    Contents,
    /// The pictures of the book.
    Pictures,
    /// The diagrams or formulas of the book.
    Figures(FigureKind),
    /// Where the text begins and the other places a reader can be taken to.
    Landmarks,
    /// What the last check or save found.
    Problems,
    /// The chapter being edited.
    Chapters,
}

impl Section {
    /// The sections about the whole book, in the order the list shows them.
    /// The chapters stand in a list of their own under them.
    pub(super) const BOOK: [Self; 8] = [
        Self::Details,
        Self::Cover,
        Self::Contents,
        Self::Pictures,
        Self::Figures(FigureKind::Diagram),
        Self::Figures(FigureKind::Formula),
        Self::Landmarks,
        Self::Problems,
    ];

    /// Every section.
    #[cfg(test)]
    pub(super) const ALL: [Self; 9] = [
        Self::Details,
        Self::Cover,
        Self::Contents,
        Self::Pictures,
        Self::Figures(FigureKind::Diagram),
        Self::Figures(FigureKind::Formula),
        Self::Landmarks,
        Self::Problems,
        Self::Chapters,
    ];

    /// The name of the section.
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Details => "Details",
            Self::Cover => "Cover",
            Self::Contents => "Contents",
            Self::Chapters => "Chapters",
            Self::Landmarks => "Landmarks",
            Self::Pictures => "Pictures",
            Self::Figures(FigureKind::Diagram) => "Diagrams",
            Self::Figures(FigureKind::Formula) => "Formulas",
            Self::Problems => "Problems",
        }
    }

    /// The identifier of the control that opens the section.
    pub(super) const fn id(self) -> &'static str {
        match self {
            Self::Details => "section-details",
            Self::Cover => "section-cover",
            Self::Contents => "section-contents",
            Self::Chapters => "section-chapters",
            Self::Landmarks => "section-landmarks",
            Self::Pictures => "section-pictures",
            Self::Figures(FigureKind::Diagram) => "section-diagrams",
            Self::Figures(FigureKind::Formula) => "section-formulas",
            Self::Problems => "section-problems",
        }
    }
}

/// What the chapter area shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ViewMode {
    /// Formatted text that can be edited directly.
    Visual,
    /// The Markdown editor alone.
    Source,
    /// The editor and the preview side by side.
    Split,
    /// The preview alone.
    Preview,
}

impl ViewMode {
    /// The name of the choice.
    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Visual => "Visual",
            Self::Source => "Source",
            Self::Split => "Split",
            Self::Preview => "Preview",
        }
    }

    /// The choices a window of this size class offers.
    pub(super) const fn available(class: SizeClass) -> &'static [Self] {
        match class {
            SizeClass::Expanded => &[Self::Visual, Self::Source, Self::Split, Self::Preview],
            SizeClass::Medium | SizeClass::Compact => &[Self::Visual, Self::Source, Self::Preview],
        }
    }

    /// The choice that is drawn: split is only for an expanded window and
    /// shows the source below it.
    pub(super) const fn effective(self, class: SizeClass) -> Self {
        match (self, class) {
            (Self::Split, SizeClass::Medium | SizeClass::Compact) => Self::Source,
            _ => self,
        }
    }
}

/// A field of the Details form.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum MetaField {
    /// The title.
    Title,
    /// The authors, one per line.
    Authors,
    /// The language.
    Language,
    /// The series and its number.
    Series,
    /// The publisher.
    Publisher,
    /// The publication date.
    Date,
    /// The annotation.
    Annotation,
}

impl MetaField {
    /// Every field, in the order of the form.
    pub(super) const ALL: [Self; 7] = [
        Self::Title,
        Self::Authors,
        Self::Language,
        Self::Series,
        Self::Publisher,
        Self::Date,
        Self::Annotation,
    ];
}

/// The control an error belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum Control {
    /// A field of the Details form.
    Meta(MetaField),
    /// The text of the open chapter.
    ChapterText,
    /// Adding, moving, renaming or removing a chapter.
    Chapters,
    /// Changing the landmarks in the Landmarks section.
    Landmarks,
    /// Adding or removing a picture.
    Pictures,
    /// Adding, editing or removing a diagram or formula.
    Figures(FigureKind),
    /// Adding a picture from a file into the chapter being edited.
    ChapterPicture,
    /// Replacing the cover.
    Cover,
    /// The contents.
    Contents,
    /// The save.
    Save,
}

/// Where the opening of the book is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Phase {
    /// The book is being opened.
    Loading,
    /// The outline has arrived.
    Ready,
    /// The book could not be opened; the reason is in words.
    Failed(String),
}

/// What an outline changed besides the lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct Applied {
    /// The outline was newer than the last one and was taken.
    pub(super) taken: bool,
    /// It was the first outline of the session.
    pub(super) first: bool,
    /// The chapter that is open changed, so its text must be loaded.
    pub(super) open_changed: bool,
}

/// The state of the page.
#[derive(Debug)]
pub(super) struct Session {
    /// How far the opening is.
    pub(super) phase: Phase,
    /// The book as the last command left it.
    pub(super) outline: Option<Outline>,
    /// The section in front.
    pub(super) section: Section,
    /// The chapter in the editor.
    pub(super) open: Option<String>,
    /// What the chapter area shows.
    pub(super) mode: ViewMode,
    /// Chapters whose text was changed since the last save.
    edited: BTreeSet<String>,
    /// The chapter whose text was typed and not sent.
    pending: Option<String>,
    /// Texts the book refused, kept so that nothing typed is lost.
    drafts: BTreeMap<String, String>,
    /// What went wrong, by control.
    errors: HashMap<Control, String>,
    /// Pictures that are queued or being encoded.
    pub(super) encoding: usize,
    /// A replacement of the landmarks is waiting for its outline.
    pub(super) landmarks_pending: bool,
    /// A change of the contents list is waiting for its outline; the
    /// positions the list shows are those of the outline before it.
    pub(super) contents_pending: bool,
    /// The request number of the outline that was taken last.
    applied: u64,
    /// The place of a problem in words, by chapter and offset.
    pub(super) places: HashMap<(String, u32), (usize, usize)>,
}

impl Session {
    /// A session that has not heard from the book yet.
    pub(super) fn new() -> Self {
        Self {
            phase: Phase::Loading,
            outline: None,
            section: Section::Chapters,
            open: None,
            mode: ViewMode::Visual,
            edited: BTreeSet::new(),
            pending: None,
            drafts: BTreeMap::new(),
            errors: HashMap::new(),
            encoding: 0,
            landmarks_pending: false,
            contents_pending: false,
            applied: 0,
            places: HashMap::new(),
        }
    }

    /// Takes the outline that answers request `seq`, unless a newer one was
    /// taken already. A chapter that is open and gone gives way to its
    /// neighbour; with none open, the first is opened.
    pub(super) fn apply_outline(&mut self, seq: u64, outline: Outline) -> Applied {
        if seq <= self.applied {
            return Applied::default();
        }
        self.applied = seq;
        let first = self.outline.is_none();
        let before = self.open.clone();
        let old_index = before.as_deref().and_then(|path| self.chapter_index(path));
        if !outline.unsaved {
            self.edited.clear();
        }
        self.phase = Phase::Ready;
        self.outline = Some(outline);
        let chapters = self.chapters();
        let still_there = before
            .as_deref()
            .is_some_and(|path| chapters.iter().any(|chapter| chapter.path == path));
        if !still_there {
            let index = old_index.unwrap_or(0).min(chapters.len().saturating_sub(1));
            self.open = chapters.get(index).map(|chapter| chapter.path.clone());
        }
        if self.problems().is_empty() {
            self.places.clear();
        }
        Applied {
            taken: true,
            first,
            open_changed: self.open != before,
        }
    }

    /// The book could not be opened.
    pub(super) fn fail(&mut self, reason: String) {
        self.phase = Phase::Failed(reason);
    }

    /// The chapters in reading order; empty before the outline arrives.
    pub(super) fn chapters(&self) -> &[ChapterOutline] {
        self.outline
            .as_ref()
            .map_or(&[], |outline| outline.chapters.as_slice())
    }

    /// The landmarks in the order they are stored in the book.
    pub(super) fn landmarks(&self) -> &[mbook::Landmark] {
        self.outline
            .as_ref()
            .map_or(&[], |outline| outline.landmarks.as_slice())
    }

    /// How many landmarks, of every kind, lead into a chapter.
    pub(super) fn landmarks_into(&self, path: &str) -> usize {
        let landmarks = self.landmarks().iter();
        landmarks
            .filter(|landmark| landmark.chapter == path)
            .count()
    }

    /// How many entries of a contents list the book keeps lead into a
    /// chapter. Contents that follow the headings have no entries to lose.
    pub(super) fn stored_entries_of(&self, path: &str) -> usize {
        self.contents()
            .filter(|contents| !contents.automatic)
            .map_or(0, |contents| {
                let entries = contents.entries.iter();
                entries.filter(|entry| entry.chapter == path).count()
            })
    }

    /// The contents of the book; `None` before the outline arrives.
    pub(super) fn contents(&self) -> Option<&ContentsOutline> {
        self.outline.as_ref().map(|outline| &outline.contents)
    }

    /// The chapter with this path.
    pub(super) fn chapter(&self, path: &str) -> Option<&ChapterOutline> {
        self.chapters().iter().find(|chapter| chapter.path == path)
    }

    /// The pictures of the book.
    pub(super) fn pictures(&self) -> &[PictureOutline] {
        self.outline
            .as_ref()
            .map_or(&[], |outline| outline.pictures.as_slice())
    }

    /// The diagrams and formulas of the book.
    pub(super) fn figures(&self) -> &[FigureOutline] {
        self.outline
            .as_ref()
            .map_or(&[], |outline| outline.figures.as_slice())
    }

    /// The problems the last check or save found.
    pub(super) fn problems(&self) -> &[ProblemOutline] {
        self.outline
            .as_ref()
            .map_or(&[], |outline| outline.problems.as_slice())
    }

    /// The metadata of the book.
    pub(super) fn meta(&self) -> Option<&MetaOutline> {
        self.outline.as_ref().map(|outline| &outline.meta)
    }

    /// The position of a chapter in reading order.
    pub(super) fn chapter_index(&self, path: &str) -> Option<usize> {
        self.chapters()
            .iter()
            .position(|chapter| chapter.path == path)
    }

    /// The title of the book as the editor has it.
    pub(super) fn title(&self) -> Option<&str> {
        self.meta().map(|meta| meta.title.as_str())
    }

    /// Whether leaving would lose something: a change of the book, a text
    /// that was typed, a text the book refused, or an edit still in flight.
    pub(super) fn has_unsaved(&self) -> bool {
        self.outline.as_ref().is_some_and(|outline| outline.unsaved)
            || !self.edited.is_empty()
            || self.pending.is_some()
            || !self.drafts.is_empty()
            || self.encoding > 0
            || self.landmarks_pending
            || self.contents_pending
    }

    /// The words of the header about the state of the book.
    pub(super) fn status(&self, saving: bool) -> Option<&'static str> {
        if saving {
            Some("Saving")
        } else if self.has_unsaved() {
            Some("Unsaved changes")
        } else {
            None
        }
    }

    /// Whether a chapter has text that was typed and not saved.
    pub(super) fn is_edited(&self, path: &str) -> bool {
        self.edited.contains(path) || self.drafts.contains_key(path)
    }

    /// The text of the open chapter was changed by typing.
    pub(super) fn text_typed(&mut self, path: &str) {
        self.pending = Some(path.to_owned());
        self.edited.insert(path.to_owned());
    }

    /// Takes the chapter whose text must be sent, if any.
    pub(super) fn take_pending(&mut self) -> Option<String> {
        self.pending.take()
    }

    /// Whether text was typed and not sent.
    #[cfg(test)]
    pub(super) fn has_pending(&self) -> bool {
        self.pending.is_some()
    }

    /// The book took the text of a chapter.
    pub(super) fn text_accepted(&mut self, path: &str) {
        self.drafts.remove(path);
        if self.open.as_deref() == Some(path) {
            self.errors.remove(&Control::ChapterText);
        }
    }

    /// The book refused the text of a chapter: it is kept as a draft.
    pub(super) fn text_refused(&mut self, path: &str, text: String, message: String) {
        self.drafts.insert(path.to_owned(), text);
        self.edited.insert(path.to_owned());
        if self.open.as_deref() == Some(path) {
            self.errors.insert(Control::ChapterText, message);
        }
    }

    /// The text a refused chapter keeps, to show again when it is reopened.
    pub(super) fn draft(&self, path: &str) -> Option<&str> {
        self.drafts.get(path).map(String::as_str)
    }

    /// Whether the book refused some text and has not taken it since.
    #[cfg(test)]
    pub(super) fn has_refused_text(&self) -> bool {
        !self.drafts.is_empty()
    }

    /// The first chapter with a refused text.
    pub(super) fn first_refused(&self) -> Option<&str> {
        self.drafts.keys().next().map(String::as_str)
    }

    /// A chapter got another path: what was kept under the old one follows.
    pub(super) fn rename(&mut self, from: &str, to: &str) {
        if self.open.as_deref() == Some(from) {
            self.open = Some(to.to_owned());
        }
        if self.edited.remove(from) {
            self.edited.insert(to.to_owned());
        }
        if self.pending.as_deref() == Some(from) {
            self.pending = Some(to.to_owned());
        }
        if let Some(text) = self.drafts.remove(from) {
            self.drafts.insert(to.to_owned(), text);
        }
    }

    /// A chapter is gone: nothing is kept for it.
    pub(super) fn forget(&mut self, path: &str) {
        self.edited.remove(path);
        self.drafts.remove(path);
        if self.pending.as_deref() == Some(path) {
            self.pending = None;
        }
    }

    /// Shows the section.
    pub(super) fn show_section(&mut self, section: Section) {
        self.section = section;
    }

    /// Opens a chapter in the editor.
    pub(super) fn show_chapter(&mut self, path: &str) {
        self.section = Section::Chapters;
        self.open = Some(path.to_owned());
    }

    /// Records what went wrong at a control.
    pub(super) fn set_error(&mut self, control: Control, message: String) {
        self.errors.insert(control, message);
    }

    /// Clears the error of a control.
    pub(super) fn clear_error(&mut self, control: &Control) {
        self.errors.remove(control);
    }

    /// What went wrong at a control, in words.
    pub(super) fn error(&self, control: &Control) -> Option<&str> {
        self.errors.get(control).map(String::as_str)
    }

    /// The error of a control as a clone, for a view that moves it.
    pub(super) fn error_text(&self, control: &Control) -> Option<String> {
        self.error(control).map(str::to_owned)
    }
}

/// Words for what went wrong in a command.
pub(super) fn edit_error_words(error: &EditError) -> String {
    let detail = chain(error);
    match error {
        EditError::Open(_) => format!("The book could not be opened: {detail}."),
        EditError::Book(_) => format!("The book refused the change: {detail}."),
        EditError::Save(_) => format!("The book could not be saved: {detail}."),
        EditError::Contents(_) => format!("The contents were not changed: {detail}."),
        EditError::NoSuchPath(path) => format!("Nothing in the book has the path {path}."),
        EditError::Ended => {
            "The editing session has ended. Go back and open the book again.".to_owned()
        }
    }
}

/// The messages of the sources of an error, joined.
fn chain(error: &EditError) -> String {
    let mut parts = Vec::new();
    let mut source = error.source();
    while let Some(cause) = source {
        parts.push(cause.to_string());
        source = cause.source();
    }
    if parts.is_empty() {
        error.to_string()
    } else {
        parts.join(": ")
    }
}

/// The line and the column, both counted from 1, of a byte offset in a text.
/// An offset past the end is the end; one inside a character is the start of
/// that character.
pub(super) fn line_and_column(text: &str, offset: usize) -> (usize, usize) {
    let end = clamp_to_boundary(text, offset);
    let before = &text[..end];
    let line = before.matches('\n').count() + 1;
    let start = before.rfind('\n').map_or(0, |at| at + 1);
    (line, before[start..].chars().count() + 1)
}

/// The largest character boundary of the text that is not past `offset`.
pub(super) fn clamp_to_boundary(text: &str, offset: usize) -> usize {
    let mut at = offset.min(text.len());
    while !text.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// The place of a problem in words, when the chapter and the position are
/// known.
pub(super) fn problem_words(problem: &ProblemOutline, place: Option<(usize, usize)>) -> String {
    let location = match (&problem.path, problem.offset, place) {
        (Some(path), Some(_), Some((line, column))) => {
            format!("{path}, line {line}, column {column}: ")
        }
        (Some(path), Some(offset), None) => format!("{path}, byte {offset}: "),
        (Some(path), None, _) => format!("{path}: "),
        (None, _, _) => String::new(),
    };
    format!("{location}{}", problem.message)
}

/// Whether a problem is about the contents of the book, which the contents
/// can be rebuilt from the headings to settle.
pub(super) fn is_contents_problem(problem: &ProblemOutline) -> bool {
    problem.message.contains("contents item")
}

/// What the announcement of a failed save says.
pub(super) fn unsaved_announcement(problems: usize) -> String {
    let noun = if problems == 1 { "problem" } else { "problems" };
    format!("The book has {problems} {noun} and was not saved")
}

/// What a chapter without a heading is called.
pub(super) const UNTITLED: &str = "Untitled chapter";

/// The title of a chapter as the lists show it: its first heading, or
/// [`UNTITLED`].
pub(super) fn chapter_title(heading: Option<&str>) -> &str {
    heading.unwrap_or(UNTITLED)
}

/// A chapter in a list of choices: its number in reading order and its title.
pub(super) fn chapter_choice(number: usize, heading: Option<&str>) -> String {
    format!("{number}. {}", chapter_title(heading))
}

/// The name of a chapter as its control says it: its number in reading
/// order, its title, its file, and the state that colour or a mark also
/// shows.
pub(super) fn chapter_label(
    number: usize,
    path: &str,
    heading: Option<&str>,
    current: bool,
    edited: bool,
) -> String {
    let mut label = format!("Chapter {number}, {}, {path}", chapter_title(heading));
    if current {
        label.push_str(", current chapter");
    }
    if edited {
        label.push_str(", edited");
    }
    label
}

/// The size of a file in the units a person reads.
pub(super) fn size_label(len: usize) -> String {
    const KIB: usize = 1024;
    const MIB: usize = 1024 * 1024;
    if len < KIB {
        format!("{len} bytes")
    } else if len < MIB {
        format!("{}.{} KiB", len / KIB, len % KIB * 10 / KIB)
    } else {
        format!("{}.{} MiB", len / MIB, len % MIB * 10 / MIB)
    }
}

/// The path proposed for a new chapter: `chNN.md`, the first number after the
/// count of chapters that is free.
pub(super) fn propose_chapter_path(chapters: &[ChapterOutline]) -> String {
    let mut number = chapters.len() + 1;
    loop {
        let path = format!("ch{number:02}.md");
        if chapters.iter().all(|chapter| chapter.path != path) {
            return path;
        }
        number += 1;
    }
}

/// The path proposed for a picture taken from a file: its name with the
/// extension `.avif`, and a number when the book has that path already.
pub(super) fn propose_picture_path(file_name: &str, pictures: &[PictureOutline]) -> String {
    let stem = file_name
        .rsplit_once('.')
        .map_or(file_name, |(stem, _)| stem)
        .trim();
    let stem = if stem.is_empty() { "picture" } else { stem };
    let mut path = format!("{stem}.avif");
    let mut number = 1;
    while pictures.iter().any(|picture| picture.path == path) {
        number += 1;
        path = format!("{stem}-{number}.avif");
    }
    path
}

/// The path the user typed, or what is wrong with it.
///
/// # Errors
///
/// The words that say what to enter, when nothing was.
pub(super) fn clean_path(input: &str) -> Result<String, &'static str> {
    required(input, "Enter a file name, such as img/pond.avif.")
}

/// What was typed without its edges, or `missing` when nothing is left.
///
/// # Errors
///
/// `missing`, the words that say what to enter.
pub(super) fn required(input: &str, missing: &'static str) -> Result<String, &'static str> {
    let value = input.trim();
    if value.is_empty() {
        Err(missing)
    } else {
        Ok(value.to_owned())
    }
}

/// The authors as the form holds them: one name per line.
pub(super) fn authors_from_text(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// What the fields of the Details form hold.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct MetaValues {
    /// The title.
    pub(super) title: String,
    /// The authors, one per line.
    pub(super) authors: String,
    /// The language.
    pub(super) language: String,
    /// The name of the series.
    pub(super) series_name: String,
    /// The position in the series.
    pub(super) series_index: String,
    /// The publisher.
    pub(super) publisher: String,
    /// The publication date.
    pub(super) date: String,
    /// The annotation.
    pub(super) annotation: String,
}

impl MetaValues {
    /// The values the form shows for a book.
    pub(super) fn of(meta: &MetaOutline) -> Self {
        Self {
            title: meta.title.clone(),
            authors: meta.authors.join("\n"),
            language: meta.language.clone(),
            series_name: meta
                .series
                .as_ref()
                .map_or_default(|series| series.name.clone()),
            series_index: meta
                .series
                .as_ref()
                .and_then(|series| series.index.clone())
                .unwrap_or_default(),
            publisher: meta.publisher.clone().unwrap_or_default(),
            date: meta.date.clone().unwrap_or_default(),
            annotation: meta.annotation.clone().unwrap_or_default(),
        }
    }
}

/// `Some` of the text without its edges, or `None` when nothing is left.
fn optional(text: &str) -> Option<String> {
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// The change that makes the book agree with a field, or `None` when it
/// agrees already.
pub(super) fn meta_change(
    field: MetaField,
    values: &MetaValues,
    meta: &MetaOutline,
) -> Option<MetaChange> {
    match field {
        MetaField::Title => {
            let title = values.title.trim();
            (title != meta.title).then(|| MetaChange::Title(title.to_owned()))
        }
        MetaField::Authors => {
            let authors = authors_from_text(&values.authors);
            (authors != meta.authors).then_some(MetaChange::Authors(authors))
        }
        MetaField::Language => {
            let language = values.language.trim();
            (language != meta.language).then(|| MetaChange::Language(language.to_owned()))
        }
        MetaField::Series => {
            let wanted =
                optional(&values.series_name).map(|name| (name, optional(&values.series_index)));
            let current = meta
                .series
                .as_ref()
                .map(|series| (series.name.clone(), series.index.clone()));
            (wanted != current).then_some(MetaChange::Series(wanted))
        }
        MetaField::Publisher => {
            let publisher = optional(&values.publisher);
            (publisher != meta.publisher).then_some(MetaChange::Publisher(publisher))
        }
        MetaField::Date => {
            let date = optional(&values.date);
            (date != meta.date).then_some(MetaChange::Date(date))
        }
        MetaField::Annotation => {
            let annotation = optional(&values.annotation);
            (annotation != meta.annotation).then_some(MetaChange::Annotation(annotation))
        }
    }
}

#[cfg(test)]
mod tests;
