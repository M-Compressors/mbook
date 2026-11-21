//! Reading text whose encoding nobody declared.

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};

/// The text of a source in any encoding. A byte order mark decides; without
/// one the encoding is guessed from the bytes, and bytes that are not text
/// in it become U+FFFD.
pub(crate) fn decode(source: &[u8]) -> String {
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(source, true);
    let guessed = detector.guess(None, Utf8Detection::Allow);
    let (text, _, _) = guessed.decode(source);
    text.into_owned()
}

#[cfg(test)]
mod tests {
    use super::decode;

    #[test]
    fn utf_8_is_read_as_it_is() {
        assert_eq!(
            decode("Привет, мир — «да».".as_bytes()),
            "Привет, мир — «да»."
        );
    }

    #[test]
    fn a_byte_order_mark_decides_the_encoding_and_is_dropped() {
        let mut utf16 = vec![0xff, 0xfe];
        for unit in "Hello".encode_utf16() {
            utf16.extend_from_slice(&unit.to_le_bytes());
        }
        assert_eq!(decode(&utf16), "Hello");
        assert_eq!(decode(b"\xef\xbb\xbfHello"), "Hello");
    }

    #[test]
    fn a_legacy_encoding_is_guessed() {
        let text = "Привет, мир. Это текст в старой кодировке.";
        let (bytes, _, _) = encoding_rs::WINDOWS_1251.encode(text);
        assert_eq!(decode(&bytes), text);
    }
}
