//! IMAP mailbox names in "modified UTF-7" (RFC 3501, section 5.1.3), the form a server lists
//! non-ASCII folder names in: `Entw&APw-rfe` is `Entwürfe`. AzMail keeps the server's own name
//! for `SELECT` and shows (and files) the decoded one.

/// The name in UTF-8. A name that is not valid modified UTF-7 is returned as it is, so nothing
/// is lost and the server's name still reaches the folder.
pub fn decode(name: &str) -> String {
    decode_strict(name).unwrap_or_else(|| name.to_string())
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
    fn a_name_that_is_not_modified_utf_7_comes_back_unchanged() {
        for bad in ["&APw", "a&AP*-b", "&2D3-", "&A-"] {
            assert_eq!(decode(bad), bad, "{bad:?}");
        }
    }
}
