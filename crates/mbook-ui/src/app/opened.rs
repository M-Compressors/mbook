//! Files the system hands the application to open: named on its command line,
//! or opened with it from a file manager, which the platform reports as URLs
//! (mbook-ui docs, "Opening files from the system").
//!
//! They wait in [`Opened`] until the library is open, and are then added as a
//! dropped file is: one file opens its book once it is in the library.

use std::path::PathBuf;

use futures::StreamExt as _;
use futures::channel::mpsc::UnboundedReceiver;
use gpui_kit::{App, Global};
use percent_encoding::percent_decode_str;

/// The files handed over and not yet given to the library.
#[derive(Debug, Default)]
pub(super) struct Opened {
    pending: Vec<PathBuf>,
}

impl Global for Opened {}

impl Opened {
    /// Queues `files`, which notifies the observers of the global.
    pub(super) fn push(files: Vec<PathBuf>, cx: &mut App) {
        if files.is_empty() {
            return;
        }
        cx.default_global::<Self>().pending.extend(files);
    }

    /// The files queued so far; the queue is empty afterwards.
    ///
    /// An empty queue is only read: writing a global notifies its observers,
    /// and the shell, which observes it, takes from it when notified.
    pub(super) fn take(cx: &mut App) -> Vec<PathBuf> {
        let queued = cx
            .try_global::<Self>()
            .is_some_and(|opened| !opened.pending.is_empty());
        if !queued {
            return Vec::new();
        }
        std::mem::take(&mut cx.global_mut::<Self>().pending)
    }
}

/// Queues what arrives on `urls` for as long as the application runs. The
/// platform calls back without access to the application, so its URLs come
/// over a channel.
pub(super) fn listen(mut urls: UnboundedReceiver<Vec<String>>, cx: &mut App) {
    cx.spawn(async move |cx| {
        while let Some(batch) = urls.next().await {
            let files = paths_of(&batch);
            cx.update(|cx| Opened::push(files, cx));
        }
    })
    .detach();
}

/// Takes, once a second, the documents the activity of Android received.
///
/// Android hands a document opened or shared with the application to its
/// activity, as an intent that only the activity may read. The activity of
/// `mbook-android` copies it into the cache and queues the path; nothing
/// wakes the interface for it, so it asks.
#[cfg(target_os = "android")]
pub(super) fn poll_android(cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(std::time::Duration::from_secs(1))
                .await;
            let files = android::take_opened();
            if !files.is_empty() {
                cx.update(|cx| Opened::push(files, cx));
            }
        }
    })
    .detach();
}

#[cfg(target_os = "android")]
mod android {
    use std::path::PathBuf;

    use gpui_mobile::android::jni::{find_app_class, get_string, with_env};

    /// The activity of `mbook-android`, which queues the documents.
    const ACTIVITY: &str = "io.github.m_compressors.mbook.MbookActivity";

    /// The paths the activity queued since the last call. A failed call is
    /// written to logcat and taken for none: the next one asks again.
    pub(super) fn take_opened() -> Vec<PathBuf> {
        let taken = with_env(|env| {
            let class = find_app_class(env, ACTIVITY)?;
            let paths = env
                .call_static_method(
                    &class,
                    jni::jni_str!("takeOpened"),
                    jni::jni_sig!("()Ljava/lang/String;"),
                    &[],
                )
                .and_then(jni::objects::JValueOwned::l)
                .map_err(|error| {
                    env.exception_clear();
                    error.to_string()
                })?;
            Ok(get_string(env, &paths))
        });
        match taken {
            Ok(paths) => super::lines(&paths),
            Err(error) => {
                log::warn!("the documents of the activity could not be taken: {error}");
                Vec::new()
            }
        }
    }
}

/// The paths of `text`, one per line, without empty lines.
#[cfg(any(target_os = "android", test))]
fn lines(text: &str) -> Vec<PathBuf> {
    text.lines()
        .filter(|line| !line.is_empty())
        .map(PathBuf::from)
        .collect()
}

