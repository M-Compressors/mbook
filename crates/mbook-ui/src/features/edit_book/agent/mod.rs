//! What an agent asks of an editing session, done in the task that owns the
//! book (mbook-ui docs, "Agents (MCP)").
//!
//! An [`Op`] is one call of a tool on one book. The task runs it on the pool
//! in the order of every other request, so an agent and the editor never
//! change the book at once. A call that fails changes nothing.

pub(crate) mod error;
pub(crate) mod text;
pub(crate) mod types;

use std::collections::HashMap;

use mbook::{Book, Landmark, Meta, Toc, TocItem};
use mbook_converter::figure::{self, Palette, Setting};
use mbook_converter::inline;
use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use self::error::{AgentError, chain, line_and_column};
use self::text::{Shown, apply, changed, headings, line_count, section, shown, snippet};
use self::types::{
    AddChapter, BookView, ChapterArgs, ChapterChanged, ChapterText, ChapterView, ContentsView,
    Done, EditChapter, EntryView, FigurePut, FigureSource, FigureView, HeadingView, Kind,
    LandmarkView, LandmarksView, Made, Match, Matches, MetaPatch, MetaView, MoveChapter, Patch,
    PathArgs, PathView, PictureAdded, PictureInfo, PictureView, ProblemView, ProblemsView,
    PutFigure, ReadChapter, Render, Saved, Search, Series, SetContents, SetLandmarks, Use, Warning,
    WriteChapter,
};
use super::actor::{self, Session};
use super::outline::{Fault, Outline};

/// The size of body text the size of a figure is given for, in pixels.
const TEXT: f32 = 16.0;
/// At most this many matches of a search.
const MAX_MATCHES: usize = 500;

/// One call of a tool on a book.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    GetBook {
        /// A window edits the book too.
        open_in_editor: bool,
    },
    ReadChapter(ReadChapter),
    Search(Search),
    ReadFigure(PathArgs),
    /// The AVIF file of a picture and what is known of it.
    Picture(PathArgs),
    AddChapter(AddChapter),
    EditChapter(EditChapter),
    WriteChapter(WriteChapter),
    MoveChapter(MoveChapter),
    RemoveChapter(ChapterArgs),
    AddPicture {
        /// Where it goes; derived from `name` when left out.
        path: Option<String>,
        /// The name of the file it came from.
        name: Option<String>,
        avif: Vec<u8>,
        cover: bool,
    },
    RemovePicture(PathArgs),
    PutFigure(PutFigure),
    RemoveFigure(PathArgs),
    SetMeta(MetaPatch),
    SetContents(SetContents),
    SetLandmarks(SetLandmarks),
    Check,
    Save,
}

impl Op {
    /// Whether the call changes the book.
    pub(crate) const fn changes(&self) -> bool {
        !matches!(
            self,
            Self::GetBook { .. }
                | Self::ReadChapter(_)
                | Self::Search(_)
                | Self::ReadFigure(_)
                | Self::Picture(_)
                | Self::Check
                | Self::Save
        )
    }
}

/// What a call answers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Answer {
    /// The structured content.
    pub(crate) value: Value,
    /// A PNG image to show the agent.
    pub(crate) png: Option<Vec<u8>>,
    /// The AVIF file of a picture, for the caller to turn into PNG.
    pub(crate) avif: Option<Vec<u8>>,
    /// The chapters whose text changed.
    pub(crate) chapters: Vec<String>,
    /// The book has changes that are not saved, after the call.
    pub(crate) unsaved: bool,
}

impl Answer {
    /// An answer with `value` as its content.
    pub(crate) fn of(value: &impl Serialize) -> Self {
        Self {
            // The types of this module serialise to JSON without failing.
            value: serde_json::to_value(value).unwrap_or(Value::Null),
            png: None,
            avif: None,
            chapters: Vec::new(),
            unsaved: false,
        }
    }

    fn touching(mut self, chapter: impl Into<String>) -> Self {
        self.chapters.push(chapter.into());
        self
    }
}

