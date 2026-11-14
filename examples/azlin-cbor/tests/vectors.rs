//! C1: the golden vectors of the profile (`tests/vectors/cbor_strict.json`): the integer, byte
//! string, text string, array, map and simple-value examples of RFC 8949 Appendix A that are
//! inside the profile, the boundaries of every integer width and length form, and the refusals
//! with the reason each one gets.
//!
//! The file is the contract other implementations and `azlin-contract` copy (INT-WIRE:
//! `azlin-contract/fixtures/c4_cbor_strict.json`). The table below is its source: the file is
//! written only under `UPDATE_VECTORS=1`, never when `CI` is set, and otherwise this test fails
//! with the first difference. `expect` of a valid vector is the diagnostic notation of the value
//! (RFC 8949 section 8), of an invalid one the `DecodeError` variant; all are decoded under
//! `Limits::OBJECT`.

mod common;

use azlin_cbor::{decode, encode, DecodeError, Limits, Value};
use common::{diag, from_hex, load_vectors, render_json, vector_path, Vector};

fn pos(name: &str, hex: String, expect: String) -> Vector {
    Vector {
        name: name.to_string(),
        valid: true,
        hex,
        expect,
    }
}

fn neg(name: &str, hex: &str, error: DecodeError) -> Vector {
    Vector {
        name: name.to_string(),
        valid: false,
        hex: hex.to_string(),
        expect: format!("{error:?}"),
    }
}

fn rep(unit: &str, n: usize) -> String {
    unit.repeat(n)
}

fn cat(parts: &[&str]) -> String {
    parts.concat()
}

fn s(x: &str) -> String {
    x.to_string()
}

