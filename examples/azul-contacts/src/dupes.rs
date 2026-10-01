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
    email.trim().to_lowercase()
}

/// A phone number as compared: its last nine digits (country code and
/// trunk prefix drop out); `None` for fewer than seven digits.
#[must_use]
pub fn normalize_phone(phone: &str) -> Option<String> {
    let digits: String = phone.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 7 {
        return None;
    }
    let start = digits.len().saturating_sub(9);
    Some(digits[start..].to_string())
}

/// A name as compared: folded words without a parenthesised note; empty
/// when the contact has no name.
#[must_use]
pub fn name_key(c: &Contact) -> String {
    let name = if c.composed_name().is_empty() {
        if c.formatted.trim().is_empty() {
            c.org.clone()
        } else {
            c.formatted.clone()
        }
    } else {
        c.composed_name()
    };
    let mut out = String::new();
    let mut depth = 0usize;
    for ch in name.chars() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            other if depth == 0 => out.push(other),
            _ => {}
        }
    }
    fold(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// How alike two contacts are (0..=1) and why.
#[must_use]
pub fn similarity(a: &Contact, b: &Contact) -> (f32, Vec<String>) {
    let mut score: f32 = 0.0;
    let mut reasons = Vec::new();
    let (na, nb) = (name_key(a), name_key(b));
    let same_name = !na.is_empty() && na == nb;
    if same_name {
        // Exactly the same name, or the same only after dropping a note such as "(imported)".
        let exact = fold(&a.display_name()) == fold(&b.display_name());
        score = score.max(if exact { 0.93 } else { 0.9 });
        reasons.push("same name".to_string());
    }
    if let Some(e) = a
        .emails
        .iter()
        .map(|e| normalize_email(&e.value))
        .filter(|e| !e.is_empty())
        .find(|e| b.emails.iter().any(|f| normalize_email(&f.value) == *e))
    {
        score = score.max(0.95);
        reasons.push(format!("same email {e}"));
    }
    let phones_b: Vec<String> = b.phones.iter().filter_map(|p| normalize_phone(&p.value)).collect();
    if a
        .phones
        .iter()
        .filter_map(|p| normalize_phone(&p.value))
        .any(|p| phones_b.contains(&p))
    {
        score = score.max(0.9);
        reasons.push("same phone number".to_string());
    }
    if same_name && reasons.len() > 1 {
        score = 0.99;
    }
    (score, reasons)
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
    let mut out = Vec::new();
    for i in 0..contacts.len() {
        for j in i + 1..contacts.len() {
            let (a, b) = (&contacts[i], &contacts[j]);
            if ignored
                .iter()
                .any(|(x, y)| (x == &a.uid && y == &b.uid) || (x == &b.uid && y == &a.uid))
            {
                continue;
            }
            let (score, reasons) = similarity(a, b);
            if score >= threshold {
                out.push(Pair { a: i, b: j, score, reasons });
            }
        }
    }
    out.sort_by(|x, y| y.score.partial_cmp(&x.score).unwrap_or(std::cmp::Ordering::Equal).then(x.a.cmp(&y.a)));
    out
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
    let name_from = match plan.name {
        Pick::B if !name_key(b).is_empty() => b,
        _ if name_key(a).is_empty() => b,
        _ => a,
    };
    let company_from = match plan.company {
        Pick::B if !(b.org.trim().is_empty() && b.title.trim().is_empty()) => b,
        _ if a.org.trim().is_empty() && a.title.trim().is_empty() => b,
        _ => a,
    };
    let birthday = match plan.birthday {
        Pick::A => a.birthday.or(b.birthday),
        Pick::B => b.birthday.or(a.birthday),
    };
    let notes = if plan.notes_both {
        let (x, y) = (a.notes.trim(), b.notes.trim());
        match (x.is_empty(), y.is_empty()) {
            (false, false) if x != y => format!("{x}\n{y}"),
            (false, _) => x.to_string(),
            _ => y.to_string(),
        }
    } else {
        pick(plan.notes, &a.notes, &b.notes).to_string()
    };
    let mut addresses: Vec<Address> = Vec::new();
    for addr in a.addresses.iter().chain(&b.addresses) {
        let k = fold(&addr.lines().join(" "));
        if !k.is_empty() && !addresses.iter().any(|x| fold(&x.lines().join(" ")) == k) {
            addresses.push(addr.clone());
        }
    }
    let mut groups = a.groups.clone();
    for g in &b.groups {
        if !groups.contains(g) {
            groups.push(g.clone());
        }
    }
    let mut extra = a.extra.clone();
    for p in &b.extra {
        if !extra.iter().any(|x| x.to_line() == p.to_line()) {
            extra.push(p.clone());
        }
    }
    Contact {
        uid: a.uid.clone(),
        prefix: name_from.prefix.clone(),
        given: name_from.given.clone(),
        additional: name_from.additional.clone(),
        family: name_from.family.clone(),
        suffix: name_from.suffix.clone(),
        formatted: name_from.formatted.clone(),
        nickname: pick(plan.name, &a.nickname, &b.nickname).to_string(),
        org: company_from.org.clone(),
        department: company_from.department.clone(),
        title: company_from.title.clone(),
        phones: union_labeled(&a.phones, &b.phones, |v| normalize_phone(v).unwrap_or_else(|| v.trim().to_string())),
        emails: union_labeled(&a.emails, &b.emails, normalize_email),
        addresses,
        urls: union_labeled(&a.urls, &b.urls, |v| v.trim().trim_end_matches('/').to_lowercase()),
        birthday,
        notes,
        photo: pick(plan.photo, &a.photo, &b.photo).to_string(),
        groups,
        favorite: a.favorite || b.favorite,
        custom: {
            let mut custom = a.custom.clone();
            for f in &b.custom {
                if !custom.contains(f) {
                    custom.push(f.clone());
                }
            }
            custom
        },
        extra,
    }
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
