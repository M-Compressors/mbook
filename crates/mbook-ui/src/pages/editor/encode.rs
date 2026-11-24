//! Reading a picture file and encoding it as AVIF, on the background.

use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use rust_i18n::t;

use crate::entities::picture::{EncodeOptions, PictureError, encode_avif};

/// The largest file that is read: the file is held in memory while it is
/// decoded.
pub(super) const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;

/// What a picture is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// A picture of the book.
    Picture,
    /// The cover, which is a picture too.
    Cover,
    /// A picture of the book that is also put into the chapter being edited,
    /// where the caret is.
    Chapter,
}

/// Where a picture that is being added goes in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Placement {
    /// The path of the chapter that was open when the picture was chosen.
    pub(super) chapter: String,
    /// What is read to those who cannot see the picture.
    pub(super) description: String,
}

/// A picture file waiting to be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Job {
    /// The file the user chose.
    pub(super) file: PathBuf,
    /// What it is for.
    pub(super) target: Target,
    /// The path selected for the picture inside the book.
    pub(super) path: String,
    /// The output options selected for this file.
    pub(super) options: EncodeOptions,
    /// The chapter the picture is put into; `None` for one that only joins
    /// the pictures of the book.
    pub(super) placement: Option<Placement>,
}

impl Job {
    /// The name of the file, without its folders.
    pub(super) fn name(&self) -> String {
        self.file
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }
}

/// Why a file could not be turned into an AVIF picture.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(super) enum FileError {
    /// The file could not be read.
    #[error("the file could not be read")]
    Read(#[source] io::Error),
    /// The file is larger than [`MAX_FILE_BYTES`].
    #[error("the file has {len} bytes, more than the {limit} allowed")]
    TooLarge {
        /// The size of the file.
        len: u64,
        /// The limit.
        limit: u64,
    },
    /// The picture could not be decoded or encoded.
    #[error("the picture could not be used")]
    Picture(#[source] PictureError),
}

/// Reads a file and returns it as AVIF. It blocks: run it on the background.
///
/// # Errors
///
/// [`FileError::Read`] when the file cannot be read, [`FileError::TooLarge`]
/// above [`MAX_FILE_BYTES`] and [`FileError::Picture`] when its content is
/// not a picture that can be used.
pub(super) fn encode_file(path: &Path, options: EncodeOptions) -> Result<Vec<u8>, FileError> {
    let file = File::open(path).map_err(FileError::Read)?;
    let len = file.metadata().map_err(FileError::Read)?.len();
    if len > MAX_FILE_BYTES {
        return Err(FileError::TooLarge {
            len,
            limit: MAX_FILE_BYTES,
        });
    }
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES)
        .read_to_end(&mut bytes)
        .map_err(FileError::Read)?;
    encode_avif(&bytes, options).map_err(FileError::Picture)
}

/// Words for what went wrong with a file, at the control that took it.
pub(super) fn words(name: &str, error: &FileError) -> String {
    let reason = match error {
        FileError::Read(source) => t!("editor.file.unreadable", source = source),
        FileError::TooLarge { .. } => t!("editor.file.too_large"),
        FileError::Picture(PictureError::UnsupportedFormat) => t!("editor.file.unsupported"),
        FileError::Picture(PictureError::TooManyPixels { .. }) => {
            t!("editor.file.too_many_pixels")
        }
        FileError::Picture(_) => t!("editor.file.damaged"),
    };
    t!("editor.file.not_added", name = name, reason = reason).into_owned()
}
