//! The address book's views: sorting by first or last name, the letter
//! sections and the A-Z jump bar, initials for the avatars, search, groups.
//!
//! Sorting and search ignore case and the common Latin diacritics (Krüger
//! sorts and is found as "kruger", Łukasz as "lukasz", Straße as "strasse");
//! names that do not start with a Latin letter (王芳, an Arabic name, a
//! number) are listed under `#` after Z.

use crate::contact::Contact;

/// The list's order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SortBy {
    #[default]
    First,
    Last,
}

impl SortBy {
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            SortBy::First => "first",
            SortBy::Last => "last",
        }
    }
}

/// What the list shows.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum Filter {
    #[default]
    All,
    Favorites,
    Group(String),
}

/// The jump bar's letters: A..Z and `#`.
pub const ALPHABET: [char; 27] = [
    'A', 'B', 'C', 'D', 'E', 'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V',
    'W', 'X', 'Y', 'Z', '#',
];

/// Lower-case without the common Latin diacritics: `Krüger` -> `kruger`.
#[must_use]
pub fn fold(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        let mapped: &str = match c {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
            'æ' => "ae",
            'ç' | 'ć' | 'č' | 'ĉ' | 'ċ' => "c",
            'ď' | 'đ' | 'ð' => "d",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ė' | 'ę' | 'ě' => "e",
            'ğ' | 'ģ' => "g",
            'ì' | 'í' | 'î' | 'ï' | 'ī' | 'į' | 'ı' => "i",
            'ķ' => "k",
            'ł' | 'ľ' | 'ļ' | 'ĺ' => "l",
            'ñ' | 'ń' | 'ň' | 'ņ' => "n",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ő' => "o",
            'œ' => "oe",
            'ŕ' | 'ř' => "r",
            'ś' | 'š' | 'ş' | 'ș' => "s",
            'ß' => "ss",
            'ť' | 'ţ' | 'ț' => "t",
            'ù' | 'ú' | 'û' | 'ü' | 'ū' | 'ů' | 'ű' | 'ų' => "u",
            'ý' | 'ÿ' => "y",
            'ź' | 'ż' | 'ž' => "z",
            'þ' => "th",
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(mapped);
    }
    out
}

/// The sort key: first name first or last name first; a company by its name.
#[must_use]
pub fn sort_key(c: &Contact, by: SortBy) -> String {
    let (given, family) = (c.given.trim(), c.family.trim());
    let name = if given.is_empty() && family.is_empty() {
        c.display_name()
    } else {
        match by {
            SortBy::First => format!("{given} {family}"),
            SortBy::Last => format!("{family} {given}"),
        }
    };
    fold(name.trim())
}

/// The letter section a contact is listed under: A..Z, or `#`.
#[must_use]
pub fn letter(c: &Contact, by: SortBy) -> char {
    match sort_key(c, by).chars().next() {
        Some(ch) if ch.is_ascii_alphabetic() => ch.to_ascii_uppercase(),
        _ => '#',
    }
}

/// The avatar's initials: first and last name (`RW`), a company's first two
/// words (`NG`), a single name's first letter (`M`), or the display name's
/// first character (`王`).
#[must_use]
pub fn initials(c: &Contact) -> String {
    let first = |s: &str| s.trim().chars().next().map(|ch| ch.to_uppercase().collect::<String>());
    match (first(&c.given), first(&c.family)) {
        (Some(g), Some(f)) => return format!("{g}{f}"),
        (Some(g), None) => return g,
        (None, Some(f)) => return f,
        (None, None) => {}
    }
    let name = c.display_name();
    if name == "(no name)" {
        return "?".to_string();
    }
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.as_slice() {
        [] => "?".to_string(),
        [one] => first(one).unwrap_or_default(),
        [a, b, ..] => format!("{}{}", first(a).unwrap_or_default(), first(b).unwrap_or_default()),
    }
}

fn digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}