/// Runs a call on the session.
pub(crate) fn run(session: &mut Session, op: Op) -> Result<Answer, AgentError> {
    if let Some(chapter) = held_by(&op)
        && session
            .held
            .as_ref()
            .is_some_and(|held| held.path == chapter)
    {
        return Err(AgentError::UserTyping);
    }
    let changes = op.changes();
    let mut answer = match op {
        Op::GetBook { open_in_editor } => Answer::of(&book_view(session, open_in_editor)),
        Op::ReadChapter(args) => Answer::of(&read_chapter(session, &args)?),
        Op::Search(args) => Answer::of(&search(&session.book, &args)?),
        Op::ReadFigure(args) => Answer::of(&read_figure(&session.book, &args.path)?),
        Op::Picture(args) => picture(&session.book, &args.path)?,
        Op::AddChapter(args) => add_chapter(session, args)?,
        Op::EditChapter(args) => edit_chapter(session, &args)?,
        Op::WriteChapter(args) => write_chapter(session, args)?,
        Op::MoveChapter(args) => move_chapter(session, args)?,
        Op::RemoveChapter(args) => {
            actor::remove_chapter(&mut session.book, &args.chapter)?;
            Answer::of(&Done {})
        }
        Op::AddPicture {
            path,
            name,
            avif,
            cover,
        } => Answer::of(&add_picture(
            &mut session.book,
            path,
            name.as_deref(),
            avif,
            cover,
        )?),
        Op::RemovePicture(args) => {
            session
                .book
                .remove_picture(&args.path)
                .ok_or(AgentError::NoSuchPath(args.path))?;
            Answer::of(&Done {})
        }
        Op::PutFigure(args) => put_figure(&mut session.book, args)?,
        Op::RemoveFigure(args) => {
            let book = &mut session.book;
            let removed = book.remove_diagram(&args.path).is_some()
                || book.remove_formula(&args.path).is_some();
            if !removed {
                return Err(AgentError::NoSuchPath(args.path));
            }
            Answer::of(&Done {})
        }
        Op::SetMeta(patch) => {
            set_meta(session.book.meta_mut(), patch)?;
            Answer::of(&meta_view(session.book.meta()))
        }
        Op::SetContents(args) => {
            set_contents(&mut session.book, args)?;
            Answer::of(&contents_view(&session.book))
        }
        Op::SetLandmarks(args) => {
            let landmarks = args
                .landmarks
                .iter()
                .map(|landmark| Landmark::new(&landmark.kind, &landmark.title, &landmark.href))
                .collect();
            session
                .book
                .set_landmarks(landmarks)
                .map_err(|error| AgentError::of_book(&error, None))?;
            Answer::of(&LandmarksView {
                landmarks: landmark_views(&session.book),
            })
        }
        Op::Check => {
            session.problems = session.found();
            Answer::of(&ProblemsView {
                problems: problems(&session.book, None),
            })
        }
        Op::Save => {
            session.save()?;
            Answer::of(&Saved {
                saved: !session.unsaved,
                problems: problems(&session.book, None),
            })
        }
    };
    if changes {
        session.problems.clear();
        session.unsaved = true;
        for chapter in &answer.chapters {
            session.revisions.touch(chapter, true);
        }
    }
    answer.unsaved = session.unsaved;
    Ok(answer)
}

/// The chapter a call changes the text or place of, which the editor may
/// hold.
fn held_by(op: &Op) -> Option<&str> {
    match op {
        Op::EditChapter(args) => Some(&args.chapter),
        Op::WriteChapter(args) => Some(&args.chapter),
        Op::MoveChapter(args) => Some(&args.chapter),
        Op::RemoveChapter(args) => Some(&args.chapter),
        _ => None,
    }
}

