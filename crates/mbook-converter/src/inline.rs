//! Diagrams and formulas written into the Markdown of a chapter.
//!
//! An author, or an agent, writes a diagram as a `mermaid` code fence and a
//! formula as `$…$` or `$$…$$`. A book stores both as resources of their own
//! and shows them with the syntax of a picture (RFC 0001, "Diagram and
//! formula resources"). [`extract`] finds them in a text and says what the
//! text becomes and which resources it needs; the caller adds the resources
//! to the book and stores the text.
//!
//! ````
//! use mbook::Book;
//! use mbook_converter::inline::{self, Kind};
//!
//! let book = Book::new("The Example", "en")?;
//! let text = "# One\n\n```mermaid\n%% alt: A to B\nflowchart LR\n  A --> B\n```\n";
//! let found = inline::extract(text, "ch01.md", &book, &[0..text.len()]);
//! assert_eq!(found.text, "# One\n\n![A to B](dia/0001.mmd)\n");
//! assert_eq!(found.figures[0].kind, Kind::Diagram);
//! assert_eq!(found.figures[0].source, "flowchart LR\n  A --> B\n");
//! # Ok::<(), mbook::Error>(())
//! ````
//!
//! The rules, which `mbook-ui` applies to the text an agent writes:
//!
//! - A fenced code block whose info string is `mermaid` is a diagram. Its
//!   leading lines `%% alt: …` and `%% caption: …` give the alternative text
//!   and the caption and are removed from its source; other lines are kept.
//! - `$$…$$` is a display formula, and `$…$` a formula in a line, as the
//!   parser of `markdown` finds them with its math constructs on. Code spans,
//!   other code blocks and `\$` are not formulas.
//! - A diagram and a display formula become a paragraph of their own,
//!   `![alt](dia/0001.mmd "caption")`; a formula in a line becomes a
//!   reference in its place. The alternative text of a formula is its source.
//! - Only what lies wholly inside one of the ranges `new` is moved. A `$` is
//!   plain text in mbook, so the text a book already had is left alone.
//! - A new resource gets the first free number under `dia/` or `math/`; a
//!   formula whose source the book already has, or that this text has
//!   twice, names one resource.

use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

use markdown::mdast::Node;
use markdown::{Constructs, ParseOptions, to_mdast};
use mbook::Book;

/// What a figure is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A diagram in Mermaid notation.
    Diagram,
    /// A formula in the notation of `KaTeX`.
    Formula,
}

/// A figure that was found in the text.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Found {
    /// A diagram or a formula.
    pub kind: Kind,
    /// The path of its resource in the book.
    pub path: String,
    /// Its source, as the resource holds it.
    pub source: String,
    /// The line of the text it was written on, from 1.
    pub line: usize,
    /// Whether the resource is new; `false` for a formula the book, or an
    /// earlier place of the text, already has.
    pub new: bool,
    /// Whether an alternative text in words was given: `%% alt:` for a
    /// diagram. A formula never has one; its source stands in.
    pub described: bool,
}

/// A text with its diagrams and formulas moved out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Extracted {
    /// The text, with references in place of the figures.
    pub text: String,
    /// The figures, in the order of the text.
    pub figures: Vec<Found>,
}

/// One figure and the text that takes its place.
struct Replacement {
    span: Range<usize>,
    with: String,
}

/// Finds the diagrams and formulas of `text` that lie inside `new`.
///
/// `chapter` is the path of the chapter the text is for: the references are
/// relative to it. `book` gives the paths that are taken and the formulas
/// that exist. Nothing is changed: the caller adds the resources that are
/// [`new`](Found::new) and stores [`text`](Extracted::text).
#[must_use]
pub fn extract(text: &str, chapter: &str, book: &Book, new: &[Range<usize>]) -> Extracted {
    let options = ParseOptions {
        constructs: Constructs {
            math_flow: true,
            math_text: true,
            ..Constructs::gfm()
        },
        ..ParseOptions::gfm()
    };
    // Plain Markdown has no syntax error; only MDX, which is off, has.
    let Ok(root) = to_mdast(text, &options) else {
        return Extracted {
            text: text.to_owned(),
            figures: Vec::new(),
        };
    };
    let mut names = Names::of(book);
    let up = "../".repeat(chapter.matches('/').count());
    let mut figures = Vec::new();
    let mut replacements = Vec::new();
    visit(&root, &mut |node| {
        let Some(found) = Figure::of(node) else {
            return;
        };
        let Some(position) = node.position() else {
            return;
        };
        let span = position.start.offset..position.end.offset;
        if !new
            .iter()
            .any(|range| range.start <= span.start && span.end <= range.end)
        {
            return;
        }
        let (path, fresh) = names.name(found.kind, &found.source);
        let reference = reference(&found, &format!("{up}{path}"));
        let with = if found.block {
            alone(text, &span, &reference)
        } else {
            reference
        };
        figures.push(Found {
            kind: found.kind,
            path,
            source: found.source,
            line: position.start.line,
            new: fresh,
            described: found.alt.is_some(),
        });
        replacements.push(Replacement { span, with });
    });
    let mut out = text.to_owned();
    for replacement in replacements.iter().rev() {
        out.replace_range(replacement.span.clone(), &replacement.with);
    }
    Extracted { text: out, figures }
}