/// The positive vectors. The heads are typed by hand from RFC 8949 section 3; long contents are
/// repeated patterns.
fn positives() -> Vec<Vector> {
    let n25: Vec<String> = (1..=25).map(|i: u32| i.to_string()).collect();
    let zeros = |n: usize| format!("[{}]", vec!["0"; n].join(", "));
    let pairs24: Vec<String> = (0..24).map(|i: u32| format!("{i}: 0")).collect();
    let pair24_hex: String = (0..24u32).map(|i| format!("{i:02x}00")).collect();
    let a = |n: usize| format!("\"{}\"", "a".repeat(n));
    vec![
        // Unsigned integers: every width boundary (RFC 8949 Appendix A has 0 1 10 23 24 25 100
        // 1000 1000000 1000000000000 18446744073709551615).
        pos("uint_0", s("00"), s("0")),
        pos("uint_1", s("01"), s("1")),
        pos("uint_10", s("0a"), s("10")),
        pos("uint_23", s("17"), s("23")),
        pos("uint_24", s("1818"), s("24")),
        pos("uint_25", s("1819"), s("25")),
        pos("uint_100", s("1864"), s("100")),
        pos("uint_255", s("18ff"), s("255")),
        pos("uint_256", s("190100"), s("256")),
        pos("uint_1000", s("1903e8"), s("1000")),
        pos("uint_65535", s("19ffff"), s("65535")),
        pos("uint_65536", s("1a00010000"), s("65536")),
        pos("uint_1000000", s("1a000f4240"), s("1000000")),
        pos("uint_u32_max", s("1affffffff"), s("4294967295")),
        pos(
            "uint_u32_max_plus_1",
            s("1b0000000100000000"),
            s("4294967296"),
        ),
        pos(
            "uint_1000000000000",
            s("1b000000e8d4a51000"),
            s("1000000000000"),
        ),
        pos(
            "uint_i64_max",
            s("1b7fffffffffffffff"),
            s("9223372036854775807"),
        ),
        pos(
            "uint_2_pow_63",
            s("1b8000000000000000"),
            s("9223372036854775808"),
        ),
        pos(
            "uint_u64_max",
            s("1bffffffffffffffff"),
            s("18446744073709551615"),
        ),
        // Negative integers: -1 - n.
        pos("nint_minus_1", s("20"), s("-1")),
        pos("nint_minus_10", s("29"), s("-10")),
        pos("nint_minus_24", s("37"), s("-24")),
        pos("nint_minus_25", s("3818"), s("-25")),
        pos("nint_minus_100", s("3863"), s("-100")),
        pos("nint_minus_256", s("38ff"), s("-256")),
        pos("nint_minus_257", s("390100"), s("-257")),
        pos("nint_minus_1000", s("3903e7"), s("-1000")),
        pos("nint_minus_65536", s("39ffff"), s("-65536")),
        pos("nint_minus_65537", s("3a00010000"), s("-65537")),
        pos("nint_minus_2_pow_32", s("3affffffff"), s("-4294967296")),
        pos(
            "nint_minus_2_pow_32_minus_1",
            s("3b0000000100000000"),
            s("-4294967297"),
        ),
        pos(
            "nint_minus_2_pow_64",
            s("3bffffffffffffffff"),
            s("-18446744073709551616"),
        ),
        // Simple values in the profile.
        pos("false", s("f4"), s("false")),
        pos("true", s("f5"), s("true")),
        pos("null", s("f6"), s("null")),
        // Byte strings: every length form.
        pos("bytes_empty", s("40"), s("h''")),
        pos("bytes_4", s("4401020304"), s("h'01020304'")),
        pos(
            "bytes_23",
            cat(&["57", &rep("ab", 23)]),
            format!("h'{}'", rep("ab", 23)),
        ),
        pos(
            "bytes_24",
            cat(&["5818", &rep("ab", 24)]),
            format!("h'{}'", rep("ab", 24)),
        ),
        pos(
            "bytes_255",
            cat(&["58ff", &rep("ab", 255)]),
            format!("h'{}'", rep("ab", 255)),
        ),
        pos(
            "bytes_256",
            cat(&["590100", &rep("ab", 256)]),
            format!("h'{}'", rep("ab", 256)),
        ),
        // Text strings: RFC 8949 Appendix A, the length forms, NUL, and NFC next to NFD.
        pos("text_empty", s("60"), s("\"\"")),
        pos("text_a", s("6161"), s("\"a\"")),
        pos("text_ietf", s("6449455446"), s("\"IETF\"")),
        pos("text_quote_backslash", s("62225c"), s("\"\\\"\\\\\"")),
        pos("text_u00fc", s("62c3bc"), s("\"\\u00fc\"")),
        pos("text_u6c34", s("63e6b0b4"), s("\"\\u6c34\"")),
        pos("text_u10151", s("64f0908591"), s("\"\\ud800\\udd51\"")),
        pos("text_nul", s("6100"), s("\"\\u0000\"")),
        pos("text_nfc_e_acute", s("62c3a9"), s("\"\\u00e9\"")),
        pos("text_nfd_e_acute", s("6365cc81"), s("\"e\\u0301\"")),
        pos("text_23", cat(&["77", &rep("61", 23)]), a(23)),
        pos("text_24", cat(&["7818", &rep("61", 24)]), a(24)),
        // Arrays.
        pos("array_empty", s("80"), s("[]")),
        pos("array_123", s("83010203"), s("[1, 2, 3]")),
        pos(
            "array_nested",
            s("8301820203820405"),
            s("[1, [2, 3], [4, 5]]"),
        ),
        pos(
            "array_25",
            s("98190102030405060708090a0b0c0d0e0f101112131415161718181819"),
            format!("[{}]", n25.join(", ")),
        ),
        pos("array_23_zeros", cat(&["97", &rep("00", 23)]), zeros(23)),
        pos("array_24_zeros", cat(&["9818", &rep("00", 24)]), zeros(24)),
        pos(
            "array_256_zeros",
            cat(&["990100", &rep("00", 256)]),
            zeros(256),
        ),
        pos(
            "array_mixed",
            s("86012041ff6161f5f6"),
            s("[1, -1, h'ff', \"a\", true, null]"),
        ),
        pos(
            "array_depth_12",
            cat(&[&rep("81", 11), "80"]),
            cat(&[&rep("[", 12), &rep("]", 12)]),
        ),
        // Maps: keys in the bytewise order of their encodings.
        pos("map_empty", s("a0"), s("{}")),
        pos("map_ints", s("a201020304"), s("{1: 2, 3: 4}")),
        pos(
            "map_text",
            s("a26161016162820203"),
            s("{\"a\": 1, \"b\": [2, 3]}"),
        ),
        pos(
            "map_in_array",
            s("826161a161626163"),
            s("[\"a\", {\"b\": \"c\"}]"),
        ),
        pos(
            "map_five",
            cat(&[
                "a5", "61616141", "61626142", "61636143", "61646144", "61656145",
            ]),
            s("{\"a\": \"A\", \"b\": \"B\", \"c\": \"C\", \"d\": \"D\", \"e\": \"E\"}"),
        ),
        pos(
            "map_keys_23_24_256_a",
            cat(&["a4", "1700", "181801", "19010002", "616103"]),
            s("{23: 0, 24: 1, 256: 2, \"a\": 3}"),
        ),
        pos(
            "map_every_key_type",
            cat(&[
                "a9", "0100", "2001", "410002", "616103", "8004", "a005", "f406", "f507", "f608",
            ]),
            s("{1: 0, -1: 1, h'00': 2, \"a\": 3, []: 4, {}: 5, false: 6, true: 7, null: 8}"),
        ),
        pos(
            "map_text_keys_shorter_first",
            s("a261620162616102"),
            s("{\"b\": 1, \"aa\": 2}"),
        ),
        pos("map_array_keys", s("a2810100810201"), s("{[1]: 0, [2]: 1}")),
        pos("map_map_key", s("a1a1010203"), s("{{1: 2}: 3}")),
        pos(
            "map_24_pairs",
            cat(&["b818", &pair24_hex]),
            format!("{{{}}}", pairs24.join(", ")),
        ),
    ]
}

