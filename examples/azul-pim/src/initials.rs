//! An avatar's initials. AzContacts' book.rs, AzMail's reading pane and AzMeet each worked them
//! out (scripts/DEDUP_EDITORS_2026_10_02.md, B24); this is AzContacts' rule, the most complete.

/// The upper-case first character of `word` (`ß` would be `SS`, as `char::to_uppercase` says).
fn first_upper(word: &str) -> Option<String> {
    word.trim()
        .chars()
        .next()
        .map(|c| c.to_uppercase().collect::<String>())
}

/// Up to two initials of a name: the first characters of its first two words ("Ada Lovelace"
/// is `AL`, "Northwind GmbH" `NG`), of its only word ("Madonna" is `M`, "王芳" `王`); `?` for a
/// name without words.
#[must_use]
pub fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.as_slice() {
        [] => String::from("?"),
        [one] => first_upper(one).unwrap_or_else(|| String::from("?")),
        [a, b, ..] => format!(
            "{}{}",
            first_upper(a).unwrap_or_default(),
            first_upper(b).unwrap_or_default()
        ),
    }
}

/// A person's initials: given and family name (`RW`), or the one of them there is, else the
/// [`initials`] of `display` (a company, a nickname, an address).
#[must_use]
pub fn person_initials(given: &str, family: &str, display: &str) -> String {
    match (first_upper(given), first_upper(family)) {
        (Some(g), Some(f)) => format!("{g}{f}"),
        (Some(one), None) | (None, Some(one)) => one,
        (None, None) => initials(display),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn initials_for_people_companies_and_single_names() {
        // AzContacts' book.rs cases.
        assert_eq!(person_initials("Anna", "Berg", "Anna Berg"), "AB");
        assert_eq!(person_initials("\u{141}ukasz", "Nowak", ""), "\u{141}N");
        assert_eq!(person_initials("", "", "\u{738b}\u{82b3}"), "\u{738b}");
        assert_eq!(person_initials("", "", "Northwind GmbH"), "NG");
        assert_eq!(person_initials("Madonna", "", "Madonna"), "M");
        assert_eq!(person_initials(" ", "Weber", ""), "W");
        assert_eq!(
            person_initials("", "", ""),
            "?",
            "nothing to take initials from"
        );
    }

    #[test]
    fn initials_of_a_display_name_take_its_first_two_words() {
        // AzMail's reading pane: the sender's name, or the address when there is none.
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("  ada   king lovelace "), "AK");
        assert_eq!(initials("ada@example.org"), "A");
        assert_eq!(initials("élodie durand"), "ÉD");
        assert_eq!(initials(""), "?");
        assert_eq!(initials("   "), "?");
    }
}
