//! What the tools of an agent are given and answer, with their schemas.
//!
//! The names and shapes follow the reference of the tools (mbook-ui docs,
//! "Agents (MCP)", "Tools"); lines and columns count from 1.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

use crate::entities::figure::FigureKind;

/// A field of a patch: left out, `null`, or a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Patch<T> {
    /// The field was left out: what the book has stays.
    #[default]
    Keep,
    /// The field is `null`: the book loses it.
    Remove,
    /// The field has a value.
    Set(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Option::<T>::deserialize(deserializer).map(|value| value.map_or(Self::Remove, Self::Set))
    }
}

impl<T: JsonSchema> JsonSchema for Patch<T> {
    fn inline_schema() -> bool {
        true
    }

    fn schema_name() -> std::borrow::Cow<'static, str> {
        Option::<T>::schema_name()
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        Option::<T>::json_schema(generator)
    }
}

/// A diagram or a formula.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    /// Mermaid notation.
    Diagram,
    /// The notation of `KaTeX`.
    Formula,
}

impl From<Kind> for FigureKind {
    fn from(kind: Kind) -> Self {
        match kind {
            Kind::Diagram => Self::Diagram,
            Kind::Formula => Self::Formula,
        }
    }
}

impl From<FigureKind> for Kind {
    fn from(kind: FigureKind) -> Self {
        match kind {
            FigureKind::Diagram => Self::Diagram,
            FigureKind::Formula => Self::Formula,
        }
    }
}

/// Lines of a chapter, from `from` to `to`, both included.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct Lines {
    /// The first line, from 1.
    pub(crate) from: usize,
    /// The last line.
    pub(crate) to: usize,
}

/// What `read_chapter` reads.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct ReadChapter {
    /// The path of the chapter.
    pub(crate) chapter: String,
    /// Read only the section that starts at this anchor: a heading to the
    /// next heading of the same level or higher, or a named line to the next
    /// heading.
    #[serde(default)]
    pub(crate) anchor: Option<String>,
    /// Read only these lines.
    #[serde(default)]
    pub(crate) lines: Option<Lines>,
}

/// What `search_book` looks for.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct Search {
    /// The text, or a regular expression with `regex`.
    pub(crate) query: String,
    /// Read `query` as a regular expression of the `regex` crate.
    #[serde(default)]
    pub(crate) regex: bool,
    /// At most this many matches; 50 when left out.
    #[serde(default)]
    pub(crate) limit: Option<usize>,
}

/// A path in the book.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct PathArgs {
    /// The path of the picture, diagram or formula.
    pub(crate) path: String,
}

/// A chapter of the book.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct ChapterArgs {
    /// The path of the chapter.
    pub(crate) chapter: String,
}

/// What `add_chapter` adds.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct AddChapter {
    /// The Markdown text. Mermaid fences and `$…$` become resources.
    pub(crate) text: String,
    /// The path; the first free `chNN.md` when left out.
    #[serde(default)]
    pub(crate) path: Option<String>,
    /// The position in the reading order, from 0; after the last chapter when
    /// left out.
    #[serde(default)]
    pub(crate) position: Option<usize>,
    /// Store diagrams and formulas that cannot be drawn.
    #[serde(default)]
    pub(crate) allow_unrendered: bool,
}

/// One replacement of `edit_chapter`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct Edit {
    /// Text of the chapter as it is stored, exactly once unless
    /// `replace_all`.
    pub(crate) old: String,
    /// What takes its place. Mermaid fences and `$…$` become resources.
    pub(crate) new: String,
    /// Replace every occurrence.
    #[serde(default)]
    pub(crate) replace_all: bool,
}

/// What `edit_chapter` changes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct EditChapter {
    /// The path of the chapter.
    pub(crate) chapter: String,
    /// The replacements, applied in order; all or none.
    pub(crate) edits: Vec<Edit>,
    /// Store diagrams and formulas that cannot be drawn.
    #[serde(default)]
    pub(crate) allow_unrendered: bool,
}

/// What `write_chapter` writes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct WriteChapter {
    /// The path of the chapter.
    pub(crate) chapter: String,
    /// The whole new text.
    pub(crate) text: String,
    /// The revision `read_chapter` gave.
    pub(crate) revision: String,
    /// Store diagrams and formulas that cannot be drawn.
    #[serde(default)]
    pub(crate) allow_unrendered: bool,
}

/// What `move_chapter` changes.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct MoveChapter {
    /// The path of the chapter.
    pub(crate) chapter: String,
    /// The new position in the reading order, from 0.
    #[serde(default)]
    pub(crate) position: Option<usize>,
    /// The new path.
    #[serde(default)]
    pub(crate) new_path: Option<String>,
}

/// What `put_figure` stores.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct PutFigure {
    /// A diagram or a formula.
    pub(crate) kind: Kind,
    /// The Mermaid source, or `KaTeX` without `$`.
    pub(crate) source: String,
    /// The figure to change; a new one gets the first free number when left
    /// out.
    #[serde(default)]
    pub(crate) path: Option<String>,
    /// Answer with a PNG image of the figure as well.
    #[serde(default)]
    pub(crate) preview: bool,
    /// Store it even when it cannot be drawn.
    #[serde(default)]
    pub(crate) allow_unrendered: bool,
}