/// The structure of the book.
fn book_view(session: &Session, open_in_editor: bool) -> BookView {
    let book = &session.book;
    let uses = uses(book);
    let lines_of = |path: &str| -> Vec<Use> {
        uses.get(path).map_or_else(Vec::new, |shown| {
            shown
                .iter()
                .map(|(chapter, shown)| Use {
                    chapter: chapter.clone(),
                    line: shown.line,
                })
                .collect()
        })
    };
    let chapters = book
        .chapters()
        .iter()
        .map(|chapter| {
            let text = chapter.text();
            let names: HashMap<usize, &str> = chapter
                .anchors()
                .iter()
                .map(|anchor| (anchor.offset as usize, anchor.name.as_str()))
                .collect();
            let headings: Vec<HeadingView> = headings(text)
                .into_iter()
                .map(|heading| HeadingView {
                    level: heading.level,
                    anchor: names.get(&heading.offset).map(|name| (*name).to_owned()),
                    title: heading.title,
                })
                .collect();
            ChapterView {
                path: chapter.path().to_owned(),
                lines: line_count(text),
                bytes: text.len(),
                title: headings.first().map(|heading| heading.title.clone()),
                headings,
                anchors: chapter
                    .extra_anchors()
                    .iter()
                    .map(|anchor| anchor.name.clone())
                    .collect(),
            }
        })
        .collect();
    let pictures = book
        .pictures()
        .iter()
        .map(|picture| PictureView {
            path: picture.path().to_owned(),
            width: picture.size().width,
            height: picture.size().height,
            bytes: picture.data().len(),
            alt: alt(&uses, picture.path()),
            used_by: lines_of(picture.path()),
        })
        .collect();
    let figures = book
        .diagrams()
        .iter()
        .map(|diagram| (Kind::Diagram, diagram.path()))
        .chain(
            book.formulas()
                .iter()
                .map(|formula| (Kind::Formula, formula.path())),
        )
        .map(|(kind, path)| FigureView {
            kind,
            path: path.to_owned(),
            used_by: lines_of(path),
        })
        .collect();
    BookView {
        meta: meta_view(book.meta()),
        chapters,
        contents: contents_view(book),
        landmarks: landmark_views(book),
        pictures,
        figures,
        unsaved: session.unsaved,
        problems: book
            .check()
            .err()
            .map_or(0, |problems| problems.iter().count()),
        open_in_editor,
    }
}

/// What every chapter shows, by the path of the resource.
fn uses(book: &Book) -> HashMap<String, Vec<(String, Shown)>> {
    let mut out: HashMap<String, Vec<(String, Shown)>> = HashMap::new();
    for chapter in book.chapters() {
        for found in shown(chapter.path(), chapter.text()) {
            out.entry(found.path.clone())
                .or_default()
                .push((chapter.path().to_owned(), found));
        }
    }
    out
}

/// The alternative text of the first use of a resource that has one.
fn alt(uses: &HashMap<String, Vec<(String, Shown)>>, path: &str) -> Option<String> {
    uses.get(path)?
        .iter()
        .map(|(_, shown)| shown.alt.as_str())
        .find(|alt| !alt.is_empty())
        .map(str::to_owned)
}

/// The metadata as an agent reads it.
fn meta_view(meta: &Meta) -> MetaView {
    MetaView {
        title: meta.title().to_owned(),
        authors: meta.authors().map(str::to_owned).collect(),
        translators: meta.translators().map(str::to_owned).collect(),
        language: meta.language().to_owned(),
        id: meta.id().to_owned(),
        series: meta.series().map(|series| Series {
            name: series.name.to_owned(),
            index: series.index.map(str::to_owned),
        }),
        publisher: meta.publisher().map(str::to_owned),
        date: meta.date().map(str::to_owned),
        isbn: meta.isbns().map(str::to_owned).collect(),
        genres: meta.genres().map(str::to_owned).collect(),
        annotation: meta.annotation().map(str::to_owned),
        cover: meta.cover().map(str::to_owned),
    }
}

/// A link destination: a chapter and, after `#`, an anchor.
fn href(chapter: &str, anchor: Option<&str>) -> String {
    anchor.map_or_else(
        || chapter.to_owned(),
        |anchor| format!("{chapter}#{anchor}"),
    )
}

