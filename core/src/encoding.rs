//! Telling a file's encoding from its bytes, and turning text back into those bytes.
//! encoding_rs holds the tables (the WHATWG Encoding Standard's) and chardetng guesses.
use crate::doc::{split_eol, Eol};

/// Most bytes looked at to guess a legacy encoding; the whole file is still decoded.
const SNIFF_BYTES: usize = 1 << 20;

/// How a file's bytes become the editor's text and back.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Encoding {
    pub encoding: &'static encoding_rs::Encoding,
    /// Whether the file starts with a byte-order mark, which saving keeps.
    pub bom: bool,
}

impl Default for Encoding {
    fn default() -> Self {
        Encoding::UTF8
    }
}

impl Encoding {
    pub const UTF8: Encoding = Encoding { encoding: encoding_rs::UTF_8, bom: false };

    /// The encoding's name in the WHATWG Encoding Standard ("UTF-8", "Shift_JIS", "windows-1252").
    pub fn name(self) -> &'static str {
        self.encoding.name()
    }

    /// `text` in this encoding, byte-order mark first when the file had one, or `None` when the
    /// encoding has no bytes for some of its characters (see [`Encoding::unencodable`]).
    pub fn encode(self, text: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(text.len() + 3);
        let e = self.encoding;
        if e == encoding_rs::UTF_16LE || e == encoding_rs::UTF_16BE {
            // encoding_rs only writes UTF-8 for these, as browsers do, so the units are written here.
            let le = e == encoding_rs::UTF_16LE;
            let unit = |u: u16| if le { u.to_le_bytes() } else { u.to_be_bytes() };
            if self.bom {
                out.extend(unit(0xFEFF));
            }
            out.extend(text.encode_utf16().flat_map(unit));
            return Some(out);
        }
        if self.bom && e == encoding_rs::UTF_8 {
            out.extend([0xEF, 0xBB, 0xBF]);
        }
        let (bytes, _, unencodable) = e.encode(text);
        if unencodable {
            return None;
        }
        out.extend_from_slice(&bytes);
        Some(out)
    }

    /// The first `limit` characters of `text` this encoding can't write, with their byte offsets.
    pub fn unencodable(self, text: &str, limit: usize) -> Vec<(usize, char)> {
        if self.encode(text).is_some() {
            return vec![];
        }
        // ponytail: encodes character by character, only after a whole-text encode failed.
        let mut buf = [0; 4];
        text.char_indices().filter(|&(_, c)| self.encode(c.encode_utf8(&mut buf)).is_none()).take(limit).collect()
    }
}

/// Why a document opened read-only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReadOnly {
    /// The file has NUL bytes or a binary format's signature.
    Binary,
    /// The detected encoding can't read the file in a way that writes the same bytes back.
    Undecodable,
}

/// A file's bytes, read: the editor's text (LF line ends), how to write it back, and whether it
/// may be written back at all.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub eol: Eol,
    /// Whether the file mixed line-end styles; saving writes `eol` throughout.
    pub mixed: bool,
    pub encoding: Encoding,
    pub read_only: Option<ReadOnly>,
}

/// Reads `bytes` in their own encoding: a byte-order mark first (UTF-8, UTF-16 LE or BE), then
/// UTF-8 when they are valid UTF-8, then the legacy encoding they look like (chardetng, Firefox's
/// detector, ISO-2022-JP included). Bytes the encoding can't turn into text and back unchanged
/// open read-only, as do files with NUL bytes and files craft-sniff names (images, archives…).
pub fn decode(bytes: &[u8]) -> Decoded {
    let (encoding, body) = match encoding_rs::Encoding::for_bom(bytes) {
        Some((e, n)) => (Encoding { encoding: e, bom: true }, &bytes[n..]),
        None => {
            // ponytail: an ASCII PLY model is the one text file craft-sniff names; it opens
            // read-only until E07-S4-T1 can reopen it as text.
            if bytes.contains(&0) || craft_sniff::identify(bytes).is_some() {
                let (text, eol, mixed) = split_eol(&String::from_utf8_lossy(bytes));
                return Decoded { text, eol, mixed, encoding: Encoding::UTF8, read_only: Some(ReadOnly::Binary) };
            }
            (Encoding { encoding: guess(bytes), bom: false }, bytes)
        }
    };
    // Some encodings read two byte sequences as one character (Shift_JIS's duplicate NEC and IBM
    // rows, ISO-2022-JP's redundant escapes), so only writing the text back proves it is the file.
    let exact = |e: Encoding| {
        let (text, malformed) = e.encoding.decode_without_bom_handling(body);
        (!malformed && e.encode(&text).as_deref() == Some(bytes)).then_some(text)
    };
    let mut encoding = encoding;
    let mut text = exact(encoding);
    if text.is_none() && encoding.encoding == encoding_rs::GBK {
        // chardetng says GBK for GB18030 too: same reading, but GB18030 writes four-byte sequences.
        encoding.encoding = encoding_rs::GB18030;
        text = exact(encoding);
    }
    let read_only = text.is_none().then_some(ReadOnly::Undecodable);
    let (text, eol, mixed) = split_eol(&text.unwrap_or_else(|| encoding.encoding.decode_without_bom_handling(body).0));
    Decoded { text, eol, mixed, encoding, read_only }
}

/// The encoding of bytes with no byte-order mark or NUL byte.
fn guess(bytes: &[u8]) -> &'static encoding_rs::Encoding {
    let utf8 = std::str::from_utf8(bytes).is_ok();
    // ISO-2022-JP is seven-bit, so valid UTF-8 too; its escapes start with ESC.
    if utf8 && !bytes.contains(&0x1B) {
        return encoding_rs::UTF_8;
    }
    let mut detector = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Allow);
    detector.feed(bytes.get(..SNIFF_BYTES).unwrap_or(bytes), bytes.len() <= SNIFF_BYTES);
    match detector.guess(None, chardetng::Utf8Detection::Deny) {
        e if e == encoding_rs::ISO_2022_JP || !utf8 => e,
        _ => encoding_rs::UTF_8,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unencodable_characters_are_found_where_they_are() {
        let latin1 = Encoding { encoding: encoding_rs::WINDOWS_1252, bom: false };
        assert_eq!(latin1.unencodable("café ok", 5), []);
        assert_eq!(latin1.unencodable("a€ b日 c本", 5), [(6, '日'), (11, '本')], "€ is in windows-1252");
        assert_eq!(latin1.unencodable("日本語", 2).len(), 2, "stops at the limit");
        assert!(latin1.encode("日").is_none(), "never writes character references");
    }

    #[test]
    fn files_that_cant_be_written_back_open_read_only() {
        let undecodable: [&[u8]; 2] = [b"\xEF\xBB\xBFUTF-8 \xFF", b"\xFF\xFEo\0k\0!"];
        for bytes in undecodable {
            assert_eq!(decode(bytes).read_only, Some(ReadOnly::Undecodable), "{bytes:?}");
        }
        let binary: [&[u8]; 2] = [b"text\0more text", b"\x89PNG\r\n\x1a\nIHDR"];
        for bytes in binary {
            assert_eq!(decode(bytes).read_only, Some(ReadOnly::Binary), "{bytes:?}");
        }
    }
}