/// A series and the place in it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Series {
    /// The name of the series.
    pub(crate) name: String,
    /// A decimal number such as `2` or `2.5`.
    #[serde(default)]
    pub(crate) index: Option<String>,
}

/// A patch of the metadata: a missing field stays, `null` or an empty list
/// removes it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct MetaPatch {
    /// The title; never empty.
    #[serde(default)]
    pub(crate) title: Option<String>,
    /// The authors, in order.
    #[serde(default)]
    pub(crate) authors: Option<Vec<String>>,
    /// The translators, in order.
    #[serde(default)]
    pub(crate) translators: Option<Vec<String>>,
    /// The main language, a BCP 47 tag.
    #[serde(default)]
    pub(crate) language: Option<String>,
    /// The series.
    #[serde(default)]
    pub(crate) series: Patch<Series>,
    /// The publisher.
    #[serde(default)]
    pub(crate) publisher: Patch<String>,
    /// `YYYY`, `YYYY-MM` or `YYYY-MM-DD`.
    #[serde(default)]
    pub(crate) date: Patch<String>,
    /// ISBN-13 or ISBN-10, digits only.
    #[serde(default)]
    pub(crate) isbn: Option<Vec<String>>,
    /// Free-form genres or tags.
    #[serde(default)]
    pub(crate) genres: Option<Vec<String>>,
    /// Plain text; paragraphs are separated by an empty line.
    #[serde(default)]
    pub(crate) annotation: Patch<String>,
}

/// One entry of the contents, or one landmark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct EntryView {
    /// The level, from 1.
    pub(crate) level: u8,
    /// What the entry says.
    pub(crate) title: String,
    /// Where it leads: `ch01.md` or `ch01.md#a-heading`.
    pub(crate) href: String,
    /// Why it leads nowhere, when it does not.
    #[serde(default, skip_deserializing, skip_serializing_if = "Option::is_none")]
    pub(crate) fault: Option<String>,
}

/// What `set_contents` sets.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct SetContents {
    /// Follow the headings of the chapters.
    #[serde(default)]
    pub(crate) automatic: bool,
    /// The list to keep, when not automatic.
    #[serde(default)]
    pub(crate) entries: Option<Vec<EntryView>>,
}

/// A landmark.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct LandmarkView {
    /// `start`, `glossary`, `index`, … or an `x-` kind.
    pub(crate) kind: String,
    /// May be empty.
    #[serde(default)]
    pub(crate) title: String,
    /// Where it leads.
    pub(crate) href: String,
}

/// The landmarks of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct LandmarksView {
    pub(crate) landmarks: Vec<LandmarkView>,
}

/// What `set_landmarks` sets.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, JsonSchema)]
pub(crate) struct SetLandmarks {
    /// The whole list; empty removes the landmarks.
    pub(crate) landmarks: Vec<LandmarkView>,
}

/// A line of a chapter that shows a picture, a diagram or a formula.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Use {
    /// The chapter.
    pub(crate) chapter: String,
    /// The line, from 1.
    pub(crate) line: usize,
}

/// A heading of a chapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct HeadingView {
    /// From 1 to 6.
    pub(crate) level: u8,
    /// The text, without markup.
    pub(crate) title: String,
    /// The anchor a link names it by; `None` for a heading without one.
    pub(crate) anchor: Option<String>,
}

/// A chapter in the structure of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ChapterView {
    /// The path.
    pub(crate) path: String,
    /// The number of lines.
    pub(crate) lines: usize,
    /// The size in bytes.
    pub(crate) bytes: usize,
    /// The first heading, when there is one.
    pub(crate) title: Option<String>,
    /// Every heading.
    pub(crate) headings: Vec<HeadingView>,
    /// The names of the lines its anchor definitions name.
    pub(crate) anchors: Vec<String>,
}

/// A picture in the structure of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct PictureView {
    /// The path.
    pub(crate) path: String,
    /// The width in pixels.
    pub(crate) width: u32,
    /// The height in pixels.
    pub(crate) height: u32,
    /// The size of the AVIF file.
    pub(crate) bytes: usize,
    /// The alternative text of its first use.
    pub(crate) alt: Option<String>,
    /// The lines that show it.
    pub(crate) used_by: Vec<Use>,
}

/// A diagram or formula in the structure of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct FigureView {
    /// A diagram or a formula.
    pub(crate) kind: Kind,
    /// The path.
    pub(crate) path: String,
    /// The lines that show it.
    pub(crate) used_by: Vec<Use>,
}

/// The metadata of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct MetaView {
    pub(crate) title: String,
    pub(crate) authors: Vec<String>,
    pub(crate) translators: Vec<String>,
    pub(crate) language: String,
    pub(crate) id: String,
    pub(crate) series: Option<Series>,
    pub(crate) publisher: Option<String>,
    pub(crate) date: Option<String>,
    pub(crate) isbn: Vec<String>,
    pub(crate) genres: Vec<String>,
    pub(crate) annotation: Option<String>,
    /// The path of the cover picture.
    pub(crate) cover: Option<String>,
}

