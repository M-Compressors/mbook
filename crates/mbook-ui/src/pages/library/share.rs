//! Sending the file of a library book through the share sheet of the
//! platform.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use gpui_kit::{App, Window};
use rust_i18n::t;

use super::export;
use crate::entities::catalogue::Store;
use crate::features::import_book::error_chain;
use crate::shared::model::BookKey;
use crate::shared::ui::announce;

/// A book that could not be offered to the share sheet.
#[derive(Debug, thiserror::Error)]
pub(super) enum ShareError {
    /// The copy named after the book could not be made.
    #[error("{}", t!("library.share_copy"))]
    Copy(#[from] io::Error),
    /// The share sheet of the platform could not be shown.
    #[cfg(not(target_os = "android"))]
    #[error("{}", t!("library.share_sheet"))]
    Sheet(#[from] robius_share::Error),
    /// The activity of Android could not show the share sheet.
    #[cfg(target_os = "android")]
    #[error("{}", t!("library.share_sheet"))]
    Sheet(#[source] android::SheetError),
}

/// Shows the share sheet of the platform over the key window with the file at
/// `path`, called `title`. Its targets (nearby devices, Messages, Mail,
/// messengers and social networks) receive the mbook file itself.
///
/// It must be called on the main thread; it returns once the sheet is shown,
/// not when the file has been sent.
#[cfg(not(target_os = "android"))]
pub(super) fn present(path: &Path, title: &str) -> Result<(), ShareError> {
    robius_share::ShareSheet::new()
        .set_title(title)
        .set_subject(title)
        .add_file(path)
        .share()
        .map_err(ShareError::from)
}

/// Asks the activity of Android to show the share sheet with the file at
/// `path`, in the folder of shared files, called `title`. The activity lends
/// the file through a provider of its own, and starts the sheet on its own
/// thread: the call returns before the sheet is shown.
#[cfg(target_os = "android")]
pub(super) fn present(path: &Path, title: &str) -> Result<(), ShareError> {
    android::share(path, title).map_err(ShareError::Sheet)
}

#[cfg(target_os = "android")]
mod android {
    use std::path::Path;

    use gpui_mobile::android::jni::{activity, find_app_class, with_env};
    use jni::objects::{JObject, JValue};

    /// The activity of `mbook-android`, which shows the sheet.
    const ACTIVITY: &str = "io.github.m_compressors.mbook.MbookActivity";

    /// Why the activity did not show the sheet.
    #[derive(Debug, thiserror::Error)]
    pub(crate) enum SheetError {
        /// A call into Java failed.
        #[error("{0}")]
        Java(String),
        /// The file is not one the activity may lend.
        #[error("{}", rust_i18n::t!("library.share_not_shared"))]
        NotShared,
    }

    pub(super) fn share(path: &Path, title: &str) -> Result<(), SheetError> {
        let path = path.to_string_lossy();
        let shared = with_env(|env| {
            let class = find_app_class(env, ACTIVITY)?;
            let activity = activity(env)?;
            let path = env
                .new_string(path.as_ref())
                .map_err(|error| error.to_string())?;
            let title = env.new_string(title).map_err(|error| error.to_string())?;
            env.call_static_method(
                &class,
                jni::jni_str!("share"),
                jni::jni_sig!("(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)Z"),
                &[
                    JValue::Object(&activity),
                    JValue::Object(&JObject::from(path)),
                    JValue::Object(&JObject::from(title)),
                ],
            )
            .and_then(jni::objects::JValueOwned::z)
            .map_err(|error| {
                env.exception_clear();
                error.to_string()
            })
        })
        .map_err(SheetError::Java)?;
        if shared {
            Ok(())
        } else {
            Err(SheetError::NotShared)
        }
    }
}

/// Offers the book `key`, called `title`, to `present` under a file named
/// after the title, and announces a failure. The copy is made on the
/// background executor; `present` runs on the main thread. A shown sheet is
/// not announced: the platform reports what the reader does with it.
pub(super) fn choose(
    key: BookKey,
    title: &str,
    window: &Window,
    cx: &App,
    present: impl FnOnce(&Path, &str) -> Result<(), ShareError> + 'static,
) {
    let roots = &Store::global(cx).roots;
    let source = roots.book(key);
    let folder = roots.shared();
    let name = format!("{}.mbook", export::base_name(title));
    let title = title.to_owned();
    let executor = cx.background_executor().clone();
    window
        .spawn(cx, async move |cx| {
            let staged = executor
                .spawn(async move { stage(&source, &folder, &name) })
                .await;
            cx.update(|window, cx| {
                let shown = staged
                    .map_err(ShareError::from)
                    .and_then(|path| present(&path, &title));
                if let Err(error) = shown {
                    announce(failure(&title, &error), window, cx);
                }
            })
            .ok();
        })
        .detach();
}

/// Puts the file `source` in a new folder under `folder` as `name`, and
/// returns its path. A hard link is made where the volume allows one, a copy
/// elsewhere. Each share has a folder of its own, so two books of one title
/// do not take each other's place while an app still reads the first.
fn stage(source: &Path, folder: &Path, name: &str) -> io::Result<PathBuf> {
    fs::create_dir_all(folder)?;
    let own = tempfile::Builder::new()
        .prefix("share-")
        .tempdir_in(folder)?
        .keep();
    let path = own.join(name);
    if fs::hard_link(source, &path).is_err() {
        fs::copy(source, &path)?;
    }
    Ok(path)
}

/// The announcement of a book that could not be offered.
fn failure(title: &str, error: &ShareError) -> String {
    t!(
        "library.share_failed",
        title = title,
        reason = error_chain(error)
    )
    .into_owned()
}

#[cfg(test)]
mod tests {
    use std::error::Error as _;

    use super::*;

    #[test]
    fn the_cause_of_a_failed_sheet_is_kept_as_the_source() {
        let error = ShareError::from(robius_share::Error::AlreadyOpen);
        assert_eq!(error.to_string(), "the share sheet could not be shown");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("another share sheet is already open")
        );
    }

    #[test]
    fn the_cause_of_a_failed_copy_is_kept_as_the_source() {
        let error = ShareError::from(io::Error::other("disk full"));
        assert_eq!(error.to_string(), "a copy of the book could not be made");
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("disk full")
        );
    }

    #[test]
    fn a_failure_names_the_book_and_the_cause() {
        let error = ShareError::from(robius_share::Error::Unsupported);
        assert_eq!(
            failure("Dune", &error),
            "Could not share Dune. the share sheet could not be shown: \
             this platform is unsupported."
        );
    }

    #[test]
    fn a_staged_book_has_the_given_name_and_the_bytes_of_the_source() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let source = dir.path().join("0123.mbook");
        fs::write(&source, b"a book")?;

        let staged = stage(&source, &dir.path().join("share"), "Dune.mbook")?;

        assert_eq!(
            staged.file_name().and_then(|name| name.to_str()),
            Some("Dune.mbook")
        );
        assert!(staged.starts_with(dir.path().join("share")));
        assert_eq!(fs::read(staged)?, b"a book");
        assert_eq!(fs::read(source)?, b"a book");
        Ok(())
    }

    #[test]
    fn two_books_of_one_title_are_staged_apart() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let first = dir.path().join("first.mbook");
        let second = dir.path().join("second.mbook");
        fs::write(&first, b"first")?;
        fs::write(&second, b"second")?;
        let folder = dir.path().join("share");

        let one = stage(&first, &folder, "Dune.mbook")?;
        let other = stage(&second, &folder, "Dune.mbook")?;

        assert_ne!(one, other);
        assert_eq!(fs::read(one)?, b"first");
        assert_eq!(fs::read(other)?, b"second");
        Ok(())
    }

    #[test]
    fn a_missing_source_is_an_error() -> io::Result<()> {
        let dir = tempfile::tempdir()?;
        let missing = dir.path().join("missing.mbook");
        assert!(stage(&missing, &dir.path().join("share"), "Dune.mbook").is_err());
        Ok(())
    }
}