/// The contents as a reader is shown them.
fn contents_view(book: &Book) -> ContentsView {
    let contents = Outline::of(book, &[], false).contents;
    ContentsView {
        automatic: contents.automatic,
        entries: contents
            .entries
            .into_iter()
            .map(|entry| EntryView {
                level: entry.level,
                href: href(&entry.chapter, entry.anchor.as_deref()),
                title: entry.title,
                fault: entry.fault.map(|fault| {
                    match fault {
                        Fault::Nowhere => "leads to no chapter or heading of the book",
                        Fault::OutOfOrder => "leads before the place of the entry above it",
                    }
                    .to_owned()
                }),
            })
            .collect(),
    }
}

/// The landmarks of the book.
fn landmark_views(book: &Book) -> Vec<LandmarkView> {
    book.landmarks()
        .iter()
        .map(|landmark| LandmarkView {
            kind: landmark.kind.clone(),
            title: landmark.title.clone(),
            href: href(&landmark.chapter, landmark.anchor.as_deref()),
        })
        .collect()
}

/// What a check finds, with lines and columns; only those of `chapter` when
/// one is given.
fn problems(book: &Book, chapter: Option<&str>) -> Vec<ProblemView> {
    let Err(found) = book.check() else {
        return Vec::new();
    };
    found
        .iter()
        .filter(|problem| chapter.is_none() || problem.path.as_deref() == chapter)
        .map(|problem| {
            let text = problem.path.as_deref().and_then(|path| {
                book.chapter(path)
                    .map(mbook::Chapter::text)
                    .or_else(|| book.diagram(path).map(mbook::Diagram::source))
                    .or_else(|| book.formula(path).map(mbook::Formula::source))
            });
            let place = problem
                .offset
                .zip(text)
                .map(|(offset, text)| line_and_column(text, offset as usize));
            ProblemView {
                chapter: problem.path.clone(),
                line: place.map(|(line, _)| line),
                column: place.map(|(_, column)| column),
                message: problem.kind.to_string(),
            }
        })
        .collect()
}

/// A part of a chapter.
fn read_chapter(session: &Session, args: &ReadChapter) -> Result<ChapterText, AgentError> {
    let chapter = session
        .book
        .chapter(&args.chapter)
        .ok_or_else(|| AgentError::NoSuchPath(args.chapter.clone()))?;
    let text = chapter.text();
    let range = match (&args.anchor, args.lines) {
        (Some(_), Some(_)) => {
            return Err(AgentError::InvalidArguments(
                "give `anchor` or `lines`, not both".into(),
            ));
        }
        (Some(anchor), None) => {
            let offset = chapter
                .anchors()
                .iter()
                .chain(chapter.extra_anchors())
                .find(|found| found.name == *anchor)
                .ok_or_else(|| AgentError::NoSuchPath(format!("{}#{anchor}", args.chapter)))?
                .offset;
            section(text, offset as usize)
        }
        (None, Some(lines)) => text::lines(text, lines).0,
        (None, None) => 0..text.len(),
    };
    let part = &text[range.clone()];
    let first_line = if text.is_empty() {
        0
    } else {
        text::line_of(text, range.start)
    };
    Ok(ChapterText {
        text: part.to_owned(),
        revision: session.revisions.of(&args.chapter),
        first_line,
        last_line: first_line + line_count(part).saturating_sub(1),
        total_lines: line_count(text),
    })
}

/// The places of the book where the query is.
fn search(book: &Book, args: &Search) -> Result<Matches, AgentError> {
    let pattern = if args.regex {
        args.query.clone()
    } else {
        regex::escape(&args.query)
    };
    if args.query.is_empty() {
        return Err(AgentError::InvalidArguments("the query is empty".into()));
    }
    let pattern =
        Regex::new(&pattern).map_err(|error| AgentError::InvalidArguments(error.to_string()))?;
    let limit = args.limit.unwrap_or(50).clamp(1, MAX_MATCHES);
    let mut matches = Vec::new();
    let mut truncated = false;
    'chapters: for chapter in book.chapters() {
        for (index, line) in chapter.text().lines().enumerate() {
            for found in pattern.find_iter(line) {
                if matches.len() == limit {
                    truncated = true;
                    break 'chapters;
                }
                let column = line[..found.start()].chars().count() + 1;
                matches.push(Match {
                    chapter: chapter.path().to_owned(),
                    line: index + 1,
                    column,
                    snippet: snippet(line, column),
                });
            }
        }
    }
    Ok(Matches { matches, truncated })
}

