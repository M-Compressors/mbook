//! The diagrams and formulas Visual draws in the text.
//!
//! A chapter shows a figure with the syntax of a picture (RFC 0001,
//! "Diagram and formula resources"). The block editor asks the page for the
//! image of every picture it lays out; for a destination that names a figure
//! the page answers with the drawing of the figure, sized for the text.
//!
//! The figures are those of Preview: the book resolves them from the draft
//! after a typing pause, and Visual draws them on the background in the
//! colours of its page.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use gpui_kit::{AppContext as _, Context, ImageCacheError, ImageSource};
use markdown::mdast::Node;
use markdown::{ParseOptions, to_mdast};

use super::EditorPage;
use crate::entities::figure::{FigureSources, Figures, Palette, Setting};
use crate::shared::compute;
use crate::widgets::chapter_view::fitted;

/// The figures of the draft Visual shows.
#[derive(Default)]
pub(super) struct VisualFigures {
    /// What the draft shows, as the book resolved it.
    sources: Arc<FigureSources>,
    /// The destinations of figures that are alone in their paragraph; a
    /// formula among them is drawn as a display formula, as in the reader.
    alone: HashSet<String>,
    /// The last drawing and the colours it is in. It may be of earlier
    /// sources, which it shows until the drawing of the new ones arrives.
    drawn: Option<(Palette, Arc<Figures>)>,
    /// The colours `sources` were last asked to be drawn in.
    asked: Option<Palette>,
}

/// What Visual draws for one destination of the text.
pub(super) enum Shown {
    /// No figure: perhaps a picture.
    NoFigure,
    /// A figure, sized for the text.
    Figure(ImageSource),
}

impl VisualFigures {
    /// Keeps `drawn`, the drawing of `sources` in `palette`, unless the
    /// draft shows other figures by now. Whether it was kept.
    fn keep(&mut self, sources: &Arc<FigureSources>, palette: Palette, drawn: Figures) -> bool {
        let current = Arc::ptr_eq(&self.sources, sources);
        if current {
            self.drawn = Some((palette, Arc::new(drawn)));
        }
        current
    }

    /// What the destination of an image shows in text of `text_size`
    /// pixels, in a column of `column`.
    ///
    /// A figure not drawn yet is an image still loading, and one that cannot
    /// be drawn is an image that failed.
    pub(super) fn shown(&self, destination: &str, text_size: f32, column: f32) -> Shown {
        if !self.sources.by_destination.contains_key(destination) {
            return Shown::NoFigure;
        }
        let drawn = self
            .drawn
            .as_ref()
            .and_then(|(_, figures)| figures.shown_by(destination));
        let Some(drawn) = drawn else {
            return Shown::Figure(ImageSource::Custom(Arc::new(|_, _| None)));
        };
        let setting = if self.alone.contains(destination) {
            Setting::Block
        } else {
            Setting::Line
        };
        let image = drawn.set(setting).ok().and_then(|figure| {
            let (_, _, scale) = fitted(
                (figure.width, figure.height),
                text_size / figure.em,
                column,
                None,
            )?;
            // GPUI shows an SVG at the size it declares, drawn at twice it.
            Some(ImageSource::Image(figure.image(scale)))
        });
        Shown::Figure(image.unwrap_or_else(|| {
            ImageSource::Custom(Arc::new(|_, _| {
                Some(Err(ImageCacheError::Asset(
                    "the figure cannot be drawn".into(),
                )))
            }))
        }))
    }
}

impl EditorPage {
    /// Takes the figures the book resolved for the draft `text`.
    pub(super) fn set_visual_figures(
        &mut self,
        sources: FigureSources,
        text: &str,
        cx: &mut Context<'_, Self>,
    ) {
        let alone = alone_destinations(text);
        if let Ok(mut figures) = self.visual.figures.lock() {
            if *figures.sources == sources && figures.alone == alone {
                return;
            }
            figures.sources = Arc::new(sources);
            figures.alone = alone;
            figures.asked = None;
        }
        cx.notify();
    }

