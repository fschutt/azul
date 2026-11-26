//! The text a node announces on the local network: what it says, that it fits, that a foreign text is not taken for one of ours
//! and that nothing a neighbour writes can break a screen.

use super::*;

fn advert() -> Advert {
    Advert { project: "0123456789abcdef".into(), name: "Studio Mac".into(), user: "Felix".into(), role: Some(Role::Host) }
}

#[test]
fn an_advert_is_its_version_first_then_key_value_fields_and_round_trips() {
    let text = encode(&advert());
    assert!(text.starts_with("azreview/1;"), "{text}");
    assert_eq!(text, "azreview/1;p=0123456789abcdef;n=Studio Mac;u=Felix;r=host");
    assert_eq!(parse(&text), Some(advert()));
}

#[test]
fn every_role_has_its_word_and_an_unknown_word_is_no_role_but_still_an_advert() {
    for role in [Role::Host, Role::Phone, Role::Review, Role::ReviewMirror, Role::Whiteboard] {
        let text = encode(&Advert { role: Some(role), ..advert() });
        assert_eq!(parse(&text).and_then(|a| a.role), Some(role), "{text}");
    }
    let text = "azreview/1;p=0123456789abcdef;n=x;r=hologram";
    let parsed = parse(text).expect("an advert with a role this build does not know");
    assert_eq!((parsed.role, parsed.name.as_str()), (None, "x"));
    assert_eq!(parse(&encode(&Advert { role: None, ..advert() })).map(|a| a.role), Some(None), "no role is no role");
}

#[test]
fn a_foreign_text_is_not_an_advert() {
    for text in ["", "hello", "azreview", "azreview/1", "azreview/2;p=0123456789abcdef", "AZREVIEW/1;p=0123456789abcdef", "xazreview/1;p=0123456789abcdef", "p=0123456789abcdef;n=x", "iroh/1;p=1", "azreview/1x;p=0123456789abcdef"] {
        assert_eq!(parse(text), None, "{text:?}");
    }
}

#[test]
fn the_project_is_required_and_is_a_short_lower_case_hash() {
    assert_eq!(parse("azreview/1;n=Mac"), None, "no project");
    for bad in ["", "ABCDEF0123456789", "0123456789abcdeg", "0123456789abcdef0123456789abcdef0", "../../etc", "a b"] {
        assert_eq!(parse(&format!("azreview/1;p={bad};n=Mac")), None, "{bad:?}");
    }
    assert!(parse("azreview/1;p=a").is_some(), "one digit is short but a hash");
    assert!(parse("azreview/1;p=0123456789abcdef0123456789abcdef").is_some(), "the longest, 32 digits");
}

#[test]
fn unknown_fields_are_ignored_and_the_order_does_not_matter_and_the_first_of_a_key_wins() {
    let text = "azreview/1;r=review;zzz=future;n=Linux box;p=0123456789abcdef;n=second;fancy";
    let parsed = parse(text).expect("an advert with fields this build does not know");
    assert_eq!((parsed.name.as_str(), parsed.role), ("Linux box", Some(Role::Review)));
    assert_eq!(parsed.user, "");
}

#[test]
fn the_separators_inside_a_name_are_escaped_and_come_back() {
    let a = Advert { name: "a;b=c\\d".into(), user: "x;=y".into(), ..advert() };
    let text = encode(&a);
    assert!(text.contains("n=a\\;b\\=c\\\\d"), "{text}");
    assert_eq!(parse(&text), Some(a));
    assert_eq!(parse("azreview/1;p=0123456789abcdef;n=trailing\\"), None, "an escape with nothing after it is a broken text");
}

#[test]
fn the_whole_text_is_at_most_245_bytes_whatever_the_name() {
    for name in ["Ä".repeat(300), "x".repeat(1000), "日本語".repeat(80), "a;".repeat(200)] {
        let a = Advert { name: name.clone(), user: "u".repeat(200), ..advert() };
        let text = encode(&a);
        assert!(text.len() <= MAX_ADVERT_BYTES, "{} bytes for a name of {}", text.len(), name.chars().count());
        let back = parse(&text).expect("what was written is read");
        assert!(name.starts_with(&back.name), "the name is cut at the end, never changed");
        assert!(!back.name.is_empty(), "something of the name is left");
    }
}

