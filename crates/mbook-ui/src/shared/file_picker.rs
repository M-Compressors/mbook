//! Native file selection with extension filters.
//!
//! The desktop asks `rfd` for the dialog of the platform. Android asks the
//! GPUI platform of gpui-mobile, whose picker is the document picker of the
//! Storage Access Framework: it copies each chosen document into the cache of
//! the application and returns those copies. Its picker has no extension
//! filter, so the extensions are checked here on every platform.

use std::path::PathBuf;

use thiserror::Error;

/// A file chosen that does not have one of the extensions asked for.
#[derive(Debug, Error)]
#[error("\"{name}\" does not have a supported file extension.")]
pub(crate) struct UnsupportedExtension {
    name: String,
}

/// Why no files came back from [`choose`].
#[derive(Debug, Error)]
pub(crate) enum ChooseError {
    /// A file chosen does not have a supported extension.
    #[error(transparent)]
    Extension(#[from] UnsupportedExtension),
    /// The picker of the platform failed.
    #[cfg(target_os = "android")]
    #[error("The file picker could not be shown.")]
    Picker(#[source] Box<dyn std::error::Error + Send + Sync>),
}

/// Opens a native file picker for the given extensions.
///
/// Returns `Ok(None)` if the picker is dismissed without a selection.
///
/// # Errors
///
/// Returns an error if a chosen file does not have one of the requested
/// extensions, or, on Android, if the picker fails.
#[cfg(not(target_os = "android"))]
pub(crate) async fn choose(
    title: &str,
    filter: &str,
    extensions: &[&str],
    multiple: bool,
) -> Result<Option<Vec<PathBuf>>, ChooseError> {
    let dialog = rfd::AsyncFileDialog::new()
        .set_title(title)
        .add_filter(filter, extensions);
    let files = if multiple {
        dialog.pick_files().await
    } else {
        dialog.pick_file().await.map(|file| vec![file])
    };
    let paths = files.map(|files| files.iter().map(|file| file.path().to_path_buf()).collect());
    checked(paths, extensions)
}

/// Opens the document picker of Android. Its title and filter name are not
/// shown: the system picker has neither.
///
/// # Errors
///
/// Returns an error if a chosen file does not have one of the requested
/// extensions, or if the picker fails.
#[cfg(target_os = "android")]
pub(crate) async fn choose(
    _title: &str,
    _filter: &str,
    extensions: &[&str],
    multiple: bool,
) -> Result<Option<Vec<PathBuf>>, ChooseError> {
    use gpui_kit::{PathPromptOptions, Platform as _};

    let Some(platform) = gpui_mobile::android::jni::platform() else {
        return Err(ChooseError::Picker("the platform is not running".into()));
    };
    let options = PathPromptOptions {
        files: true,
        directories: false,
        multiple,
        prompt: None,
    };
    let paths = platform
        .as_ref()
        .prompt_for_paths(options)
        .await
        .map_err(|error| ChooseError::Picker(error.into()))?
        .map_err(|error| ChooseError::Picker(error.into()))?;
    checked(paths, extensions)
}

/// The selection `paths`, or `None` for a dismissed picker, once every path
/// has one of `extensions`.
fn checked(
    paths: Option<Vec<PathBuf>>,
    extensions: &[&str],
) -> Result<Option<Vec<PathBuf>>, ChooseError> {
    Ok(paths.map(|paths| validate(paths, extensions)).transpose()?)
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
    fn a_dismissed_picker_is_no_selection() {
        assert!(checked(None, &["png"]).unwrap().is_none());
    }

    #[test]
    fn a_checked_selection_reports_the_file_with_the_wrong_extension() {
        let error = checked(Some(vec![PathBuf::from("notes.txt")]), &["png"]).unwrap_err();
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
