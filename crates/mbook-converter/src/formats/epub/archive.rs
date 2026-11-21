//! The ZIP container of an EPUB: what is checked before the package is read,
//! and how entries are unpacked within the limits.
//!
//! `rbook` copies an entry to memory without a bound, so the sizes that the
//! archive declares are checked first. `zip` stops an entry that unpacks to
//! more than its declared size, which makes the declared size a bound for
//! everything `rbook` reads. The entries this crate reads itself are bounded
//! by `Read::take` as well.

use crate::error::{Error, Result};
use crate::format::Format;
use mbook_core::Limits;
use std::io::{Cursor, Read};
use zip::ZipArchive;
use zip::result::ZipError;

/// The entry that lists encrypted resources.
const ENCRYPTION: &str = "META-INF/encryption.xml";

/// The entry of a rights management system.
const RIGHTS: &str = "META-INF/rights.xml";

/// The algorithms that obfuscate a font and nothing else: IDPF and Adobe.
const OBFUSCATION: [&str; 2] = [
    "http://www.idpf.org/2008/embedding",
    "http://ns.adobe.com/pdf/enc#RC",
];

/// An opened archive that counts what is unpacked from it.
pub(super) struct Archive<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    limits: Limits,
    /// The bytes unpacked so far.
    total: u64,
}

impl<'a> Archive<'a> {
    /// Opens an archive and checks it before anything is read from it.
    ///
    /// # Errors
    ///
    /// The source is not a ZIP archive ([`Error::Source`]), the book is
    /// protected ([`Error::Protected`]), or an entry is larger than a resource
    /// may be, or there are too many entries ([`Error::Limit`]).
    pub(super) fn open(source: &'a [u8], limits: Limits) -> Result<Self> {
        let zip = ZipArchive::new(Cursor::new(source)).map_err(wrap)?;
        let mut archive = Self {
            zip,
            limits,
            total: 0,
        };
        archive.check()?;
        Ok(archive)
    }

    /// Looks at every entry header: the entries must not be encrypted or too
    /// large, and the book must not be under rights management.
    fn check(&mut self) -> Result<()> {
        if self.zip.len() > self.limits.max_resources {
            return Err(Error::Limit("the archive has too many entries"));
        }
        let names: Vec<String> = self
            .zip
            .file_names()
            .map(|name| name.map(std::borrow::Cow::into_owned).map_err(wrap))
            .collect::<Result<_>>()?;
        for (index, name) in names.iter().enumerate() {
            let entry = self.zip.by_index_raw(index).map_err(wrap)?;
            if entry.encrypted() || name == RIGHTS {
                return Err(Error::Protected(Format::Epub));
            }
            if entry.size() > self.limits.max_resource_bytes {
                return Err(Error::Limit("an entry of the archive is too large"));
            }
        }
        if let Some(bytes) = self.read(ENCRYPTION)?
            && encrypts_content(&String::from_utf8_lossy(&bytes))
        {
            return Err(Error::Protected(Format::Epub));
        }
        // The check is not a use of the book.
        self.total = 0;
        Ok(())
    }

    /// Unpacks the entry with this name; `None` when there is none. The
    /// bytes count against the limits.
    ///
    /// # Errors
    ///
    /// The entry cannot be unpacked ([`Error::Source`]) or a limit is
    /// exceeded ([`Error::Limit`]).
    pub(super) fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>> {
        let Some(index) = self.zip.index_for_name(name) else {
            return Ok(None);
        };
        let entry = self.zip.by_index(index).map_err(|error| match error {
            ZipError::UnsupportedArchive(ZipError::PASSWORD_REQUIRED) => {
                Error::Protected(Format::Epub)
            }
            other => wrap(other),
        })?;
        // One byte more than allowed shows that the entry is too large,
        // whatever size its header declares.
        let mut bytes = Vec::new();
        entry
            .take(self.limits.max_resource_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(|error| Error::source(Format::Epub, error))?;
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if length > self.limits.max_resource_bytes {
            return Err(Error::Limit("an entry of the archive is too large"));
        }
        self.total = self.total.saturating_add(length);
        if self.total > self.limits.max_file_bytes {
            return Err(Error::Limit("the archive unpacks to too many bytes"));
        }
        Ok(Some(bytes))
    }
}

/// Wraps the error of `zip`.
fn wrap(error: ZipError) -> Error {
    Error::source(Format::Epub, error)
}

/// Tells whether `encryption.xml` names an algorithm that does more than
/// obfuscate a font. Every `Algorithm="…"` of the file is looked at; a file
/// without any names none.
fn encrypts_content(xml: &str) -> bool {
    let mut rest = xml;
    while let Some(at) = rest.find("Algorithm") {
        rest = rest.get(at + "Algorithm".len()..).unwrap_or_default();
        let Some(value) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let Some(quote) = value.chars().next().filter(|c| matches!(c, '"' | '\'')) else {
            continue;
        };
        let value = value.get(1..).unwrap_or_default();
        let end = value.find(quote).unwrap_or(value.len());
        if !OBFUSCATION.contains(&value.get(..end).unwrap_or_default().trim()) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::encrypts_content;

    #[test]
    fn font_obfuscation_alone_is_not_encryption() {
        let xml = r#"<encryption xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
            <EncryptedData xmlns="http://www.w3.org/2001/04/xmlenc#">
              <EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
              <CipherData><CipherReference URI="fonts/a.otf"/></CipherData>
            </EncryptedData>
            <EncryptedData>
              <EncryptionMethod Algorithm = 'http://ns.adobe.com/pdf/enc#RC'/>
            </EncryptedData></encryption>"#;
        assert!(!encrypts_content(xml));
    }

    #[test]
    fn another_algorithm_is_encryption() {
        let xml = r#"<EncryptedData>
            <EncryptionMethod Algorithm="http://www.w3.org/2001/04/xmlenc#aes128-cbc"/>
            </EncryptedData>"#;
        assert!(encrypts_content(xml));
        let mixed = r#"<EncryptionMethod Algorithm="http://www.idpf.org/2008/embedding"/>
            <EncryptionMethod Algorithm="urn:other"/>"#;
        assert!(encrypts_content(mixed));
    }

    #[test]
    fn a_file_without_algorithms_encrypts_nothing() {
        assert!(!encrypts_content(""));
        assert!(!encrypts_content("<encryption/>"));
        assert!(!encrypts_content(
            "Algorithm without a value, Algorithm=, Algorithm=x"
        ));
    }
}
