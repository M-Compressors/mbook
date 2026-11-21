//! The ways an update can fail.

use std::error::Error;
use std::io;
use std::path::PathBuf;

/// The error of a dependency, kept as the `source()` of an [`UpdateError`] so
/// that no type of the dependency is named by callers.
pub(crate) type Source = Box<dyn Error + Send + Sync>;

/// Why a release could not be found or installed.
///
/// A message never repeats its source: read it through
/// [`source`](std::error::Error::source).
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub(crate) enum UpdateError {
    /// The releases of the repository could not be asked: no network, a
    /// refusal of the server, or an answer that is not a list of releases.
    #[error("the releases of the repository could not be asked")]
    Releases(#[source] Source),
    /// The archive of the release could not be downloaded.
    #[error("the update could not be downloaded")]
    Download(#[source] Source),
    /// The archive is not signed with the key of the application. Nothing was
    /// installed and the downloaded file is deleted.
    #[error("the signature of the update is not valid")]
    Signature(#[source] Source),
    /// The verified archive could not be unpacked.
    #[error("the update could not be unpacked")]
    Unpack(#[source] Source),
    /// The verified archive does not hold the file the platform installs.
    #[error("the update holds no {0}")]
    MissingInArchive(&'static str),
    /// A file or folder of the update could not be read, written or moved.
    #[error("a file of the update could not be written")]
    Io(#[from] io::Error),
    /// The running executable is not inside an application bundle, as it is
    /// when started by `cargo run`. A bare binary is never replaced.
    #[error("the running program is not inside an application bundle: {}", .path.display())]
    NotInBundle {
        /// The path of the running executable.
        path: PathBuf,
    },
    /// Updates cannot be installed on this system.
    #[error("updates cannot be installed on this platform")]
    #[cfg_attr(
        all(any(target_os = "macos", target_os = "windows"), not(test)),
        expect(dead_code, reason = "only systems without an installer report it")
    )]
    Unsupported,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_does_not_repeat_its_source() {
        let error = UpdateError::Releases("connection refused".into());
        assert_eq!(
            error.to_string(),
            "the releases of the repository could not be asked"
        );
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("connection refused")
        );
    }

    #[test]
    fn download_and_signature_and_unpack_keep_their_source() {
        let errors = [
            UpdateError::Download("reset".into()),
            UpdateError::Signature("no match".into()),
            UpdateError::Unpack("truncated".into()),
        ];
        let sources: Vec<_> = errors
            .iter()
            .map(|error| error.source().map(ToString::to_string))
            .collect();
        assert_eq!(
            sources,
            [
                Some("reset".to_owned()),
                Some("no match".to_owned()),
                Some("truncated".to_owned())
            ]
        );
    }

    #[test]
    fn an_io_error_converts_and_is_the_source() {
        let error = UpdateError::from(io::Error::new(io::ErrorKind::PermissionDenied, "denied"));
        assert!(matches!(error, UpdateError::Io(_)));
        assert_eq!(
            error.to_string(),
            "a file of the update could not be written"
        );
        assert_eq!(
            error.source().map(ToString::to_string).as_deref(),
            Some("denied")
        );
    }

    #[test]
    fn a_missing_bundle_names_the_executable() {
        let error = UpdateError::NotInBundle {
            path: PathBuf::from("/work/target/debug/mbook"),
        };
        assert_eq!(
            error.to_string(),
            "the running program is not inside an application bundle: /work/target/debug/mbook"
        );
        assert!(error.source().is_none());
    }

    #[test]
    fn messages_without_a_source_have_none() {
        assert_eq!(
            UpdateError::Unsupported.to_string(),
            "updates cannot be installed on this platform"
        );
        assert!(UpdateError::Unsupported.source().is_none());
        let missing = UpdateError::MissingInArchive("mbook.app");
        assert_eq!(missing.to_string(), "the update holds no mbook.app");
        assert!(missing.source().is_none());
    }
}
