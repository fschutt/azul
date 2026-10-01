//! Possible duplicates, and merging two contacts into one.
//!
//! Two contacts are probably one person when they share an email address,
//! a phone number (the last nine digits, so `+49 151 0000 0004` and
//! `0151 00000004` agree) or the same name (case, diacritics and a
//! parenthesised note such as "(imported)" ignored). Scores: the same name
//! 0.93 (0.9 with a note), the same email 0.95, the same phone 0.9, a name
//! and an email or phone 0.99. A merge keeps the
//! first contact's file (its UID), takes each single field from the side
//! the user picks (or from the other side when the picked one is empty) and
//! keeps BOTH sides of the multi-valued fields - phones, emails, addresses,
//! web pages, groups, custom fields - without repeating a value.

use crate::book::fold;
use crate::contact::{Address, Contact, Labeled};

/// An email as compared: trimmed, lower-case.
#[must_use]
pub fn normalize_email(email: &str) -> String {
        todo!("RED: normalize_email")
    }

/// A phone number as compared: its last nine digits (country code and
/// trunk prefix drop out); `None` for fewer than seven digits.
#[must_use]
pub fn normalize_phone(phone: &str) -> Option<String> {
        todo!("RED: normalize_phone")
    }

/// A name as compared: folded words without a parenthesised note; empty
/// when the contact has no name.
#[must_use]
pub fn name_key(c: &Contact) -> String {
        todo!("RED: name_key")
    }

/// How alike two contacts are (0..=1) and why.
#[must_use]
pub fn similarity(a: &Contact, b: &Contact) -> (f32, Vec<String>) {
        todo!("RED: similarity")
    }

/// A pair of possible duplicates.
#[derive(Clone, Debug, PartialEq)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub score: f32,
    pub reasons: Vec<String>,
}

/// Every pair at or above `threshold`, best first; pairs the user marked
/// "not a duplicate" (`ignored`, by UID, either order) are left out.
#[must_use]
pub fn find_duplicates(contacts: &[Contact], threshold: f32, ignored: &[(String, String)]) -> Vec<Pair> {
        todo!("RED: find_duplicates")
    }

/// The default threshold of the duplicates finder.
pub const THRESHOLD: f32 = 0.9;

/// Which side a single field comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Pick {
    #[default]
    A,
    B,
}

/// The user's choices on the merge screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct MergePlan {
    pub name: Pick,
    pub company: Pick,
    pub birthday: Pick,
    pub photo: Pick,
    pub notes: Pick,
    /// Keep both notes (joined), whatever `notes` says.
    pub notes_both: bool,
}

fn pick<'a>(p: Pick, a: &'a str, b: &'a str) -> &'a str {
    let (first, second) = match p {
        Pick::A => (a, b),
        Pick::B => (b, a),
    };
    if first.trim().is_empty() {
        second
    } else {
        first
    }
}

fn union_labeled(a: &[Labeled], b: &[Labeled], key: impl Fn(&str) -> String) -> Vec<Labeled> {
    let mut out: Vec<Labeled> = Vec::new();
    for item in a.iter().chain(b) {
        let k = key(&item.value);
        if k.is_empty() || out.iter().any(|o| key(&o.value) == k) {
            continue;
        }
        out.push(item.clone());
    }
    out
}