/// Whether a contact matches a search: every word of the query is found
/// (any case, diacritics ignored) in the name, nickname, company, title,
/// emails, groups, notes or custom fields - or, for a word of two or more
/// digits (`+49`, `0004`), in a phone number's digits.
#[must_use]
pub fn matches(c: &Contact, query: &str) -> bool {
    let words: Vec<String> = query.split_whitespace().map(fold).collect();
    if words.is_empty() {
        return true;
    }
    let mut hay = vec![
        c.display_name(),
        c.composed_name(),
        c.nickname.clone(),
        c.org.clone(),
        c.department.clone(),
        c.title.clone(),
        c.notes.clone(),
    ];
    hay.extend(c.emails.iter().map(|e| e.value.clone()));
    hay.extend(c.groups.iter().cloned());
    hay.extend(c.custom.iter().map(|f| f.value.clone()));
    let hay = fold(&hay.join(" "));
    let phone_digits: Vec<String> = c.phones.iter().map(|p| digits(&p.value)).collect();
    words.iter().all(|w| {
        if hay.contains(w.as_str()) {
            return true;
        }
        let d = digits(w);
        d.len() >= 2 && d.len() == w.chars().filter(|ch| !matches!(ch, '+' | '-' | ' ' | '(' | ')' | '/')).count()
            && phone_digits.iter().any(|p| p.contains(&d))
    })
}

/// Whether a contact passes the filter.
#[must_use]
pub fn in_filter(c: &Contact, filter: &Filter) -> bool {
    match filter {
        Filter::All => true,
        Filter::Favorites => c.favorite,
        Filter::Group(g) => c.groups.iter().any(|x| x == g),
    }
}

/// The list: the indices of the contacts that pass the filter and the
/// search, in order (`#` after Z, then by sort key, then by display name).
#[must_use]
pub fn view(contacts: &[Contact], by: SortBy, filter: &Filter, query: &str) -> Vec<usize> {
    let mut out: Vec<usize> = (0..contacts.len())
        .filter(|&i| in_filter(&contacts[i], filter) && matches(&contacts[i], query))
        .collect();
    out.sort_by_cached_key(|&i| {
        let c = &contacts[i];
        (letter(c, by) == '#', sort_key(c, by), c.display_name(), c.uid.clone())
    });
    out
}

/// The list's letter sections: `(letter, indices)` in list order.
#[must_use]
pub fn sections(contacts: &[Contact], indices: &[usize], by: SortBy) -> Vec<(char, Vec<usize>)> {
    let mut out: Vec<(char, Vec<usize>)> = Vec::new();
    for &i in indices {
        let l = letter(&contacts[i], by);
        match out.last_mut() {
            Some((last, items)) if *last == l => items.push(i),
            _ => out.push((l, vec![i])),
        }
    }
    out
}

/// Where a click on the jump bar's `wanted` goes: that letter's section, or
/// the next one that has contacts, or the last section.
#[must_use]
pub fn jump_target(present: &[char], wanted: char) -> Option<char> {
    let rank = |c: char| ALPHABET.iter().position(|a| *a == c).unwrap_or(ALPHABET.len());
    present
        .iter()
        .copied()
        .filter(|c| rank(*c) >= rank(wanted))
        .min_by_key(|c| rank(*c))
        .or_else(|| present.iter().copied().max_by_key(|c| rank(*c)))
}

