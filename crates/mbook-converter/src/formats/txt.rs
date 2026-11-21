//! Plain text.
//!
//! The encoding is detected, paragraphs are what empty lines separate, and
//! everything Markdown would read as markup is escaped. Plain text has no
//! metadata, no pictures and no contents: the book is one section.

use crate::draft::{Draft, Section};
use crate::encoding;
use crate::error::Result;
use crate::markdown;
use crate::options::ConvertOptions;

/// Reads plain text into a draft of one section.
#[expect(
    clippy::unnecessary_wraps,
    reason = "every front end has the same signature, and the others can fail"
)]
pub(crate) fn read(source: &[u8], _options: &ConvertOptions) -> Result<Draft> {
    let text = encoding::decode(source);
    Ok(Draft {
        sections: vec![Section {
            name: "text".to_owned(),
            markdown: markdown::escape(&text),
        }],
        ..Draft::default()
    })
}