/// The negative vectors: one reason each, decoded under `Limits::OBJECT` (4 MiB, depth 12, 65536
/// items per container).
fn negatives() -> Vec<Vector> {
    use DecodeError::*;
    let deep_arrays = cat(&[&rep("81", 12), "80"]);
    let deep_maps = cat(&[&rep("a100", 12), "a0"]);
    let deep_after_leaf = cat(&["8241aa", &rep("81", 12), "80"]);
    vec![
        neg("not_shortest_uint_1byte_0", "1800", NotShortest),
        neg("not_shortest_uint_1byte_23", "1817", NotShortest),
        neg("not_shortest_uint_2byte_24", "190018", NotShortest),
        neg("not_shortest_uint_2byte_255", "1900ff", NotShortest),
        neg("not_shortest_uint_2byte_0", "190000", NotShortest),
        neg("not_shortest_uint_4byte_65535", "1a0000ffff", NotShortest),
        neg(
            "not_shortest_uint_8byte_u32_max",
            "1b00000000ffffffff",
            NotShortest,
        ),
        neg("not_shortest_nint_1byte_0", "3800", NotShortest),
        neg("not_shortest_nint_1byte_23", "3817", NotShortest),
        neg(
            "not_shortest_nint_8byte_0",
            "3b0000000000000000",
            NotShortest,
        ),
        neg("not_shortest_bytes_length", "5800", NotShortest),
        neg("not_shortest_bytes_length_2byte", "590000", NotShortest),
        neg("not_shortest_text_length", "7800", NotShortest),
        neg("not_shortest_array_length", "9800", NotShortest),
        neg("not_shortest_array_length_2byte", "990000", NotShortest),
        neg("not_shortest_map_length", "b800", NotShortest),
        neg("not_shortest_inside_array", "82011817", NotShortest),
        neg("truncated_empty_input", "", Truncated),
        neg("truncated_head_1byte", "18", Truncated),
        neg("truncated_head_2byte", "19ff", Truncated),
        neg("truncated_head_4byte", "1a000000", Truncated),
        neg("truncated_head_8byte", "1b00", Truncated),
        neg("truncated_bytes", "4301", Truncated),
        neg("truncated_text", "6261", Truncated),
        neg("truncated_array", "8201", Truncated),
        neg("truncated_map_value", "a2010203", Truncated),
        neg("truncated_map_pair", "a101", Truncated),
        neg("truncated_nested", "8281", Truncated),
        neg("trailing_after_uint", "0000", TrailingBytes),
        neg("trailing_after_array", "8000", TrailingBytes),
        neg("trailing_after_map", "a000", TrailingBytes),
        neg("trailing_after_true", "f500", TrailingBytes),
        neg("trailing_after_text", "616100", TrailingBytes),
        neg("indefinite_bytes", "5f", IndefiniteLength),
        neg("indefinite_text", "7f", IndefiniteLength),
        neg("indefinite_array", "9f", IndefiniteLength),
        neg("indefinite_map", "bf", IndefiniteLength),
        neg("indefinite_bytes_chunked", "5f4100ff", IndefiniteLength),
        neg("indefinite_array_closed", "9f01ff", IndefiniteLength),
        neg("indefinite_map_closed", "bf0102ff", IndefiniteLength),
        neg("break_alone", "ff", ReservedBreak),
        neg("break_in_definite_array", "8201ff", ReservedBreak),
        neg("break_as_map_value", "a101ff", ReservedBreak),
        neg("unsorted_keys_2_before_1", "a202000100", UnsortedMapKeys),
        neg(
            "unsorted_keys_24_before_23",
            "a21818001700",
            UnsortedMapKeys,
        ),
        neg(
            "unsorted_keys_text_before_23",
            "a26161001700",
            UnsortedMapKeys,
        ),
        neg(
            "unsorted_keys_aa_before_b",
            "a262616100616201",
            UnsortedMapKeys,
        ),
        neg(
            "unsorted_keys_bytes_00_before_empty",
            "a24100004000",
            UnsortedMapKeys,
        ),
        neg(
            "unsorted_keys_false_before_1",
            "a2f4000100",
            UnsortedMapKeys,
        ),
        neg(
            "unsorted_keys_in_a_nested_map",
            "a101a202000100",
            UnsortedMapKeys,
        ),
        neg("duplicate_key_uint", "a201000101", DuplicateMapKey),
        neg("duplicate_key_text", "a2616100616101", DuplicateMapKey),
        neg(
            "duplicate_key_in_a_nested_map",
            "a101a205000501",
            DuplicateMapKey,
        ),
        neg("float_half", "f93c00", DisallowedType),
        neg("float_single", "fa3f800000", DisallowedType),
        neg("float_double", "fb3ff0000000000000", DisallowedType),
        neg("float_inside_array", "8201f93c00", DisallowedType),
        neg("tag_0", "c000", DisallowedType),
        neg("tag_1_epoch", "c11a514b67b0", DisallowedType),
        neg("tag_bignum", "c249010000000000000000", DisallowedType),
        neg("tag_u64_max", "dbffffffffffffffff00", DisallowedType),
        neg("undefined", "f7", DisallowedType),
        neg("simple_0", "e0", DisallowedType),
        neg("simple_16", "f0", DisallowedType),
        neg("simple_32_one_byte_follows", "f820", DisallowedType),
        neg("reserved_head_uint_28", "1c", DisallowedType),
        neg("reserved_head_uint_31", "1f", DisallowedType),
        neg("reserved_head_nint_28", "3c", DisallowedType),
        neg("reserved_head_nint_31", "3f", DisallowedType),
        neg("reserved_head_bytes_28", "5c", DisallowedType),
        neg("reserved_head_text_29", "7d", DisallowedType),
        neg("reserved_head_array_30", "9e", DisallowedType),
        neg("reserved_head_map_28", "bc", DisallowedType),
        neg("reserved_head_simple_28", "fc", DisallowedType),
        neg("reserved_head_simple_30", "fe", DisallowedType),
        neg("utf8_lone_ff", "61ff", InvalidUtf8),
        neg("utf8_overlong_2byte_nul", "62c080", InvalidUtf8),
        neg("utf8_overlong_2byte_a", "62c1a1", InvalidUtf8),
        neg("utf8_surrogate_d800", "63eda080", InvalidUtf8),
        neg("utf8_above_10ffff", "64f4908080", InvalidUtf8),
        neg("utf8_truncated_2byte", "61c3", InvalidUtf8),
        neg("utf8_truncated_3byte", "62e282", InvalidUtf8),
        neg("utf8_lone_continuation", "6180", InvalidUtf8),
        neg("depth_13_arrays", &deep_arrays, DepthExceeded),
        neg("depth_13_maps", &deep_maps, DepthExceeded),
        neg("items_array_65537", "9a00010001", TooManyItems),
        neg("items_map_65537", "ba00010001", TooManyItems),
        neg("items_array_u64_max", "9bffffffffffffffff", TooManyItems),
        neg("items_map_u64_max", "bbffffffffffffffff", TooManyItems),
        neg("size_bytes_over_max_text", "5a00400001", InputTooLarge),
        neg("size_text_over_max_text", "7a00400001", InputTooLarge),
        neg("size_bytes_u64_max", "5bffffffffffffffff", InputTooLarge),
        neg("size_text_u64_max", "7bffffffffffffffff", InputTooLarge),
        // Refused AFTER byte or text strings were decoded: a secret decoder wipes the partial tree
        // before the error returns (`decode_secret`, src/secret_tests.rs C17 counts the bytes).
        neg(
            "secret_array_leaves_then_truncated",
            "8341aa626262",
            Truncated,
        ),
        neg(
            "secret_nested_arrays_then_reserved_head",
            "828241aa42bbcc1c",
            DisallowedType,
        ),
        neg(
            "secret_map_duplicate_bytes_key",
            "a2410141aa410141bb",
            DuplicateMapKey,
        ),
        neg(
            "secret_map_value_fails_after_key",
            "a14102f93c00",
            DisallowedType,
        ),
        neg(
            "secret_map_unsorted_text_keys",
            "a262626241aa616141bb",
            UnsortedMapKeys,
        ),
        neg("secret_trailing_after_tree", "8241aa616200", TrailingBytes),
        neg("secret_utf8_after_leaf", "8241aa61ff", InvalidUtf8),
        neg(
            "secret_too_many_items_after_leaf",
            "8241aa9a00010001",
            TooManyItems,
        ),
        neg("secret_depth_after_leaf", &deep_after_leaf, DepthExceeded),
        neg("secret_size_after_leaf", "8241aa5a00400001", InputTooLarge),
        neg("secret_indefinite_after_leaf", "8241aa5f", IndefiniteLength),
        neg("secret_break_after_leaf", "8241aaff", ReservedBreak),
        neg("secret_not_shortest_after_leaf", "8241aa1800", NotShortest),
        neg(
            "secret_deep_leaves_then_break",
            "8342aabb8263636363a141016164ff",
            ReservedBreak,
        ),
        neg(
            "secret_array_key_leaves_then_reserved_value",
            "a18241aa42bbcc1c",
            DisallowedType,
        ),
        neg(
            "secret_second_pair_key_fails_after_a_full_pair",
            "a2410141aa1c",
            DisallowedType,
        ),
    ]
}