/// Visits every node, parents before children.
fn visit(node: &Node, f: &mut impl FnMut(&Node)) {
    f(node);
    if let Some(children) = node.children() {
        for child in children {
            visit(child, f);
        }
    }
}

/// What a node says, when it is a figure.
struct Figure {
    kind: Kind,
    source: String,
    alt: Option<String>,
    caption: Option<String>,
    /// A diagram or a display formula, which stands as a paragraph.
    block: bool,
}

impl Figure {
    fn of(node: &Node) -> Option<Self> {
        match node {
            Node::Code(code) if code.lang.as_deref() == Some("mermaid") => {
                let mut alt = None;
                let mut caption = None;
                let mut lines = code.value.lines().peekable();
                while let Some(line) = lines.peek() {
                    let comment = line.trim_start();
                    if let Some(rest) = comment.strip_prefix("%%") {
                        let rest = rest.trim_start();
                        if let Some(value) = rest.strip_prefix("alt:") {
                            alt = Some(value.trim().to_owned());
                        } else if let Some(value) = rest.strip_prefix("caption:") {
                            caption = Some(value.trim().to_owned());
                        } else {
                            break;
                        }
                        lines.next();
                    } else if comment.is_empty() {
                        lines.next();
                    } else {
                        break;
                    }
                }
                let mut source: String = lines.flat_map(|line| [line, "\n"]).collect();
                if source.trim().is_empty() {
                    return None;
                }
                if !source.ends_with('\n') {
                    source.push('\n');
                }
                Some(Self {
                    kind: Kind::Diagram,
                    source,
                    alt: alt.filter(|alt| !alt.is_empty()),
                    caption: caption.filter(|caption| !caption.is_empty()),
                    block: true,
                })
            }
            Node::Math(math) => Self::formula(&math.value, true),
            Node::InlineMath(math) => Self::formula(&math.value, false),
            _ => None,
        }
    }

    fn formula(value: &str, block: bool) -> Option<Self> {
        let source = value.trim();
        (!source.is_empty()).then(|| Self {
            kind: Kind::Formula,
            source: format!("{source}\n"),
            alt: None,
            caption: None,
            block,
        })
    }
}

/// The reference that shows a figure: `![alt](destination "caption")`.
fn reference(figure: &Figure, destination: &str) -> String {
    let alt = figure.alt.clone().unwrap_or_else(|| {
        figure
            .source
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    });
    let mut out = format!("![{}]({destination}", crate::markdown::escape(&alt));
    if let Some(caption) = &figure.caption {
        out.push_str(" \"");
        for character in caption.chars() {
            if matches!(character, '"' | '\\') {
                out.push('\\');
            }
            out.push(character);
        }
        out.push('"');
    }
    out.push(')');
    out
}

/// A reference that stands for a block, with the empty lines that keep it a
/// paragraph of its own.
///
/// The block starts after the marks of its containers on its first line,
/// which stay in front of it. An empty line is put before it when the line
/// above has text and the containers are only quotes and indentation, and
/// after it when the line below has text.
fn alone(text: &str, span: &Range<usize>, reference: &str) -> String {
    let line_start = text[..span.start].rfind('\n').map_or(0, |at| at + 1);
    let prefix = &text[line_start..span.start];
    let quotes: String = prefix.chars().filter(|&c| c == '>').collect();
    let only_quotes = prefix.chars().all(|c| c == '>' || c == ' ' || c == '\t');
    let has_text = |line: &str| line.chars().any(|c| !matches!(c, '>' | ' ' | '\t'));
    let above = line_start
        .checked_sub(1)
        .map(|end| &text[text[..end].rfind('\n').map_or(0, |at| at + 1)..end]);
    let below = text[span.end..]
        .strip_prefix('\n')
        .map(|rest| rest.split('\n').next().unwrap_or_default());
    let mut out = String::new();
    if only_quotes && above.is_some_and(has_text) {
        out.push('\n');
        out.push_str(prefix);
    }
    out.push_str(reference);
    if below.is_some_and(has_text) {
        out.push('\n');
        out.push_str(&quotes);
    }
    out
}

