//! Archives for the tests: built in memory and signed the way
//! `zipsign sign tar` does, so no network and no tool is needed.

use std::error::Error;

use self_update::zipsign_api::sign::gather_signature_data;
use self_update::zipsign_api::{Prehash, SigningKey};

/// The signing key made of one repeated byte.
pub(super) fn signing_key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

/// The public half of [`signing_key`], as the application embeds it.
pub(super) fn public_key(seed: u8) -> [u8; 32] {
    signing_key(seed).verifying_key().to_bytes()
}

/// One entry of a tar: a file with a mode.
pub(super) struct Entry<'a> {
    pub(super) path: &'a str,
    pub(super) mode: u32,
    pub(super) data: &'a [u8],
}

fn crc32(data: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in data {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

fn tar(entries: &[Entry<'_>]) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        let mut header = [0_u8; 512];
        header[..entry.path.len()].copy_from_slice(entry.path.as_bytes());
        let fields = [
            (100, 8, format!("{:07o}\0", entry.mode)),
            (108, 8, "0000000\0".to_owned()),
            (116, 8, "0000000\0".to_owned()),
            (124, 12, format!("{:011o}\0", entry.data.len())),
            (136, 12, "00000000000\0".to_owned()),
        ];
        for (at, len, text) in fields {
            header[at..at + len].copy_from_slice(text.as_bytes());
        }
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        header[148..156].copy_from_slice(b"        ");
        let sum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        out.extend_from_slice(&header);
        out.extend_from_slice(entry.data);
        out.resize(out.len().next_multiple_of(512), 0);
    }
    out.resize(out.len() + 1024, 0);
    out
}

/// A valid `.tar.gz` of the entries, in one stored deflate block.
pub(super) fn tar_gz(entries: &[Entry<'_>]) -> Vec<u8> {
    let tar = tar(entries);
    let len = u16::try_from(tar.len()).expect("a test archive fits one stored block");
    let mut out = vec![0x1f, 0x8b, 8, 0, 0, 0, 0, 0, 0, 0xff, 1];
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&(!len).to_le_bytes());
    out.extend_from_slice(&tar);
    out.extend_from_slice(&crc32(&tar).to_le_bytes());
    out.extend_from_slice(&u32::try_from(tar.len()).unwrap_or(0).to_le_bytes());
    out
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0_u32, |n, (i, byte)| n | u32::from(*byte) << (16 - 8 * i));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Signs a `.tar.gz` as `zipsign sign tar` does: the signature goes into a
/// gzip member appended to the file, and is salted with `context`, the file
/// name of the archive.
pub(super) fn sign(
    unsigned: &[u8],
    key: &SigningKey,
    context: &str,
) -> Result<Vec<u8>, Box<dyn Error>> {
    let prehash = Prehash::calculate(&mut &*unsigned)?;
    let block = gather_signature_data(
        std::slice::from_ref(key),
        &prehash,
        Some(context.as_bytes()),
    )?;
    let mut out = unsigned.to_vec();
    out.extend_from_slice(&[0x1f, 0x8b, 8, 0x10]);
    out.extend_from_slice(&978_307_200_u32.to_le_bytes());
    out.extend_from_slice(&[0, 0xff]);
    out.extend_from_slice(base64(&block).as_bytes());
    out.extend_from_slice(format!("{:016x}", unsigned.len()).as_bytes());
    out.extend_from_slice(&[0, 1, 0, 0, 0xff, 0xff, 0, 0, 0, 0, 0, 0, 0, 0]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_standard_alphabet_with_padding() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn crc32_matches_the_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }
}