/// Every group with its member count, by name.
#[must_use]
pub fn group_counts(contacts: &[Contact]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for c in contacts {
        for g in &c.groups {
            match out.iter_mut().find(|(name, _)| name == g) {
                Some((_, n)) => *n += 1,
                None => out.push((g.clone(), 1)),
            }
        }
    }
    out.sort_by_key(|(name, _)| fold(name));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contact::Labeled;

    fn person(given: &str, family: &str) -> Contact {
        Contact {
            uid: format!("{given}-{family}"),
            given: given.to_string(),
            family: family.to_string(),
            ..Contact::default()
        }
    }

    fn names(contacts: &[Contact], indices: &[usize]) -> Vec<String> {
        indices.iter().map(|&i| contacts[i].display_name()).collect()
    }

    fn book() -> Vec<Contact> {
        let mut anna = person("Anna", "Berg");
        anna.org = "Northwind".into();
        anna.department = "Finance".into();
        anna.phones = vec![Labeled::new("mobile", "+49 151 0000 0004")];
        anna.groups = vec!["Work".into()];
        anna.favorite = true;
        let mut ben = person("Ben", "Krüger");
        ben.groups = vec!["Book club".into(), "Work".into()];
        let lukasz = person("Łukasz", "Nowak");
        let mut wang = Contact {
            uid: "wang".into(),
            formatted: "\u{738b}\u{82b3}".into(),
            ..Contact::default()
        };
        wang.emails = vec![Labeled::new("home", "fang@example.org")];
        let company = Contact {
            uid: "nw".into(),
            org: "Northwind GmbH".into(),
            ..Contact::default()
        };
        let mono = person("Madonna", "");
        vec![anna, ben, lukasz, wang, company, mono, person("Annika", "Roth")]
    }

    #[test]
    fn diacritics_and_case_fold_away() {
        assert_eq!(fold("Krüger"), "kruger");
        assert_eq!(fold("\u{141}ukasz"), "lukasz");
        assert_eq!(fold("Ångström"), "angstrom");
        assert_eq!(fold("Straße"), "strasse");
        assert_eq!(fold("\u{738b}\u{82b3}"), "\u{738b}\u{82b3}");
    }

    #[test]
    fn sorting_by_first_or_last_name_with_others_after_z() {
        let b = book();
        let first = view(&b, SortBy::First, &Filter::All, "");
        assert_eq!(
            names(&b, &first),
            vec!["Anna Berg", "Annika Roth", "Ben Krüger", "\u{141}ukasz Nowak", "Madonna", "Northwind GmbH", "\u{738b}\u{82b3}"]
        );
        let last = view(&b, SortBy::Last, &Filter::All, "");
        assert_eq!(
            names(&b, &last),
            vec!["Anna Berg", "Ben Krüger", "Madonna", "Northwind GmbH", "\u{141}ukasz Nowak", "Annika Roth", "\u{738b}\u{82b3}"]
        );
    }

    #[test]
    fn letter_sections_and_the_jump_bar() {
        let b = book();
        let list = view(&b, SortBy::First, &Filter::All, "");
        let s = sections(&b, &list, SortBy::First);
        let letters: Vec<char> = s.iter().map(|(l, _)| *l).collect();
        assert_eq!(letters, vec!['A', 'B', 'L', 'M', 'N', '#']);
        assert_eq!(s[0], ('A', vec![0, 6]));
        assert_eq!(letter(&b[2], SortBy::First), 'L', "Łukasz files under L");
        assert_eq!(letter(&b[3], SortBy::First), '#');
        let present = vec!['A', 'B', 'H', 'K', '#'];
        assert_eq!(jump_target(&present, 'C'), Some('H'));
        assert_eq!(jump_target(&present, 'B'), Some('B'));
        assert_eq!(jump_target(&present, 'Z'), Some('#'));
        assert_eq!(jump_target(&['A', 'B'], 'Z'), Some('B'), "past the end: the last section");
        assert_eq!(jump_target(&[], 'A'), None);
    }

    #[test]
    fn initials_for_people_companies_and_single_names() {
        let b = book();
        assert_eq!(initials(&b[0]), "AB");
        assert_eq!(initials(&b[2]), "\u{141}N");
        assert_eq!(initials(&b[3]), "\u{738b}");
        assert_eq!(initials(&b[4]), "NG");
        assert_eq!(initials(&b[5]), "M");
        assert_eq!(initials(&Contact::default()), "?", "nothing to take initials from");
    }

    #[test]
    fn search_finds_names_companies_emails_and_phone_digits() {
        let b = book();
        let found = |q: &str| names(&b, &view(&b, SortBy::First, &Filter::All, q));
        assert_eq!(found("berg"), vec!["Anna Berg"]);
        assert_eq!(found("KRUG"), vec!["Ben Krüger"]);
        assert_eq!(found("krüg"), vec!["Ben Krüger"]);
        assert_eq!(found("northwind finance"), vec!["Anna Berg"]);
        assert_eq!(found("0000 0004"), vec!["Anna Berg"]);
        assert_eq!(found("+49 151"), vec!["Anna Berg"]);
        assert_eq!(found("fang@"), vec!["\u{738b}\u{82b3}"]);
        assert_eq!(found("northwind").len(), 2);
        assert!(found("nobody").is_empty());
    }

    #[test]
    fn favourites_and_groups_filter_the_list() {
        let b = book();
        assert_eq!(names(&b, &view(&b, SortBy::First, &Filter::Favorites, "")), vec!["Anna Berg"]);
        assert_eq!(
            names(&b, &view(&b, SortBy::First, &Filter::Group("Work".into()), "")),
            vec!["Anna Berg", "Ben Krüger"]
        );
        assert_eq!(group_counts(&b), vec![("Book club".to_string(), 1), ("Work".to_string(), 2)]);
    }
}