/// The merged contact (it keeps `a`'s UID, so `a`'s file is rewritten and
/// `b`'s deleted).
#[must_use]
pub fn merge(a: &Contact, b: &Contact, plan: &MergePlan) -> Contact {
        todo!("RED: merge")
    }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::Birthday;

    fn anna() -> Contact {
        Contact {
            uid: "a".into(),
            given: "Anna".into(),
            family: "Berg".into(),
            org: "Northwind".into(),
            department: "Finance".into(),
            emails: vec![Labeled::new("work", "anna@example.org")],
            phones: vec![Labeled::new("mobile", "+49 151 0000 0004")],
            photo: "data:image/png;base64,AAAA".into(),
            groups: vec!["Work".into()],
            ..Contact::default()
        }
    }

    fn anna_imported() -> Contact {
        Contact {
            uid: "b".into(),
            given: "Anna".into(),
            family: "Berg (imported)".into(),
            emails: vec![Labeled::new("home", "anna.berg@example.net")],
            phones: vec![Labeled::new("mobile", "+49 151 0000 0014")],
            birthday: Some(Birthday { year: None, month: 5, day: 2 }),
            notes: "Met at the fair.".into(),
            groups: vec!["Book club".into()],
            ..Contact::default()
        }
    }

    #[test]
    fn phones_compare_by_their_last_nine_digits() {
        assert_eq!(normalize_phone("+49 151 0000 0004"), normalize_phone("0151 00000004"));
        assert_eq!(normalize_phone("0049 (151) 0000-0004"), normalize_phone("+49 151 0000 0004"));
        assert_ne!(normalize_phone("+49 151 0000 0004"), normalize_phone("+49 151 0000 0014"));
        assert_eq!(normalize_phone("112"), None, "too short to say");
    }

    #[test]
    fn a_name_with_a_note_matches_the_plain_name() {
        assert_eq!(name_key(&anna()), "anna berg");
        assert_eq!(name_key(&anna_imported()), "anna berg");
        let (score, reasons) = similarity(&anna(), &anna_imported());
        assert!((score - 0.9).abs() < 1e-6, "{score}");
        assert_eq!(reasons, vec!["same name"]);
        let mut same = anna_imported();
        same.family = "Berg".into();
        same.phones.clear();
        same.emails.clear();
        assert!((similarity(&anna(), &same).0 - 0.93).abs() < 1e-6, "exactly the same name");
    }

    #[test]
    fn a_shared_email_or_phone_makes_a_duplicate() {
        let mut other = Contact {
            uid: "c".into(),
            given: "A.".into(),
            family: "Berg".into(),
            emails: vec![Labeled::new("home", " ANNA@example.org ")],
            ..Contact::default()
        };
        let (score, reasons) = similarity(&anna(), &other);
        assert!((score - 0.95).abs() < 1e-6);
        assert_eq!(reasons, vec!["same email anna@example.org"]);
        other.emails.clear();
        other.phones = vec![Labeled::new("work", "0151 00000004")];
        assert_eq!(similarity(&anna(), &other).1, vec!["same phone number"]);
        let mut twin = anna();
        twin.uid = "d".into();
        assert!((similarity(&anna(), &twin).0 - 0.99).abs() < 1e-6, "name and email: almost certain");
    }

    #[test]
    fn strangers_and_nameless_cards_are_not_duplicates() {
        let stranger = Contact {
            uid: "s".into(),
            given: "Ben".into(),
            family: "Krüger".into(),
            ..Contact::default()
        };
        assert_eq!(similarity(&anna(), &stranger).0, 0.0);
        let empty1 = Contact { uid: "e1".into(), ..Contact::default() };
        let empty2 = Contact { uid: "e2".into(), ..Contact::default() };
        assert_eq!(similarity(&empty1, &empty2).0, 0.0);
    }

    #[test]
    fn the_finder_lists_pairs_best_first_and_skips_ignored_ones() {
        let mut twin = anna();
        twin.uid = "d".into();
        let book = vec![anna(), anna_imported(), twin];
        let pairs = find_duplicates(&book, THRESHOLD, &[]);
        assert_eq!(pairs.iter().map(|p| (p.a, p.b)).collect::<Vec<_>>(), vec![(0, 2), (0, 1), (1, 2)]);
        let ignored = vec![("d".to_string(), "a".to_string())];
        let pairs = find_duplicates(&book, THRESHOLD, &ignored);
        assert_eq!(pairs.iter().map(|p| (p.a, p.b)).collect::<Vec<_>>(), vec![(0, 1), (1, 2)]);
    }

    #[test]
    fn a_merge_keeps_both_sides_of_the_lists_and_the_picked_single_fields() {
        let plan = MergePlan {
            notes_both: true,
            ..MergePlan::default()
        };
        let m = merge(&anna(), &anna_imported(), &plan);
        assert_eq!(m.uid, "a", "the first contact's file stays");
        assert_eq!(m.display_name(), "Anna Berg");
        assert_eq!(m.org, "Northwind");
        assert_eq!(
            m.emails.iter().map(|e| e.value.as_str()).collect::<Vec<_>>(),
            vec!["anna@example.org", "anna.berg@example.net"]
        );
        assert_eq!(m.phones.len(), 2);
        assert_eq!(m.birthday, anna_imported().birthday, "A had none: B's");
        assert_eq!(m.notes, "Met at the fair.");
        assert_eq!(m.photo, anna().photo);
        assert_eq!(m.groups, vec!["Work", "Book club"]);
    }

    #[test]
    fn picking_b_takes_bs_name_and_a_same_number_is_kept_once() {
        let mut b = anna_imported();
        b.phones = vec![Labeled::new("mobile", "0151 00000004")];
        let plan = MergePlan {
            name: Pick::B,
            photo: Pick::B,
            ..MergePlan::default()
        };
        let m = merge(&anna(), &b, &plan);
        assert_eq!(m.family, "Berg (imported)");
        assert_eq!(m.phones.len(), 1, "the same number in two spellings is one");
        assert_eq!(m.photo, anna().photo, "B has no photo: A's");
    }
}
