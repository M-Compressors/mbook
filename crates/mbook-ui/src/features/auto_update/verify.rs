//! Verifying a downloaded archive against the key of the application.

use std::path::Path;

use super::error::UpdateError;

/// The public key the archives are verified against: 32 bytes of ed25519, as
/// `zipsign gen-key` writes them.
pub(super) const KEY: [u8; 32] = *include_bytes!("../../../assets/update.pub");

/// Checks that `archive` was signed with the key whose public half is `key`.
///
/// The signature is salted with the file name of the archive, so a file
/// signed under another name is refused. Reads the file; extracts nothing.
///
/// # Errors
///
/// [`UpdateError::Signature`] when the file carries no signature, or none
/// that `key` and the file name match, or when the file was changed after it
/// was signed; [`UpdateError::Io`] when it cannot be read.
pub(super) fn verify(archive: &Path, key: &[u8; 32]) -> Result<(), UpdateError> {
    // An empty list of keys would accept everything: there is always one.
    self_update::verify_signature(archive, &[*key]).map_err(|error| match error {
        self_update::errors::Error::Io(error) => UpdateError::Io(error),
        error => UpdateError::Signature(Box::new(error)),
    })
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::fs;

    use super::super::fixtures::{Entry, public_key, sign, signing_key, tar_gz};
    use super::*;

    const NAME: &str = "mbook-v0.2.0-aarch64-apple-darwin.tar.gz";

    fn archive() -> Vec<u8> {
        tar_gz(&[Entry {
            path: "mbook.app/Contents/Info.plist",
            mode: 0o644,
            data: b"plist",
        }])
    }

    fn write(dir: &Path, name: &str, bytes: &[u8]) -> std::io::Result<std::path::PathBuf> {
        let path = dir.join(name);
        fs::write(&path, bytes)?;
        Ok(path)
    }

    fn is_signature_error(result: &Result<(), UpdateError>) -> bool {
        matches!(result, Err(UpdateError::Signature(_)))
    }

    #[test]
    fn the_embedded_key_is_a_valid_public_key() -> Result<(), Box<dyn Error>> {
        let keys = self_update::zipsign_api::verify::collect_keys([Ok(KEY)])?;
        assert_eq!(keys.len(), 1);
        Ok(())
    }

    #[test]
    fn the_embedded_key_is_not_the_key_of_the_tests() {
        assert_ne!(KEY, public_key(1));
    }

    #[test]
    fn an_archive_signed_with_the_key_is_accepted() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let signed = sign(&archive(), &signing_key(1), NAME)?;
        let path = write(dir.path(), NAME, &signed)?;
        verify(&path, &public_key(1))?;
        Ok(())
    }

    #[test]
    fn an_archive_signed_with_another_key_is_refused() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let signed = sign(&archive(), &signing_key(2), NAME)?;
        let path = write(dir.path(), NAME, &signed)?;
        assert!(is_signature_error(&verify(&path, &public_key(1))));
        Ok(())
    }

    #[test]
    fn an_archive_with_a_changed_byte_is_refused() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let mut signed = sign(&archive(), &signing_key(1), NAME)?;
        signed[40] ^= 1;
        let path = write(dir.path(), NAME, &signed)?;
        assert!(is_signature_error(&verify(&path, &public_key(1))));
        Ok(())
    }

    #[test]
    fn an_archive_under_another_name_is_refused() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let signed = sign(&archive(), &signing_key(1), NAME)?;
        let path = write(
            dir.path(),
            "mbook-v0.9.9-aarch64-apple-darwin.tar.gz",
            &signed,
        )?;
        assert!(is_signature_error(&verify(&path, &public_key(1))));
        Ok(())
    }

    #[test]
    fn an_unsigned_archive_is_refused() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = write(dir.path(), NAME, &archive())?;
        assert!(is_signature_error(&verify(&path, &public_key(1))));
        Ok(())
    }

    #[test]
    fn a_file_that_is_no_archive_is_refused() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = write(dir.path(), NAME, b"")?;
        assert!(verify(&path, &public_key(1)).is_err());
        Ok(())
    }

    #[test]
    fn a_refusal_keeps_its_cause() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = write(dir.path(), NAME, &archive())?;
        let error = verify(&path, &public_key(1)).err().ok_or("accepted")?;
        assert_eq!(
            error.to_string(),
            "the signature of the update is not valid"
        );
        assert!(error.source().is_some());
        Ok(())
    }

    #[test]
    fn a_missing_file_is_an_io_error() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let result = verify(&dir.path().join(NAME), &public_key(1));
        assert!(matches!(result, Err(UpdateError::Io(_))));
        Ok(())
    }
}