/// The files among `urls`: a `file:` URL, as macOS and Android report a
/// document, or a path, as a command line names one. Other URLs name no
/// file and are left out.
pub(super) fn paths_of(urls: &[String]) -> Vec<PathBuf> {
    urls.iter().filter_map(|url| path_of(url)).collect()
}

fn path_of(url: &str) -> Option<PathBuf> {
    if let Some(rest) = url.strip_prefix("file://") {
        // `file:///path` has an empty host; `file://localhost/path` names
        // this machine. A file of another host is not opened.
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        let decoded = percent_decode_str(path).decode_utf8().ok()?;
        return Some(PathBuf::from(decoded.as_ref()));
    }
    // Any other scheme is a URL, not a path; a letter and a colon starts a
    // path on Windows.
    let scheme = url.split_once(':').map(|(scheme, _)| scheme);
    match scheme {
        Some(scheme) if scheme.len() > 1 => None,
        _ if url.is_empty() => None,
        _ => Some(PathBuf::from(url)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(urls: &[&str]) -> Vec<PathBuf> {
        paths_of(&urls.iter().map(ToString::to_string).collect::<Vec<_>>())
    }

    #[test]
    fn a_file_url_is_the_decoded_path() {
        assert_eq!(
            paths(&["file:///Users/a/My%20Book.epub"]),
            [PathBuf::from("/Users/a/My Book.epub")]
        );
        assert_eq!(
            paths(&["file://localhost/tmp/%D0%9A%D0%BD%D0%B8%D0%B3%D0%B0.fb2"]),
            [PathBuf::from("/tmp/Книга.fb2")]
        );
    }

    #[test]
    fn a_path_of_a_command_line_is_kept_as_it_is() {
        assert_eq!(
            paths(&["/home/a/book.mbook", r"C:\Books\a.pdf", "relative.txt"]),
            [
                PathBuf::from("/home/a/book.mbook"),
                PathBuf::from(r"C:\Books\a.pdf"),
                PathBuf::from("relative.txt"),
            ]
        );
    }

    #[test]
    fn a_url_that_names_no_local_file_is_left_out() {
        assert_eq!(
            paths(&[
                "https://example.com/a.epub",
                "mbook://open",
                "file://server/share/a.epub",
                "file:///bad%FF",
                "",
            ]),
            Vec::<PathBuf>::new()
        );
    }

    #[test]
    fn the_paths_of_the_activity_come_one_per_line() {
        assert_eq!(
            lines("/cache/opened/0/Dune.epub\n/cache/opened/1/Книга.fb2\n\n"),
            [
                PathBuf::from("/cache/opened/0/Dune.epub"),
                PathBuf::from("/cache/opened/1/Книга.fb2"),
            ]
        );
        assert_eq!(lines(""), Vec::<PathBuf>::new());
    }

    #[gpui_kit::test]
    fn opened_files_wait_until_they_are_taken(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            assert!(Opened::take(cx).is_empty(), "nothing is queued at first");
            Opened::push(vec![PathBuf::from("/a.epub")], cx);
            Opened::push(Vec::new(), cx);
            Opened::push(vec![PathBuf::from("/b.fb2")], cx);
            assert_eq!(
                Opened::take(cx),
                [PathBuf::from("/a.epub"), PathBuf::from("/b.fb2")]
            );
            assert_eq!(Opened::take(cx), Vec::<PathBuf>::new());
        });
    }

    #[gpui_kit::test]
    fn urls_on_the_channel_are_queued_as_files(cx: &mut gpui_kit::TestAppContext) {
        let (sender, receiver) = futures::channel::mpsc::unbounded();
        cx.update(|cx| listen(receiver, cx));
        sender
            .unbounded_send(vec![
                "file:///x/One.epub".to_owned(),
                "https://example.com".to_owned(),
            ])
            .unwrap();
        cx.run_until_parked();
        cx.update(|cx| assert_eq!(Opened::take(cx), [PathBuf::from("/x/One.epub")]));
    }
}