fn table() -> Vec<Vector> {
    let mut rows = positives();
    rows.extend(negatives());
    rows
}

/// Where the first difference of two texts is, for the failure message.
fn first_difference(have: &str, want: &str) -> String {
    for (i, (h, w)) in have.lines().zip(want.lines()).enumerate() {
        if h != w {
            return format!("line {}:\n  file:  {h}\n  table: {w}", i + 1);
        }
    }
    format!(
        "the file has {} lines, the table {}",
        have.lines().count(),
        want.lines().count()
    )
}

#[test]
fn c1_vector_file_is_the_table() {
    let want = render_json(&table());
    let path = vector_path();
    if std::env::var("UPDATE_VECTORS").is_ok_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_VECTORS=1 is refused when CI is set"
        );
        std::fs::create_dir_all(path.parent().expect("a parent folder"))
            .expect("creates tests/vectors");
        std::fs::write(&path, &want).expect("writes the vector file");
        return;
    }
    let have = std::fs::read_to_string(&path).expect(
        "tests/vectors/cbor_strict.json exists: write it once with UPDATE_VECTORS=1 (never in CI)",
    );
    assert!(
        have == want,
        "tests/vectors/cbor_strict.json differs from the table of tests/vectors.rs, first at {}\n(UPDATE_VECTORS=1 rewrites it, never in CI)",
        first_difference(&have, &want)
    );
}