    /// Draws the figures of Visual in `palette` on the background, unless
    /// they are drawn or being drawn in it.
    pub(super) fn draw_visual_figures(&self, palette: Palette, cx: &mut Context<'_, Self>) {
        let state = Arc::clone(&self.visual.figures);
        let sources = {
            let Ok(mut figures) = state.lock() else {
                return;
            };
            if figures.sources.is_empty() || figures.asked == Some(palette) {
                return;
            }
            figures.asked = Some(palette);
            Arc::clone(&figures.sources)
        };
        cx.spawn(async move |this, cx| {
            let wanted = Arc::clone(&sources);
            let drawn = cx
                .background_spawn(compute::spawn(move || Figures::draw(&wanted, palette)))
                .await;
            let kept = state
                .lock()
                .is_ok_and(|mut figures| figures.keep(&sources, palette, drawn));
            if kept {
                this.update(cx, |_, cx| cx.notify()).ok();
            }
        })
        .detach();
    }
}

/// Shared by the page and the image sources of the block editor.
pub(super) type SharedFigures = Arc<Mutex<VisualFigures>>;

/// The destinations of the images that are the only content of their
/// paragraph, a reference-style one by the destination of its definition.
fn alone_destinations(text: &str) -> HashSet<String> {
    let Ok(tree) = to_mdast(text, &ParseOptions::gfm()) else {
        return HashSet::new();
    };
    let mut definitions = HashMap::new();
    collect_definitions(&tree, &mut definitions);
    let mut alone = HashSet::new();
    collect_alone(&tree, &definitions, &mut alone);
    alone
}

fn collect_definitions(node: &Node, definitions: &mut HashMap<String, String>) {
    if let Node::Definition(definition) = node {
        definitions
            .entry(definition.identifier.clone())
            .or_insert_with(|| definition.url.clone());
    }
    for child in node.children().into_iter().flatten() {
        collect_definitions(child, definitions);
    }
}

