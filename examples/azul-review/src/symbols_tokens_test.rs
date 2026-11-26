//! The words of an identifier, the key two spellings share, the sound a name has, and the
//! bounded distance between two of them. No window, no azul.

use super::*;

fn words(name: &str) -> Vec<String> {
    split_identifier(name)
}

#[test]
fn one_name_in_camel_pascal_snake_screaming_and_kebab_case_gives_the_same_words() {
    let want = vec!["data", "table", "cell"];
    for spelling in ["DataTableCell", "dataTableCell", "data_table_cell", "DATA_TABLE_CELL", "data-table-cell", "Data_Table_Cell"] {
        assert_eq!(words(spelling), want, "{spelling}");
    }
}

#[test]
fn an_acronym_followed_by_a_word_splits_before_the_last_capital() {
    assert_eq!(words("HTTPRequest"), ["http", "request"]);
    assert_eq!(words("IOError"), ["io", "error"]);
    assert_eq!(words("ParseURL"), ["parse", "url"]);
    assert_eq!(words("XMLHttpRequest"), ["xml", "http", "request"]);
    assert_eq!(words("getHTTPResponseCode"), ["get", "http", "response", "code"]);
    assert_eq!(words("AzReview"), ["az", "review"]);
    assert_eq!(words("CssError"), ["css", "error"]);
    assert_eq!(words("CSS_ERROR"), ["css", "error"]);
    assert_eq!(words("iPhone"), ["i", "phone"]);
}

#[test]
fn digits_are_words_of_their_own() {
    assert_eq!(words("utf8"), ["utf", "8"]);
    assert_eq!(words("UTF8String"), ["utf", "8", "string"]);
    assert_eq!(words("sha256sum"), ["sha", "256", "sum"]);
    assert_eq!(words("x86_64"), ["x", "86", "64"]);
    assert_eq!(words("H264Decoder"), ["h", "264", "decoder"]);
    assert_eq!(words("404"), ["404"]);
}

#[test]
fn separators_and_underscores_at_the_ends_are_dropped_and_nothing_gives_no_words() {
    assert_eq!(words("__init__"), ["init"]);
    assert_eq!(words("_"), Vec::<String>::new());
    assert_eq!(words(""), Vec::<String>::new());
    assert_eq!(words("a::b"), ["a", "b"]);
    assert_eq!(words("$ref"), ["ref"]);
    assert_eq!(words("  spaced   out "), ["spaced", "out"]);
}

#[test]
fn letters_outside_ascii_split_like_the_ones_inside_it() {
    assert_eq!(words("ÜberGroß"), ["über", "groß"]);
    assert_eq!(words("größe_in_pixeln"), ["größe", "in", "pixeln"]);
}

#[test]
fn a_name_longer_than_the_cap_is_cut_and_the_words_stop_at_the_cap() {
    let long = "a".repeat(MAX_NAME_CHARS * 4);
    let got = words(&long);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].chars().count(), MAX_NAME_CHARS);
    let many = (0..40).map(|i| format!("w{}", (b'a' + (i % 26) as u8) as char)).collect::<Vec<_>>().join("_");
    assert!(words(&many).len() <= MAX_TOKENS);
}

#[test]
fn a_cut_never_lands_inside_a_multi_byte_letter() {
    let long = "ß".repeat(MAX_NAME_CHARS * 2);
    let got = words(&long);
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].chars().count(), MAX_NAME_CHARS);
}

#[test]
fn the_key_joins_the_words_by_a_space_and_the_concatenation_drops_them() {
    assert_eq!(key_of(&["css", "error"]), "css error");
    assert_eq!(key_of(&["utf".to_string(), "8".to_string()]), "utf 8");
    assert_eq!(key_of::<&str>(&[]), "");
    assert_eq!(concat_of("data table cell"), "datatablecell");
    assert_eq!(name_key("DataTableCell"), "data table cell");
    assert_eq!(name_key("data_table_cell"), name_key("DataTableCell"));
}

#[test]
fn words_that_sound_alike_have_one_skeleton() {
    let same = [
        ("enum", "enemy"),
        ("azreview", "asreview"),
        ("mutex", "mewtex"),
        ("struct", "strukt"),
        ("unwrap", "unrap"),
        ("vec", "veck"),
        ("speechsession", "speachsesion"),
        ("phone", "fone"),
        ("knight", "night"),
    ];
    for (a, b) in same {
        assert_eq!(phonetic(a), phonetic(b), "{a} / {b}");
    }
}

#[test]
fn words_that_sound_different_have_different_skeletons() {
    let different = [("struct", "trait"), ("enum", "union"), ("error", "mirror"), ("css", "html"), ("await", "awake"), ("parse", "pass")];
    for (a, b) in different {
        assert_ne!(phonetic(a), phonetic(b), "{a} / {b}");
    }
}

#[test]
fn the_skeleton_is_short_upper_case_and_never_panics_on_odd_input() {
    for odd in ["", "   ", "123", "ßßß", "日本語", "a", "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx", "\u{0}\u{1}", "😀😀"] {
        let p = phonetic(odd);
        assert!(p.len() <= MAX_PHONETIC, "{odd:?} gave {p:?}");
        assert!(p.bytes().all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()), "{odd:?} gave {p:?}");
    }
    assert_eq!(phonetic("enum"), "ANM");
}

#[test]
fn the_bounded_distance_counts_edits_and_gives_up_past_the_bound() {
    assert_eq!(edit_distance_bounded("kitten", "kitten", 2), Some(0));
    assert_eq!(edit_distance_bounded("kitten", "sitten", 2), Some(1));
    assert_eq!(edit_distance_bounded("kitten", "sitting", 3), Some(3));
    assert_eq!(edit_distance_bounded("kitten", "sitting", 2), None);
    assert_eq!(edit_distance_bounded("", "ab", 2), Some(2));
    assert_eq!(edit_distance_bounded("", "abc", 2), None);
    assert_eq!(edit_distance_bounded("abc", "", 3), Some(3));
    assert_eq!(edit_distance_bounded("flaw", "lawn", 2), Some(2));
    assert_eq!(edit_distance_bounded("ever", "error", 3), Some(3));
    assert_eq!(edit_distance_bounded("ever", "error", 2), None);
}

#[test]
fn the_bounded_distance_is_symmetric_and_counts_letters_not_bytes() {
    for (a, b) in [("css", "sees"), ("größe", "grosse"), ("日本", "日本語")] {
        assert_eq!(edit_distance_bounded(a, b, 4), edit_distance_bounded(b, a, 4), "{a} / {b}");
    }
    assert_eq!(edit_distance_bounded("größe", "grosse", 4), Some(3));
    assert_eq!(edit_distance_bounded("日本", "日本語", 2), Some(1));
}

#[test]
fn a_very_long_input_is_cut_before_it_costs_anything() {
    let long = "x".repeat(100_000);
    assert_eq!(edit_distance_bounded(&long, "x", 2), None);
    assert!(phonetic(&long).len() <= MAX_PHONETIC);
}