/// The lines that show a resource.
fn used_by(book: &Book, path: &str) -> Vec<Use> {
    uses(book).remove(path).map_or_else(Vec::new, |shown| {
        shown
            .into_iter()
            .map(|(chapter, shown)| Use {
                chapter,
                line: shown.line,
            })
            .collect()
    })
}

/// The source of a diagram or formula.
fn read_figure(book: &Book, path: &str) -> Result<FigureSource, AgentError> {
    let (kind, source) = book
        .diagram(path)
        .map(|diagram| (Kind::Diagram, diagram.source()))
        .or_else(|| {
            book.formula(path)
                .map(|formula| (Kind::Formula, formula.source()))
        })
        .ok_or_else(|| AgentError::NoSuchPath(path.to_owned()))?;
    Ok(FigureSource {
        kind,
        source: source.to_owned(),
        used_by: used_by(book, path),
    })
}

/// A picture of the book and what is known of it.
fn picture(book: &Book, path: &str) -> Result<Answer, AgentError> {
    let picture = book
        .picture(path)
        .ok_or_else(|| AgentError::NoSuchPath(path.to_owned()))?;
    let uses = uses(book);
    let mut answer = Answer::of(&PictureInfo {
        width: picture.size().width,
        height: picture.size().height,
        bytes: picture.data().len(),
        alt: alt(&uses, path),
        used_by: used_by(book, path),
    });
    answer.avif = Some(picture.data().to_vec());
    Ok(answer)
}

/// Where a text goes.
enum Place {
    /// It replaces the text of this chapter.
    Existing(String),
    /// It is a new chapter at this path and position.
    New(String, Option<usize>),
}

/// Stores a text, after moving the figures written in its new parts into
/// resources. All or nothing.
fn store(
    session: &mut Session,
    place: &Place,
    text: &str,
    new: &[std::ops::Range<usize>],
    allow_unrendered: bool,
) -> Result<Answer, AgentError> {
    let path = match place {
        Place::Existing(path) | Place::New(path, _) => path.clone(),
    };
    let found = inline::extract(text, &path, &session.book, new);
    let fresh: Vec<_> = found.figures.iter().filter(|figure| figure.new).collect();
    if !allow_unrendered {
        for figure in &fresh {
            if let Err(error) = draw(figure.kind.into(), &figure.source) {
                return Err(AgentError::InvalidFigure {
                    kind: figure.kind.into(),
                    line: Some(figure.line),
                    message: error,
                });
            }
        }
    }
    let book = &mut session.book;
    let mut added = Vec::new();
    let mut outcome = Ok(());
    for figure in &fresh {
        let result = match figure.kind {
            inline::Kind::Diagram => book.add_diagram(&figure.path, &figure.source).map(drop),
            inline::Kind::Formula => book.add_formula(&figure.path, &figure.source).map(drop),
        };
        match result {
            Ok(()) => added.push(figure.path.clone()),
            Err(error) => {
                outcome = Err(AgentError::of_book(&error, None));
                break;
            }
        }
    }
    if outcome.is_ok() {
        outcome = match place {
            Place::Existing(path) => match book.chapter_mut(path) {
                Some(chapter) => chapter.set_text(found.text.clone()).map(drop),
                None => Err(mbook::Error::NoSuchPath(path.clone())),
            },
            Place::New(path, None) => book.add_chapter(path.clone(), found.text.clone()).map(drop),
            Place::New(path, Some(index)) => book
                .insert_chapter(*index, path.clone(), found.text.clone())
                .map(drop),
        }
        .map_err(|error| AgentError::of_book(&error, Some(&found.text)));
    }
    if let Err(error) = outcome {
        for path in added {
            drop(book.remove_diagram(&path));
            drop(book.remove_formula(&path));
        }
        return Err(error);
    }
    session.revisions.touch(&path, true);
    let warnings = found
        .figures
        .iter()
        .filter(|figure| figure.new && !figure.described)
        .map(|figure| Warning {
            code: "missing_alt".into(),
            line: figure.line,
            message: match figure.kind {
                inline::Kind::Diagram => "the diagram has no `%% alt:` line; its source stands in",
                inline::Kind::Formula => {
                    "the formula has its source as alternative text; a reference with words is better"
                }
            }
            .into(),
        })
        .collect();
    let made = found
        .figures
        .iter()
        .map(|figure| Made {
            kind: figure.kind.into(),
            path: figure.path.clone(),
            line: figure.line,
            new: figure.new,
        })
        .collect();
    Ok(Answer::of(&ChapterChanged {
        revision: session.revisions.of(&path),
        problems: problems(&session.book, Some(&path)),
        path: path.clone(),
        figures: made,
        warnings,
    })
    .touching(path))
}