fn collect_alone(node: &Node, definitions: &HashMap<String, String>, alone: &mut HashSet<String>) {
    if let Node::Paragraph(paragraph) = node {
        let destination = match paragraph.children.as_slice() {
            [Node::Image(image)] => Some(image.url.clone()),
            [Node::ImageReference(reference)] => definitions.get(&reference.identifier).cloned(),
            _ => None,
        };
        alone.extend(destination);
        return;
    }
    for child in node.children().into_iter().flatten() {
        collect_alone(child, definitions, alone);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use gpui_kit::ImageSource;

    use super::{Shown, VisualFigures, alone_destinations};
    use crate::entities::figure::{FigureKind, FigureSource, FigureSources, Figures, Palette};

    fn sources(figures: &[(&str, FigureKind, &str)]) -> FigureSources {
        FigureSources {
            by_destination: figures
                .iter()
                .map(|(destination, kind, source)| {
                    (
                        (*destination).to_owned(),
                        FigureSource {
                            kind: *kind,
                            source: (*source).into(),
                        },
                    )
                })
                .collect(),
            by_reference: HashMap::new(),
        }
    }

    fn drawn(figures: &[(&str, FigureKind, &str)], alone: &[&str]) -> VisualFigures {
        let sources = sources(figures);
        let drawn = Figures::draw(&sources, Palette::PAPER);
        VisualFigures {
            sources: Arc::new(sources),
            alone: alone
                .iter()
                .map(|&destination| destination.to_owned())
                .collect(),
            drawn: Some((Palette::PAPER, Arc::new(drawn))),
            asked: Some(Palette::PAPER),
        }
    }

    /// The width and height an image of a figure declares.
    fn declared(shown: &Shown) -> (f32, f32) {
        let Shown::Figure(ImageSource::Image(image)) = shown else {
            panic!("not a drawn figure");
        };
        let svg = std::str::from_utf8(&image.bytes).unwrap();
        let attribute = |name: &str| -> f32 {
            let start = svg.find(&format!(" {name}=\"")).unwrap() + name.len() + 3;
            let end = start + svg[start..].find('"').unwrap();
            svg[start..end].trim_end_matches("px").parse().unwrap()
        };
        (attribute("width"), attribute("height"))
    }

    #[test]
    fn only_an_image_alone_in_its_paragraph_stands_alone() {
        let text = "![a](a.tex)\n\nSee ![b](b.tex) here.\n\n![c][c]\n\n\
                    > ![d](d.mmd)\n\n![e](e.tex) ![f](f.tex)\n\n[c]: c.tex";
        let expected: HashSet<String> = ["a.tex", "c.tex", "d.mmd"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        assert_eq!(alone_destinations(text), expected);
    }

    #[test]
    fn a_text_without_images_has_none_alone() {
        assert!(alone_destinations("").is_empty());
        assert!(alone_destinations("Just words.\n\n[x]: y.tex").is_empty());
    }

    #[test]
    fn a_destination_that_names_no_figure_is_left_to_the_pictures() {
        let figures = drawn(&[("e.tex", FigureKind::Formula, "E = mc^2")], &[]);
        assert!(matches!(
            figures.shown("pond.avif", 16.0, 600.0),
            Shown::NoFigure
        ));
    }

    #[test]
    fn a_figure_not_drawn_yet_is_still_loading() {
        let figures = VisualFigures {
            sources: Arc::new(sources(&[("e.tex", FigureKind::Formula, "x")])),
            ..VisualFigures::default()
        };
        let Shown::Figure(ImageSource::Custom(_)) = figures.shown("e.tex", 16.0, 600.0) else {
            panic!("a figure being drawn is an image that loads");
        };
    }

    #[test]
    fn a_figure_is_sized_for_the_text_and_no_wider_than_the_column() {
        let figures = drawn(
            &[(
                "d.mmd",
                FigureKind::Diagram,
                "flowchart LR\n  A[Gate] --> B[Path] --> C[Trees] --> D[Press]",
            )],
            &["d.mmd"],
        );
        let (small_width, small_height) = declared(&figures.shown("d.mmd", 10.0, 2000.0));
        let (large_width, large_height) = declared(&figures.shown("d.mmd", 20.0, 2000.0));
        assert!(
            (large_width - 2.0 * small_width).abs() < 0.5,
            "{small_width} {large_width}"
        );
        assert!((large_height - 2.0 * small_height).abs() < 0.5);
        let (narrow, _) = declared(&figures.shown("d.mmd", 20.0, small_width));
        assert!((narrow - small_width).abs() < 0.5, "{narrow} {small_width}");
    }

    #[test]
    fn a_formula_alone_is_a_display_formula_and_in_a_line_a_text_one() {
        let source = r"\sum_{i=1}^{n} i";
        let alone = drawn(&[("s.tex", FigureKind::Formula, source)], &["s.tex"]);
        let inline = drawn(&[("s.tex", FigureKind::Formula, source)], &[]);
        let (_, display) = declared(&alone.shown("s.tex", 16.0, 600.0));
        let (_, text) = declared(&inline.shown("s.tex", 16.0, 600.0));
        assert!(display > text, "{display} {text}");
    }

    #[test]
    fn a_drawing_is_kept_while_the_draft_shows_the_figures_it_was_made_of() {
        let wanted = Arc::new(sources(&[("e.tex", FigureKind::Formula, "x")]));
        let mut figures = VisualFigures {
            sources: Arc::clone(&wanted),
            ..VisualFigures::default()
        };
        assert!(figures.keep(
            &wanted,
            Palette::PAPER,
            Figures::draw(&wanted, Palette::PAPER)
        ));
        assert!(matches!(
            figures.shown("e.tex", 16.0, 600.0),
            Shown::Figure(ImageSource::Image(_))
        ));
    }

    #[test]
    fn a_drawing_of_figures_the_draft_no_longer_shows_is_dropped() {
        let old = Arc::new(sources(&[("e.tex", FigureKind::Formula, "x")]));
        let mut figures = VisualFigures {
            sources: Arc::new(sources(&[("e.tex", FigureKind::Formula, "y")])),
            ..VisualFigures::default()
        };
        assert!(!figures.keep(&old, Palette::PAPER, Figures::draw(&old, Palette::PAPER)));
        assert!(figures.drawn.is_none());
    }

    #[test]
    fn a_figure_that_cannot_be_drawn_is_an_image_that_failed() {
        let figures = drawn(&[("bad.tex", FigureKind::Formula, r"\frac{")], &[]);
        let Shown::Figure(ImageSource::Custom(_)) = figures.shown("bad.tex", 16.0, 600.0) else {
            panic!("a figure that failed is an image that failed");
        };
    }
}
