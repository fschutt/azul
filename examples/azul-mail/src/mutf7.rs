//! IMAP mailbox names in "modified UTF-7" (RFC 3501, section 5.1.3), the form a server lists
//! non-ASCII folder names in: `Entw&APw-rfe` is `Entwürfe`. AzMail keeps the server's own name
//! for `SELECT` and shows (and files) the decoded one.

/// The name in UTF-8. A name that is not valid modified UTF-7 is returned as it is, so nothing
/// is lost and the server's name still reaches the folder.
pub fn decode(name: &str) -> String {
    todo!()
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
