//! The message window's To / Cc / Bcc: each entry a token, kept as its address line writes it,
//! and what is still typed goes with the line on Send.

use crate::ui_compose::{address_token, line_with};

#[test]
fn a_typed_recipient_is_kept_as_its_address_line_writes_it() {
    assert_eq!(address_token("ada@example.org").as_deref(), Ok("ada@example.org"));
    assert_eq!(
        address_token(" Ada Lovelace <ada@example.org> ").as_deref(),
        Ok("Ada Lovelace <ada@example.org>")
    );
    // A name with a comma stays quoted: the line reads it back as one entry.
    assert_eq!(
        address_token("\"Lovelace, Ada\" <ada@example.org>").as_deref(),
        Ok("\"Lovelace, Ada\" <ada@example.org>")
    );
    assert_eq!(
        azul_pim::mail_address::split_address_line(&line_with(
            "\"Lovelace, Ada\" <ada@example.org>",
            "ben@example.org"
        ))
        .len(),
        2
    );
    assert!(address_token("not an address").is_err());
    assert!(address_token("Ada <>").is_err());
}

#[test]
fn what_is_still_typed_goes_with_the_line() {
    assert_eq!(line_with("ada@example.org", " ben@example.org "), "ada@example.org, ben@example.org");
    assert_eq!(line_with("", "ben@example.org"), "ben@example.org");
    assert_eq!(line_with("ada@example.org", "  "), "ada@example.org");
}
