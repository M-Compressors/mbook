//! The figures one text shows: their sources, and what was drawn of them.

use std::collections::HashMap;
use std::sync::Arc;

use super::{Figure, FigureError, FigureKind, Palette, Setting, render};

/// The source of one figure a text shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FigureSource {
    /// Whether it is a diagram or a formula.
    pub(crate) kind: FigureKind,
    /// Its source, as the book stores it. Empty for one that could not be
    /// read: it is then shown by its alternative text.
    pub(crate) source: Arc<str>,
}

/// The destination and optional caption of a reference-style image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageReference {
    /// The URL in the Markdown definition.
    pub(crate) destination: String,
    /// The definition's optional title.
    pub(crate) title: Option<String>,
}

/// The sources of the figures of one text.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FigureSources {
    /// The figures, by the picture destination that shows them, as the
    /// Markdown parser reports it.
    pub(crate) by_destination: HashMap<String, FigureSource>,
    /// The destination of a figure shown through a link reference
    /// definition, `![alt][name]`, by the identifier of the definition.
    pub(crate) by_reference: HashMap<String, ImageReference>,
}

impl FigureSources {
    /// Whether the text shows no figure.
    pub(crate) fn is_empty(&self) -> bool {
        self.by_destination.is_empty()
    }
}

/// One figure of a text, drawn both ways it can stand in it.
#[derive(Clone, Debug)]
pub(crate) struct DrawnFigure {
    /// Alone in a paragraph.
    pub(crate) block: Result<Arc<Figure>, FigureError>,
    /// Inside a line. A diagram is the same drawing both ways.
    pub(crate) line: Result<Arc<Figure>, FigureError>,
}

impl DrawnFigure {
    /// The drawing for a place in the text.
    pub(crate) fn set(&self, setting: Setting) -> Result<&Arc<Figure>, &FigureError> {
        match setting {
            Setting::Block => self.block.as_ref(),
            Setting::Line => self.line.as_ref(),
        }
    }
}

/// The figures of one text, drawn in the colours of one page.
#[derive(Clone, Debug, Default)]
pub(crate) struct Figures {
    drawn: HashMap<String, DrawnFigure>,
    by_reference: HashMap<String, ImageReference>,
}

impl Figures {
    /// Draws every figure of a text. Meant for a background task: a diagram
    /// takes milliseconds to tens of them.
    pub(crate) fn draw(sources: &FigureSources, palette: Palette) -> Self {
        let drawn = sources
            .by_destination
            .iter()
            .map(|(destination, figure)| {
                let draw = |setting| render(figure.kind, &figure.source, setting, palette);
                let drawn = DrawnFigure {
                    block: draw(Setting::Block),
                    line: draw(Setting::Line),
                };
                (destination.clone(), drawn)
            })
            .collect();
        Self {
            drawn,
            by_reference: sources.by_reference.clone(),
        }
    }

    /// Whether there is no figure.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.drawn.is_empty()
    }

    /// The figure a picture with this destination shows.
    pub(crate) fn shown_by(&self, destination: &str) -> Option<&DrawnFigure> {
        self.drawn.get(destination)
    }

    /// The definition of a reference-style image when it names a figure.
    pub(crate) fn reference(&self, identifier: &str) -> Option<&ImageReference> {
        let reference = self.by_reference.get(identifier)?;
        self.drawn
            .contains_key(&reference.destination)
            .then_some(reference)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> FigureSources {
        let figure = |kind, source: &str| FigureSource {
            kind,
            source: source.into(),
        };
        FigureSources {
            by_destination: HashMap::from([
                (
                    "dia/a.mmd".to_owned(),
                    figure(FigureKind::Diagram, "flowchart LR\n  A --> B"),
                ),
                (
                    "math/sum.tex".to_owned(),
                    figure(FigureKind::Formula, r"\sum_i i"),
                ),
                (
                    "math/bad.tex".to_owned(),
                    figure(FigureKind::Formula, r"\frac{"),
                ),
                ("dia/lost.mmd".to_owned(), figure(FigureKind::Diagram, "")),
            ]),
            by_reference: HashMap::from([
                (
                    "sum".to_owned(),
                    ImageReference {
                        destination: "math/sum.tex".to_owned(),
                        title: Some("Sum".to_owned()),
                    },
                ),
                (
                    "gone".to_owned(),
                    ImageReference {
                        destination: "math/gone.tex".to_owned(),
                        title: None,
                    },
                ),
            ]),
        }
    }

    #[test]
    fn a_text_without_figures_has_none_to_draw() {
        assert!(FigureSources::default().is_empty());
        assert!(!sources().is_empty());
        assert!(Figures::draw(&FigureSources::default(), Palette::PAPER).is_empty());
    }

    #[test]
    fn every_figure_is_drawn_for_a_line_and_for_a_block() {
        let figures = Figures::draw(&sources(), Palette::PAPER);
        assert!(!figures.is_empty());
        let formula = figures.shown_by("math/sum.tex").unwrap();
        let (block, line) = (
            formula.set(Setting::Block).unwrap(),
            formula.set(Setting::Line).unwrap(),
        );
        assert!(block.height > line.height);
        let diagram = figures.shown_by("dia/a.mmd").unwrap();
        assert!(Arc::ptr_eq(
            diagram.set(Setting::Block).unwrap(),
            diagram.set(Setting::Line).unwrap()
        ));
    }

    #[test]
    fn a_figure_that_cannot_be_drawn_keeps_the_reason() {
        let figures = Figures::draw(&sources(), Palette::PAPER);
        let bad = figures.shown_by("math/bad.tex").unwrap();
        assert!(matches!(
            bad.set(Setting::Line),
            Err(FigureError::Formula(_))
        ));
        let lost = figures.shown_by("dia/lost.mmd").unwrap();
        assert_eq!(lost.set(Setting::Block).err(), Some(&FigureError::Empty));
    }

    #[test]
    fn a_destination_that_shows_no_figure_is_unknown() {
        let figures = Figures::draw(&sources(), Palette::PAPER);
        assert!(figures.shown_by("img/a.avif").is_none());
    }

    #[test]
    fn a_reference_leads_to_its_figure_when_there_is_one() {
        let figures = Figures::draw(&sources(), Palette::PAPER);
        let reference = figures.reference("sum").unwrap();
        assert_eq!(reference.destination, "math/sum.tex");
        assert_eq!(reference.title.as_deref(), Some("Sum"));
        assert_eq!(figures.reference("gone"), None);
        assert_eq!(figures.reference("other"), None);
    }
}