impl From<inline::Kind> for Kind {
    fn from(kind: inline::Kind) -> Self {
        match kind {
            inline::Kind::Diagram => Self::Diagram,
            inline::Kind::Formula => Self::Formula,
        }
    }
}

/// Draws a figure, or says why it cannot be drawn.
fn draw(kind: Kind, source: &str) -> Result<figure::Figure, String> {
    match kind {
        Kind::Diagram => figure::diagram(source, Palette::default()),
        Kind::Formula => figure::formula(source, Setting::Block, Palette::default()),
    }
    .map_err(|error| error.to_string())
}

/// The first free path of the form `chNN.md`.
fn free_chapter(book: &Book) -> String {
    (1..=u32::MAX)
        .map(|number| format!("ch{number:02}.md"))
        .find(|path| book.chapter(path).is_none() && !taken(book, path))
        .unwrap_or_default()
}

/// Whether anything of the book has the path.
fn taken(book: &Book, path: &str) -> bool {
    book.chapter(path).is_some()
        || book.picture(path).is_some()
        || book.diagram(path).is_some()
        || book.formula(path).is_some()
}

fn add_chapter(session: &mut Session, args: AddChapter) -> Result<Answer, AgentError> {
    let path = args.path.unwrap_or_else(|| free_chapter(&session.book));
    if let Some(index) = args.position
        && index > session.book.chapters().len()
    {
        return Err(AgentError::InvalidPath(format!(
            "position {index} is past the {} chapters",
            session.book.chapters().len()
        )));
    }
    let whole = 0..args.text.len();
    store(
        session,
        &Place::New(path, args.position),
        &args.text,
        std::slice::from_ref(&whole),
        args.allow_unrendered,
    )
}

fn edit_chapter(session: &mut Session, args: &EditChapter) -> Result<Answer, AgentError> {
    let old = session
        .book
        .chapter(&args.chapter)
        .ok_or_else(|| AgentError::NoSuchPath(args.chapter.clone()))?
        .text()
        .to_owned();
    let (text, written) = apply(&old, &args.edits)?;
    store(
        session,
        &Place::Existing(args.chapter.clone()),
        &text,
        &written,
        args.allow_unrendered,
    )
}

fn write_chapter(session: &mut Session, args: WriteChapter) -> Result<Answer, AgentError> {
    let old = session
        .book
        .chapter(&args.chapter)
        .ok_or_else(|| AgentError::NoSuchPath(args.chapter.clone()))?
        .text()
        .to_owned();
    let revision = session.revisions.of(&args.chapter);
    if args.revision != revision {
        return Err(AgentError::Conflict {
            given: args.revision,
            revision,
        });
    }
    let written = changed(&old, &args.text);
    store(
        session,
        &Place::Existing(args.chapter),
        &args.text,
        std::slice::from_ref(&written),
        args.allow_unrendered,
    )
}

