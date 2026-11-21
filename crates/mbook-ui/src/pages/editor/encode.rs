//! Reading a picture file and encoding it as AVIF, on the background.

use std::fs::File;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

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
}

/// A picture file waiting to be encoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Job {
    /// The file the user chose.
    pub(super) file: PathBuf,
    /// What it is for.
    pub(super) target: Target,
}

impl Job {
    /// The name of the file, without its folders.
    pub(super) fn name(&self) -> String {
        self.file
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    }
}

/// A picture that is ready to be added once it has a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Encoded {
    /// The name of the file it came from.
    pub(super) name: String,
    /// What it is for.
    pub(super) target: Target,
    /// The AVIF file.
    pub(super) avif: Vec<u8>,
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
pub(super) fn encode_file(path: &Path) -> Result<Vec<u8>, FileError> {
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
    encode_avif(&bytes, EncodeOptions::default()).map_err(FileError::Picture)
}

/// Words for what went wrong with a file, at the control that took it.
pub(super) fn words(name: &str, error: &FileError) -> String {
    let reason = match error {
        FileError::Read(source) => format!("it could not be read ({source})"),
        FileError::TooLarge { .. } => "it is larger than 256 MiB".to_owned(),
        FileError::Picture(PictureError::UnsupportedFormat) => {
            "it is not an AVIF, JPEG, PNG, GIF, WebP or BMP picture".to_owned()
        }
        FileError::Picture(PictureError::TooManyPixels { .. }) => {
            "it has more than 100 million pixels".to_owned()
        }
        FileError::Picture(_) => "it is damaged or cannot be decoded".to_owned(),
    };
    format!("{name} was not added: {reason}.")
}
