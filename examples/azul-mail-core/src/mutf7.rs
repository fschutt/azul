//! IMAP mailbox names in "modified UTF-7" (RFC 3501, section 5.1.3), the form a server lists
//! non-ASCII folder names in: `Entw&APw-rfe` is `Entwürfe`. AzMail keeps the server's own name
//! for `SELECT` and shows (and files) the decoded one.

/// The name in UTF-8. A name that is not valid modified UTF-7 is returned as it is, so nothing
/// is lost and the server's name still reaches the folder.
pub fn decode(name: &str) -> String {
    decode_strict(name).unwrap_or_else(|| name.to_string())
}

/// The UTF-8 name in modified UTF-7, as an IMAP4rev1 server lists it (the Azlin Bridge lists
/// the drive's folders so): printable ASCII stays, `&` is `&-`, every other run of characters
/// is its UTF-16 (big-endian) in modified base64 between `&` and `-`. [`decode`] reads it back.
pub fn encode(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut run: Vec<u16> = Vec::new();
    for c in name.chars() {
        if (' '..='~').contains(&c) {
            if !run.is_empty() {
                encode_run(&run, &mut out);
                run.clear();
            }
            if c == '&' {
                out.push_str("&-");
            } else {
                out.push(c);
            }
        } else {
            let mut units = [0u16; 2];
            run.extend_from_slice(c.encode_utf16(&mut units));
        }
    }
    if !run.is_empty() {
        encode_run(&run, &mut out);
    }
    out
}

/// One shifted run: `&`, the UTF-16 units in modified base64 (`,` for `/`, no padding), `-`.
fn encode_run(units: &[u16], out: &mut String) {
    const ALPHABET: &[u8; 64] =
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+,";
    out.push('&');
    let mut bits: u32 = 0;
    let mut nbits: u32 = 0;
    for unit in units {
        for byte in unit.to_be_bytes() {
            bits = (bits << 8) | u32::from(byte);
            nbits += 8;
            while nbits >= 6 {
                nbits -= 6;
                out.push(char::from(ALPHABET[((bits >> nbits) & 63) as usize]));
            }
            bits &= (1 << nbits) - 1;
        }
    }
    if nbits > 0 {
        out.push(char::from(ALPHABET[((bits << (6 - nbits)) & 63) as usize]));
    }
    out.push('-');
}

/// The decoded name, or `None` when `name` is not valid modified UTF-7.
fn decode_strict(name: &str) -> Option<String> {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        let shifted = &rest[at + 1..];
        let end = shifted.find('-')?;
        let run = &shifted[..end];
        if run.is_empty() {
            out.push('&');
        } else {
            out.push_str(&decode_run(run)?);
        }
        rest = &shifted[end + 1..];
    }
    out.push_str(rest);
    Some(out)
}

/// One shifted run: modified base64 (`,` for `/`, no padding) of UTF-16 big-endian.
fn decode_run(run: &str) -> Option<String> {
    let mut bits: u32 = 0;
    let mut nbits: u32 = 0;
    let mut bytes = Vec::with_capacity(run.len());
    for c in run.bytes() {
        let value = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b',' => 63,
            _ => return None,
        };
        bits = (bits << 6) | u32::from(value);
        nbits += 6;
        if nbits >= 8 {
            nbits -= 8;
            bytes.push((bits >> nbits) as u8);
            bits &= (1 << nbits) - 1;
        }
    }
    // What is left over is padding: fewer than 6 bits, all zero.
    if nbits >= 6 || bits != 0 || bytes.is_empty() || bytes.len() % 2 != 0 {
        return None;
    }
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
        .collect();
    String::from_utf16(&units).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_names_are_left_as_they_are() {
        for name in ["INBOX", "Sent Messages", "[Gmail]/All Mail", "a-b_c.d"] {
            assert_eq!(decode(name), name);
        }
    }

    #[test]
    fn ampersand_minus_is_an_ampersand() {
        assert_eq!(decode("Bills &- Receipts"), "Bills & Receipts");
        assert_eq!(decode("&-"), "&");
    }

    #[test]
    fn shifted_runs_are_utf_16_in_modified_base64() {
        assert_eq!(decode("Entw&APw-rfe"), "Entwürfe");
        assert_eq!(decode("&ZeVnLIqe-"), "日本語");
        // RFC 3501's own examples.
        assert_eq!(
            decode("~peter/mail/&U,BTFw-/&ZeVnLIqe-"),
            "~peter/mail/台北/日本語"
        );
        assert_eq!(decode("&Jjo-!"), "☺!");
        assert_eq!(decode("Gel&APY-schte Elemente"), "Gelöschte Elemente");
    }

    #[test]
    fn utf_8_names_are_encoded_as_a_server_lists_them_and_read_back() {
        assert_eq!(encode("INBOX"), "INBOX");
        assert_eq!(encode("Entwürfe"), "Entw&APw-rfe");
        assert_eq!(encode("Bills & Receipts"), "Bills &- Receipts");
        assert_eq!(encode("Gelöschte Elemente"), "Gel&APY-schte Elemente");
        // RFC 3501's own example.
        assert_eq!(
            encode("~peter/mail/台北/日本語"),
            "~peter/mail/&U,BTFw-/&ZeVnLIqe-"
        );
        assert_eq!(encode("☺!"), "&Jjo-!");
        for name in ["Entwürfe", "R&D/台北", "😀 Fotos", "tab\there", "ü", ""] {
            assert_eq!(decode(&encode(name)), name, "{name:?}");
            assert!(encode(name).is_ascii(), "{name:?}");
        }
    }

    #[test]
    fn a_name_that_is_not_modified_utf_7_comes_back_unchanged() {
        for bad in ["&APw", "a&AP*-b", "&2D3-", "&A-"] {
            assert_eq!(decode(bad), bad, "{bad:?}");
        }
    }
}
