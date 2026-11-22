//! The error of this slice.

use thiserror::Error;

/// Why a diagram or a formula could not be drawn.
///
/// The messages of the renderers are kept as text: the editor shows them to
/// the author beside the source, and a failure is remembered in the cache,
/// which needs it to be `Clone`.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
#[non_exhaustive]
pub(crate) enum FigureError {
    /// The source holds nothing to draw.
    #[error("there is nothing to draw")]
    Empty,
    /// The source is not a diagram in Mermaid notation that can be drawn.
    #[error("the diagram cannot be drawn: {0}")]
    Diagram(String),
    /// The source is not a formula in the notation of `KaTeX` that can be
    /// set.
    #[error("the formula cannot be set: {0}")]
    Formula(String),
    /// The renderer stopped unexpectedly.
    #[error("the renderer stopped unexpectedly")]
    Renderer,
}

#[cfg(test)]
mod tests {
    use super::FigureError;

    #[test]
    fn a_failure_says_what_the_renderer_said() {
        assert_eq!(
            FigureError::Diagram("no diagram type".into()).to_string(),
            "the diagram cannot be drawn: no diagram type"
        );
        assert_eq!(
            FigureError::Formula("missing brace".into()).to_string(),
            "the formula cannot be set: missing brace"
        );
        assert_eq!(FigureError::Empty.to_string(), "there is nothing to draw");
        assert_eq!(
            FigureError::Renderer.to_string(),
            "the renderer stopped unexpectedly"
        );
    }
}
