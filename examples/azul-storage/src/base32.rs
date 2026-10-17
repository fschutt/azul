//! RFC 4648 base32 (the alphabet `A`-`Z`, `2`-`7`): what a one-time code's secret is written in
//! (AzKeys), and the codes an Azlin cash checkout puts on paper (azcloud-kit: the activation
//! code, the claim code). Encoded upper case without padding; decoded in any case, with blanks,
//! dashes and `=` padding skipped.

/// The RFC 4648 base32 alphabet.
const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

/// The bytes of RFC 4648 base32 `text`: any case; blanks, dashes and `=` padding skipped. `None`
/// for a character base32 does not have or a length no base32 has.
#[must_use]
pub fn decode(text: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(text.len() * 5 / 8);
    let (mut buf, mut bits, mut count) = (0u32, 0u32, 0usize);
    for c in text.chars() {
        if c.is_whitespace() || c == '-' || c == '=' {
            continue;
        }
        let c = c.to_ascii_uppercase();
        let value = match c {
            'A'..='Z' => c as u32 - 'A' as u32,
            '2'..='7' => c as u32 - '2' as u32 + 26,
            _ => return None,
        };
        buf = (buf << 5) | value;
        bits += 5;
        count += 1;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    // 8 characters are 5 bytes; a last group of 1, 3 or 6 characters is no whole byte count.
    if !matches!(count % 8, 0 | 2 | 4 | 5 | 7) {
        return None;
    }
    Some(out)
}

/// `bytes` as RFC 4648 base32, upper case, without padding (as otpauth URLs write secrets).
#[must_use]
pub fn encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(5) * 8);
    let (mut buf, mut bits) = (0u32, 0u32);
    for &b in bytes {
        buf = (buf << 8) | u32::from(b);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            out.push(char::from(ALPHABET[((buf >> bits) & 31) as usize]));
        }
        buf &= (1 << bits) - 1;
    }
    if bits > 0 {
        out.push(char::from(ALPHABET[((buf << (5 - bits)) & 31) as usize]));
    }
    out
}

/// `text` in blocks of four characters joined by `-` (`ABCDEFGHIJ` -> `ABCD-EFGH-IJ`): how a
/// code is written on paper.
#[must_use]
pub fn grouped(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(4)
        .map(|block| block.iter().collect::<String>())
        .collect::<Vec<String>>()
        .join("-")
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, grouped};

    #[test]
    fn base32_matches_the_rfc_4648_vectors_both_ways() {
        for (plain, coded) in [
            ("", ""),
            ("f", "MY"),
            ("fo", "MZXQ"),
            ("foo", "MZXW6"),
            ("foob", "MZXW6YQ"),
            ("fooba", "MZXW6YTB"),
            ("foobar", "MZXW6YTBOI"),
        ] {
            assert_eq!(encode(plain.as_bytes()), coded);
            assert_eq!(decode(coded).as_deref(), Some(plain.as_bytes()));
        }
        assert_eq!(decode("mzxw 6ytb-oi").as_deref(), Some(&b"foobar"[..]));
        assert_eq!(decode("MZXW6YQ=").as_deref(), Some(&b"foob"[..]));
        assert_eq!(decode("MZXW1"), None, "1 is no base32 digit");
        assert_eq!(decode("M"), None, "one character is no byte");
    }

    #[test]
    fn a_code_on_paper_is_written_in_blocks_of_four() {
        assert_eq!(grouped("ABCDEFGHIJ"), "ABCD-EFGH-IJ");
        assert_eq!(grouped("ABCD"), "ABCD");
        assert_eq!(grouped(""), "");
    }
}