#[test]
fn c1_vector_names_are_unique_and_hex_is_well_formed() {
    let rows = load_vectors();
    let mut names = std::collections::BTreeSet::new();
    for r in &rows {
        assert!(
            names.insert(r.name.clone()),
            "vector name used twice: {}",
            r.name
        );
        assert!(
            r.hex.len().is_multiple_of(2)
                && r.hex
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()),
            "bad hex in {}",
            r.name
        );
    }
}

#[test]
fn c1_rfc8949_appendix_a_subset_roundtrips() {
    let rows = load_vectors();
    let positives: Vec<&Vector> = rows.iter().filter(|r| r.valid).collect();
    assert!(
        positives.len() >= 60,
        "at least 60 positive vectors, have {}",
        positives.len()
    );
    for r in &positives {
        let bytes = from_hex(&r.hex);
        let value = decode(&bytes, &Limits::OBJECT)
            .unwrap_or_else(|e| panic!("{}: refused with {e:?}", r.name));
        assert_eq!(diag(&value), r.expect, "{}: the value", r.name);
        let again = encode(&value, &Limits::OBJECT)
            .unwrap_or_else(|e| panic!("{}: not encodable: {e:?}", r.name));
        assert_eq!(again, bytes, "{}: encode(decode(b)) must be b", r.name);
    }
}