fn move_chapter(session: &mut Session, args: MoveChapter) -> Result<Answer, AgentError> {
    let book = &mut session.book;
    if book.chapter(&args.chapter).is_none() {
        return Err(AgentError::NoSuchPath(args.chapter));
    }
    let mut path = args.chapter;
    if let Some(to) = args.new_path.filter(|to| *to != path) {
        actor::move_chapter_path(book, &path, to.clone())?;
        path = to;
    }
    if let Some(index) = args.position {
        actor::move_chapter(book, &path, index)?;
    }
    Ok(Answer::of(&PathView { path: path.clone() }).touching(path))
}

/// A name for a picture from the name of its file: lower case, with `-` for
/// what a path should not hold.
fn picture_name(name: &str) -> Option<String> {
    let stem = name.rsplit(['/', '\\']).next()?.split('.').next()?;
    let mut out = String::new();
    for character in stem.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-') {
            out.push(character);
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-');
    (!out.is_empty()).then(|| format!("img/{out}.avif"))
}

fn add_picture(
    book: &mut Book,
    path: Option<String>,
    name: Option<&str>,
    avif: Vec<u8>,
    cover: bool,
) -> Result<PictureAdded, AgentError> {
    let path = path
        .or_else(|| {
            name.and_then(picture_name)
                .filter(|path| !taken(book, path))
        })
        .unwrap_or_else(|| {
            (1..=u32::MAX)
                .map(|number| format!("img/{number:04}.avif"))
                .find(|path| !taken(book, path))
                .unwrap_or_default()
        });
    let bytes = avif.len();
    let picture = if cover {
        book.set_cover(path, avif)
    } else {
        book.add_picture(path, avif)
    }
    .map_err(|error| AgentError::of_book(&error, None))?;
    Ok(PictureAdded {
        path: picture.path().to_owned(),
        width: picture.size().width,
        height: picture.size().height,
        bytes,
    })
}

fn put_figure(book: &mut Book, args: PutFigure) -> Result<Answer, AgentError> {
    let drawn = draw(args.kind, &args.source);
    let render = match &drawn {
        Ok(figure) => Render {
            ok: true,
            width: Some(figure.width() * TEXT / figure.em()),
            height: Some(figure.height() * TEXT / figure.em()),
            error: None,
        },
        Err(error) => Render {
            ok: false,
            width: None,
            height: None,
            error: Some(error.clone()),
        },
    };
    if let Err(message) = &drawn
        && !args.allow_unrendered
    {
        return Err(AgentError::InvalidFigure {
            kind: args.kind,
            line: None,
            message: message.clone(),
        });
    }
    let (folder, extension) = match args.kind {
        Kind::Diagram => ("dia", "mmd"),
        Kind::Formula => ("math", "tex"),
    };
    let path = args.path.unwrap_or_else(|| {
        (1..=u32::MAX)
            .map(|number| format!("{folder}/{number:04}.{extension}"))
            .find(|path| !taken(book, path))
            .unwrap_or_default()
    });
    let result = match args.kind {
        Kind::Diagram if book.diagram(&path).is_some() => book
            .diagram_mut(&path)
            .map_or(Ok(()), |diagram| diagram.set_source(args.source)),
        Kind::Formula if book.formula(&path).is_some() => book
            .formula_mut(&path)
            .map_or(Ok(()), |formula| formula.set_source(args.source)),
        _ if taken(book, &path) => Err(mbook::Error::DuplicatePath(path.clone())),
        Kind::Diagram => book.add_diagram(path.clone(), args.source).map(drop),
        Kind::Formula => book.add_formula(path.clone(), args.source).map(drop),
    };
    result.map_err(|error| AgentError::of_book(&error, None))?;
    let mut answer = Answer::of(&FigurePut { path, render });
    if args.preview
        && let Ok(figure) = &drawn
    {
        answer.png = figure
            .png(2.0 * TEXT / figure.em(), Some(Palette::default().page))
            .ok();
    }
    Ok(answer)
}