#[test]
fn a_name_is_cut_on_a_character_boundary_and_never_in_the_middle_of_one() {
    let a = Advert { name: "É".repeat(100), user: String::new(), ..advert() };
    let text = encode(&a);
    assert!(std::str::from_utf8(text.as_bytes()).is_ok());
    let back = parse(&text).unwrap();
    assert!(back.name.chars().all(|c| c == 'É'), "{:?}", back.name);
    assert!(back.name.len() <= MAX_ADVERT_NAME_BYTES, "{} bytes", back.name.len());
}

#[test]
fn a_text_over_245_bytes_is_not_read_at_all() {
    let long = format!("azreview/1;p=0123456789abcdef;n={}", "x".repeat(300));
    assert!(long.len() > MAX_ADVERT_BYTES);
    assert_eq!(parse(&long), None);
    let exact = format!("azreview/1;p=0123456789abcdef;n={}", "x".repeat(MAX_ADVERT_BYTES - "azreview/1;p=0123456789abcdef;n=".len()));
    assert_eq!(exact.len(), MAX_ADVERT_BYTES);
    assert!(parse(&exact).is_some(), "exactly the limit is read (its name cut to the display cap)");
}

#[test]
fn control_and_direction_characters_never_get_into_a_name() {
    let nasty = "Mac\u{0}\u{7}\n\r\t\u{202E}evil\u{2066}\u{200E}\u{061C}\u{200B}\u{FEFF}\u{2069}";
    let a = Advert { name: nasty.into(), user: "\u{202E}Anna\u{0}".into(), ..advert() };
    let text = encode(&a);
    assert!(!text.chars().any(|c| c.is_control() || ('\u{202A}'..='\u{202E}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c)), "{text:?}");
    let back = parse(&text).unwrap();
    assert_eq!((back.name.as_str(), back.user.as_str()), ("Macevil", "Anna"));
    // and the same when somebody else wrote the text by hand
    let hand = "azreview/1;p=0123456789abcdef;n=Ev\u{202E}il\u{0};u=\u{200E}x";
    let parsed = parse(hand).unwrap();
    assert_eq!((parsed.name.as_str(), parsed.user.as_str()), ("Evil", "x"));
}

#[test]
fn a_name_is_trimmed_and_a_name_of_nothing_but_spaces_is_empty() {
    let a = Advert { name: "   padded  ".into(), user: "  ".into(), ..advert() };
    let back = parse(&encode(&a)).unwrap();
    assert_eq!((back.name.as_str(), back.user.as_str()), ("padded", ""));
}

#[test]
fn the_project_hash_is_short_stable_and_says_nothing_of_the_path() {
    let a = project_hash("git@github.com:fschutt/azul.git");
    assert_eq!(a.len(), PROJECT_HASH_LEN);
    assert!(a.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()), "{a}");
    assert_eq!(a, project_hash("git@github.com:fschutt/azul.git"), "stable");
    assert_ne!(a, project_hash("git@github.com:fschutt/other.git"));
    assert!(!a.contains("azul") && !a.contains("fschutt"));
    assert!(parse(&format!("azreview/1;p={a}")).is_some(), "what the hash makes is a project the parser takes");
    assert_eq!(project_hash(""), project_hash(""), "even nothing has a hash");
}

#[test]
fn whatever_the_bytes_the_parser_answers_and_never_panics() {
    let mut state = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = move || {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        state.wrapping_mul(0x2545_F491_4F6C_DD1D)
    };
    let alphabet: Vec<char> = "azreview/1;=\\pnur0123456789abcdef é日\u{202E}\n".chars().collect();
    for _ in 0..5_000 {
        let len = (next() % 80) as usize;
        let text: String = (0..len).map(|_| alphabet[(next() % alphabet.len() as u64) as usize]).collect();
        let _ = parse(&text);
        let prefixed = format!("azreview/1;{text}");
        if let Some(parsed) = parse(&prefixed) {
            assert!(encode(&parsed).len() <= MAX_ADVERT_BYTES);
        }
    }
}