#[test]
fn c1_the_integer_boundaries_of_the_brief_are_vectors() {
    let rows = load_vectors();
    let wanted: Vec<Value> = vec![
        Value::Uint(0),
        Value::Uint(23),
        Value::Uint(24),
        Value::Uint(255),
        Value::Uint(256),
        Value::Uint(65535),
        Value::Uint(65536),
        Value::Uint(u64::from(u32::MAX)),
        Value::Uint(u64::from(u32::MAX) + 1),
        Value::Uint(u64::MAX),
        Value::Nint(0),
        Value::Nint(23),
        Value::Nint(24),
        Value::Nint(255),
        Value::Nint(u64::MAX),
    ];
    for v in wanted {
        let bytes = encode(&v, &Limits::OBJECT).expect("an integer encodes");
        assert!(
            rows.iter().any(|r| r.valid && from_hex(&r.hex) == bytes),
            "no vector holds {} ({})",
            diag(&v),
            common::to_hex(&bytes)
        );
    }
}

#[test]
fn c1_negative_vectors_are_refused_with_their_reason() {
    let rows = load_vectors();
    let negatives: Vec<&Vector> = rows.iter().filter(|r| !r.valid).collect();
    assert!(negatives.len() >= 3, "at least three negative vectors");
    for r in &negatives {
        let bytes = from_hex(&r.hex);
        let got = decode(&bytes, &Limits::OBJECT);
        match got {
            Ok(v) => panic!("{}: accepted as {}", r.name, diag(&v)),
            Err(e) => assert_eq!(format!("{e:?}"), r.expect, "{}: the reason", r.name),
        }
    }
}

#[test]
fn c1_every_refusal_reason_has_a_negative_vector() {
    let rows = load_vectors();
    let reasons: std::collections::BTreeSet<String> = rows
        .iter()
        .filter(|r| !r.valid)
        .map(|r| r.expect.clone())
        .collect();
    for name in [
        "Truncated",
        "TrailingBytes",
        "NotShortest",
        "IndefiniteLength",
        "UnsortedMapKeys",
        "DuplicateMapKey",
        "DisallowedType",
        "InvalidUtf8",
        "DepthExceeded",
        "TooManyItems",
        "InputTooLarge",
        "ReservedBreak",
    ] {
        assert!(reasons.contains(name), "no negative vector for {name}");
    }
}