/// The contents of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ContentsView {
    /// They follow the headings.
    pub(crate) automatic: bool,
    /// What a reader is shown.
    pub(crate) entries: Vec<EntryView>,
}

/// The structure of a book, without its text and pictures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct BookView {
    pub(crate) meta: MetaView,
    pub(crate) chapters: Vec<ChapterView>,
    pub(crate) contents: ContentsView,
    pub(crate) landmarks: Vec<LandmarkView>,
    pub(crate) pictures: Vec<PictureView>,
    pub(crate) figures: Vec<FigureView>,
    /// Changes were made since the book was opened or saved.
    pub(crate) unsaved: bool,
    /// The number of problems a check finds.
    pub(crate) problems: usize,
    /// A window of the application edits the book too.
    pub(crate) open_in_editor: bool,
}

/// A part of a chapter.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ChapterText {
    /// The text, exactly as the chapter holds it.
    pub(crate) text: String,
    /// What `write_chapter` needs.
    pub(crate) revision: String,
    /// The first line of `text`.
    pub(crate) first_line: usize,
    /// The last line of `text`.
    pub(crate) last_line: usize,
    /// The number of lines of the chapter.
    pub(crate) total_lines: usize,
}

/// A place where the query was found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Match {
    pub(crate) chapter: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
    /// The line, shortened around the match.
    pub(crate) snippet: String,
}

/// What `search_book` found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Matches {
    pub(crate) matches: Vec<Match>,
    /// More matches were left out.
    pub(crate) truncated: bool,
}

/// The source of a diagram or formula.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct FigureSource {
    pub(crate) kind: Kind,
    pub(crate) source: String,
    pub(crate) used_by: Vec<Use>,
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ProblemView {
    /// The chapter, diagram or formula it is in.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) chapter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) column: Option<usize>,
    pub(crate) message: String,
}

/// The problems of a book.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ProblemsView {
    pub(crate) problems: Vec<ProblemView>,
}

/// A diagram or formula moved out of the text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Made {
    pub(crate) kind: Kind,
    pub(crate) path: String,
    /// The line of the text it was written on.
    pub(crate) line: usize,
    /// A new resource; `false` for a formula the book had.
    pub(crate) new: bool,
}

/// Something the agent should know about a change that was made.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Warning {
    /// `missing_alt`.
    pub(crate) code: String,
    pub(crate) line: usize,
    pub(crate) message: String,
}

/// A chapter after a change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct ChapterChanged {
    pub(crate) path: String,
    pub(crate) revision: String,
    pub(crate) figures: Vec<Made>,
    pub(crate) warnings: Vec<Warning>,
    /// The problems of this chapter.
    pub(crate) problems: Vec<ProblemView>,
}

/// A path after a change.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct PathView {
    pub(crate) path: String,
}

/// A picture that was added.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct PictureAdded {
    pub(crate) path: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bytes: usize,
}

/// Whether a figure could be drawn.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub(crate) struct Render {
    pub(crate) ok: bool,
    /// The size of the drawing at the size of body text, in pixels.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) height: Option<f32>,
    /// What the renderer said.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) error: Option<String>,
}

/// A figure that was stored.
#[derive(Clone, Debug, PartialEq, Serialize, JsonSchema)]
pub(crate) struct FigurePut {
    pub(crate) path: String,
    pub(crate) render: Render,
}

/// A picture of the book, for `view_picture`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct PictureInfo {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bytes: usize,
    pub(crate) alt: Option<String>,
    pub(crate) used_by: Vec<Use>,
}

/// The answer to a save.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Saved {
    pub(crate) saved: bool,
    pub(crate) problems: Vec<ProblemView>,
}

/// An answer without content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, JsonSchema)]
pub(crate) struct Done {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_patch_tells_a_missing_field_from_null_and_from_a_value() {
        let patch: MetaPatch =
            serde_json::from_str(r#"{"publisher": null, "date": "2020", "authors": []}"#).unwrap();
        assert_eq!(patch.publisher, Patch::Remove);
        assert_eq!(patch.date, Patch::Set("2020".into()));
        assert_eq!(patch.series, Patch::Keep);
        assert_eq!(patch.authors, Some(Vec::new()));
    }

    #[test]
    fn a_field_of_a_patch_has_the_schema_of_a_value_that_may_be_null() {
        let schema = serde_json::to_value(schemars::schema_for!(MetaPatch)).unwrap();
        let publisher = &schema["properties"]["publisher"];
        assert_eq!(
            publisher["type"],
            serde_json::json!(["string", "null"]),
            "{schema}"
        );
    }

    #[test]
    fn a_kind_is_the_kind_of_figure_of_the_application() {
        for kind in [Kind::Diagram, Kind::Formula] {
            assert_eq!(Kind::from(FigureKind::from(kind)), kind);
        }
    }
}