/// The paths a text may give its figures.
struct Names {
    /// Every path of the book, and those given since.
    taken: BTreeSet<String>,
    /// Formulas by source.
    formulas: HashMap<String, String>,
    next: HashMap<Kind, u32>,
}

impl Names {
    fn of(book: &Book) -> Self {
        let mut taken = BTreeSet::new();
        taken.extend(book.chapters().iter().map(|c| c.path().to_owned()));
        taken.extend(book.pictures().iter().map(|p| p.path().to_owned()));
        taken.extend(book.diagrams().iter().map(|d| d.path().to_owned()));
        taken.extend(book.formulas().iter().map(|f| f.path().to_owned()));
        let formulas = book
            .formulas()
            .iter()
            .map(|f| (f.source().to_owned(), f.path().to_owned()))
            .collect();
        Self {
            taken,
            formulas,
            next: HashMap::new(),
        }
    }

    /// The path for a figure, and whether it is new.
    fn name(&mut self, kind: Kind, source: &str) -> (String, bool) {
        if kind == Kind::Formula
            && let Some(path) = self.formulas.get(source)
        {
            return (path.clone(), false);
        }
        let (folder, extension) = match kind {
            Kind::Diagram => ("dia", "mmd"),
            Kind::Formula => ("math", "tex"),
        };
        let next = self.next.entry(kind).or_insert(1);
        let path = loop {
            let path = format!("{folder}/{:04}.{extension}", *next);
            *next += 1;
            if !self.taken.contains(&path) {
                break path;
            }
        };
        self.taken.insert(path.clone());
        if kind == Kind::Formula {
            self.formulas.insert(source.to_owned(), path.clone());
        }
        (path, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The range from `start` to the end of `text`.
    fn whole(start: usize, text: &str) -> Range<usize> {
        start..text.len()
    }

    fn book() -> Book {
        Book::new("The Example", "en").unwrap()
    }

    fn all(text: &str) -> Extracted {
        extract(text, "ch01.md", &book(), &[whole(0, text)])
    }

    #[test]
    fn a_diagram_becomes_a_figure_with_its_alt_and_caption() {
        let found = all(
            "```mermaid\n%% alt: From A to B\n%% caption: Fig. 1. \"Way\"\nflowchart LR\n  A --> B\n```\n",
        );
        assert_eq!(
            found.text,
            "![From A to B](dia/0001.mmd \"Fig. 1. \\\"Way\\\"\")\n"
        );
        let figure = &found.figures[0];
        assert_eq!(figure.source, "flowchart LR\n  A --> B\n");
        assert_eq!((figure.line, figure.new, figure.described), (1, true, true));
    }

    #[test]
    fn a_diagram_without_alt_is_described_by_its_source() {
        let found = all("```mermaid\npie\n  \"a\" : 1\n```\n");
        assert_eq!(found.text, "![pie \"a\" : 1](dia/0001.mmd)\n");
        assert!(!found.figures[0].described);
    }

    #[test]
    fn other_comments_of_a_diagram_are_kept() {
        let found = all("```mermaid\n%% alt: X\n%% layout\nflowchart TD\n```\n");
        assert_eq!(found.figures[0].source, "%% layout\nflowchart TD\n");
    }

    #[test]
    fn an_empty_diagram_and_other_code_are_left_alone() {
        let text = "```mermaid\n%% alt: nothing\n```\n\n```rust\nfn main() {}\n```\n";
        let found = all(text);
        assert_eq!(found.text, text);
        assert_eq!(found.figures, []);
    }

    #[test]
    fn a_formula_in_a_line_stays_in_its_line() {
        let found = all("Speed $v = s / t$ here.\n");
        assert_eq!(found.text, "Speed ![v = s / t](math/0001.tex) here.\n");
        assert_eq!(found.figures[0].source, "v = s / t\n");
        assert!(!found.figures[0].described);
    }

    #[test]
    fn a_display_formula_becomes_a_paragraph() {
        let found = all("Before\n$$\nE = mc^2\n$$\nAfter\n");
        assert_eq!(
            found.text,
            "Before\n\n![E = mc^2](math/0001.tex)\n\nAfter\n"
        );
    }

    #[test]
    fn a_figure_between_lines_of_text_gets_empty_lines() {
        let found = all("Text\n```mermaid\n%% alt: X\nflowchart TD\n```\nMore\n");
        assert_eq!(found.text, "Text\n\n![X](dia/0001.mmd)\n\nMore\n");
    }

    #[test]
    fn a_figure_in_a_quote_keeps_its_quote() {
        let found = all("> Text\n> $$\n> x\n> $$\n> More\n");
        assert_eq!(found.text, "> Text\n> \n> ![x](math/0001.tex)\n>\n> More\n");
    }

    #[test]
    fn a_figure_that_opens_a_list_item_gets_no_line_before() {
        let found = all("- a\n- $$\n  x\n  $$\n");
        assert_eq!(found.text, "- a\n- ![x](math/0001.tex)\n");
    }

    #[test]
    fn a_formula_given_twice_is_one_resource() {
        let found = all("$x$ and $x$ and $y$\n");
        let paths: Vec<_> = found
            .figures
            .iter()
            .map(|f| (f.path.as_str(), f.new))
            .collect();
        assert_eq!(
            paths,
            [
                ("math/0001.tex", true),
                ("math/0001.tex", false),
                ("math/0002.tex", true)
            ]
        );
    }

    #[test]
    fn a_formula_the_book_has_is_not_stored_again() {
        let mut book = book();
        book.add_formula("math/speed.tex", "v\n").unwrap();
        book.add_diagram("dia/0001.mmd", "flowchart TD\n").unwrap();
        let text = "$v$ and\n```mermaid\nflowchart LR\n```\n";
        let found = extract(text, "ch01.md", &book, &[whole(0, text)]);
        assert_eq!(found.figures[0].path, "math/speed.tex");
        assert!(!found.figures[0].new);
        assert_eq!(found.figures[1].path, "dia/0002.mmd");
    }

    #[test]
    fn only_what_lies_inside_the_new_ranges_is_moved() {
        let text = "Old $5 and $10.\n\nNew $x$.\n";
        let start = text.find("New").unwrap();
        let found = extract(text, "ch01.md", &book(), &[whole(start, text)]);
        assert_eq!(found.text, "Old $5 and $10.\n\nNew ![x](math/0001.tex).\n");
        assert_eq!(found.figures[0].line, 3);
    }

    #[test]
    fn a_reference_is_relative_to_the_chapter() {
        let text = "$x$\n";
        let found = extract(text, "part/one/ch01.md", &book(), &[whole(0, text)]);
        assert_eq!(found.text, "![x](../../math/0001.tex)\n");
        assert_eq!(found.figures[0].path, "math/0001.tex");
    }

    #[test]
    fn code_and_escaped_dollars_are_not_formulas() {
        let text = "`$x$` and \\$5 and \\$6\n";
        assert_eq!(all(text).text, text);
    }

    #[test]
    fn markup_in_an_alternative_text_is_escaped() {
        let found = all("$[a]_b$\n");
        assert_eq!(found.text, "![\\[a\\]\\_b](math/0001.tex)\n");
    }

    #[test]
    fn a_book_given_what_was_found_passes_its_check() {
        let mut book = book();
        let text = "# One\n\nText $x^2$ and\n```mermaid\n%% alt: A to B\nflowchart LR\n  A --> B\n```\nend.\n";
        let found = extract(text, "part/ch01.md", &book, &[whole(0, text)]);
        for figure in found.figures.iter().filter(|f| f.new) {
            match figure.kind {
                Kind::Diagram => drop(book.add_diagram(&figure.path, &figure.source).unwrap()),
                Kind::Formula => drop(book.add_formula(&figure.path, &figure.source).unwrap()),
            }
        }
        book.add_chapter("part/ch01.md", found.text).unwrap();
        assert_eq!(book.check(), Ok(()));
    }

    #[test]
    fn a_text_without_figures_is_unchanged() {
        let found = all("# Title\n\nText.\n");
        assert_eq!(
            found,
            Extracted {
                text: "# Title\n\nText.\n".into(),
                figures: Vec::new()
            }
        );
    }
}