/// Draws a source without a book, for `render_figure`: the size at body
/// text and a PNG at twice that size.
pub(crate) fn render(kind: Kind, source: &str, display: bool) -> Result<Answer, AgentError> {
    let setting = if display {
        Setting::Block
    } else {
        Setting::Line
    };
    let drawn = match kind {
        Kind::Diagram => figure::diagram(source, Palette::default()),
        Kind::Formula => figure::formula(source, setting, Palette::default()),
    }
    .map_err(|error| AgentError::InvalidFigure {
        kind,
        line: None,
        message: error.to_string(),
    })?;
    let mut answer = Answer::of(&Render {
        ok: true,
        width: Some(drawn.width() * TEXT / drawn.em()),
        height: Some(drawn.height() * TEXT / drawn.em()),
        error: None,
    });
    answer.png = Some(
        drawn
            .png(2.0 * TEXT / drawn.em(), Some(Palette::default().page))
            .map_err(|error| AgentError::InvalidFigure {
                kind,
                line: None,
                message: error.to_string(),
            })?,
    );
    Ok(answer)
}

/// Applies a patch of the metadata, all or nothing.
fn set_meta(meta: &mut Meta, patch: MetaPatch) -> Result<(), AgentError> {
    let mut changed = meta.clone();
    let field = |name: &'static str| {
        move |error: mbook::Error| AgentError::InvalidMeta {
            field: name.into(),
            message: chain(&error),
        }
    };
    if let Some(title) = patch.title {
        changed.set_title(title).map_err(field("title"))?;
    }
    if let Some(language) = patch.language {
        changed.set_language(language).map_err(field("language"))?;
    }
    for (name, key, values) in [
        ("authors", "author", patch.authors),
        ("translators", "translator", patch.translators),
        ("isbn", "isbn", patch.isbn),
        ("genres", "genre", patch.genres),
    ] {
        if let Some(values) = values {
            changed.remove(key).map_err(field(name))?;
            for value in values {
                changed.add(key, value).map_err(field(name))?;
            }
        }
    }
    match patch.series {
        Patch::Set(series) => changed
            .set_series(series.name, series.index.as_deref())
            .map_err(field("series"))?,
        Patch::Remove => {
            changed.remove("series").map_err(field("series"))?;
            changed.remove("series-index").map_err(field("series"))?;
        }
        Patch::Keep => {}
    }
    for (name, key, value) in [
        ("publisher", "publisher", patch.publisher),
        ("date", "date", patch.date),
        ("annotation", "annotation", patch.annotation),
    ] {
        match value {
            Patch::Set(value) => changed.set(key, value).map_err(field(name))?,
            Patch::Remove => changed.remove(key).map_err(field(name))?,
            Patch::Keep => {}
        }
    }
    *meta = changed;
    Ok(())
}

/// Sets automatic contents, or the list to keep.
fn set_contents(book: &mut Book, args: SetContents) -> Result<(), AgentError> {
    let entries = match (args.automatic, args.entries) {
        (true, Some(_)) => {
            return Err(AgentError::InvalidArguments(
                "give `automatic` or `entries`, not both".into(),
            ));
        }
        (true, None) => Vec::new(),
        (false, Some(entries)) => entries,
        (false, None) => {
            return Err(AgentError::InvalidArguments(
                "give `automatic: true` or `entries`".into(),
            ));
        }
    };
    if entries.is_empty() {
        return actor::contents_from_headings(book)
            .map_err(|error| AgentError::of_book(&error, None));
    }
    if entries
        .iter()
        .any(|entry| entry.level == 0 || entry.title.is_empty())
    {
        return Err(AgentError::InvalidArguments(
            "every entry needs a level from 1 and a title".into(),
        ));
    }
    let items: Vec<TocItem> = entries
        .iter()
        .map(|entry| TocItem::new(entry.level, &entry.title, &entry.href))
        .collect();
    book.meta_mut()
        .remove(actor::CONTENTS_MODE)
        .map_err(|error| AgentError::of_book(&error, None))?;
    book.set_toc(Toc::Explicit(items));
    Ok(())
}

#[cfg(test)]
mod tests;
