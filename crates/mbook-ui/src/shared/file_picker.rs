//! Native file selection with extension filters.

use std::path::PathBuf;

use rfd::AsyncFileDialog;
use thiserror::Error;

/// A file returned by the platform dialog despite its extension filter.
#[derive(Debug, Error)]
#[error("\"{name}\" does not have a supported file extension.")]
pub(crate) struct UnsupportedExtension {
    name: String,
}

/// Opens a native file picker for the given extensions.
///
/// Returns `Ok(None)` if the picker is dismissed without a selection.
///
/// # Errors
///
/// Returns an error if the system picker returns a path that does not match
/// the requested extensions.
pub(crate) async fn choose(
    title: &str,
    filter: &str,
    extensions: &[&str],
    multiple: bool,
) -> Result<Option<Vec<PathBuf>>, UnsupportedExtension> {
    let dialog = AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter, extensions);
    let files = if multiple {
        dialog.pick_files().await
    } else {
        dialog.pick_file().await.map(|file| vec![file])
    };
    files
        .map(|files| {
            let paths = files.iter().map(|file| file.path().to_path_buf()).collect();
            validate(paths, extensions)
        })
        .transpose()
}

fn validate(
    paths: Vec<PathBuf>,
    extensions: &[&str],
) -> Result<Vec<PathBuf>, UnsupportedExtension> {
    for path in &paths {
        let extension = path.extension().and_then(std::ffi::OsStr::to_str);
        if !extension.is_some_and(|ext| {
            extensions
                .iter()
                .any(|allowed| ext.eq_ignore_ascii_case(allowed))
        }) {
            let name = path
                .file_name()
                .unwrap_or(path.as_os_str())
                .to_string_lossy()
                .into_owned();
            return Err(UnsupportedExtension { name });
        }
    }
    Ok(paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_accepts_only_supported_extensions_regardless_of_letter_case() {
        let paths = [PathBuf::from("Cover.PNG"), PathBuf::from("Cover.avif")];
        assert_eq!(validate(paths.to_vec(), &["png", "avif"]).unwrap(), paths);
    }

    #[test]
    fn selection_rejects_a_mixed_batch_with_an_unsupported_file() {
        let paths = [PathBuf::from("Cover.png"), PathBuf::from("notes.txt")];
        let error = validate(paths.to_vec(), &["png"]).unwrap_err();
        assert_eq!(
            error.to_string(),
            "\"notes.txt\" does not have a supported file extension."
        );
    }

    #[test]
    fn selection_rejects_a_file_without_an_extension() {
        let error = validate(vec![PathBuf::from("cover")], &["png"]).unwrap_err();
        assert_eq!(error.name, "cover");
    }
}
