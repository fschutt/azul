//! `font-weight` is a number from 1 to 1000, and a variable font draws exactly that weight.
//!
//! The parser took the nine hundreds only: `350`, `650`, `1000` and `1` were errors, the
//! declaration was dropped and the node kept the inherited weight, so a variable face was drawn at
//! 300, 400, 700 or not at all (the nine buckets), never at 350. The request now carries the
//! number; the chain draws the face as the STATIC instance of the file at the weight's
//! coordinate (clamped to the axis, 300 to 700 here), and two weights are two chains.
//!
//! The face is registered as a FILE, the way a system scan registers `SFNS.ttf`.

use rust_fontconfig::FontId;

use crate::variable_fonts::{
    cache_with_file, chain_of, drawn_bytes, drawn_face, first_face, instance_bytes, page, resolve,
    red_hat_mono_path,
};

/// Every weight the document asks for draws the file at that coordinate: the instance of the file
/// at `wght` = the weight, byte for byte, and 1000 and 1 are clamped to the axis (700 and 300).
#[test]
fn a_numeric_weight_draws_the_file_at_exactly_that_weight() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    for (css, wght) in [
        ("font-weight: 350", 350.0),
        ("font-weight: 650", 650.0),
        ("font-weight: 1000", 700.0),
        ("font-weight: 1", 300.0),
        ("font-weight: 450.5", 450.5),
    ] {
        let drawn = drawn_bytes(&cache, &family, css);
        let expected = instance_bytes(&file, &[(*b"wght", wght)]);
        assert!(
            drawn == expected,
            "`{css}` draws {} bytes, not the {} bytes of the file at wght {wght}",
            drawn.len(),
            expected.len()
        );
    }
}

/// The weights of the hundreds still draw their own instances, and they differ from each other
/// and from 350 and 650.
#[test]
fn the_instances_of_different_weights_are_different_faces() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    let mut seen: Vec<(&str, Vec<u8>)> = Vec::new();
    for css in [
        "font-weight: 300",
        "font-weight: 350",
        "font-weight: 400",
        "font-weight: 650",
        "font-weight: 700",
    ] {
        let drawn = drawn_bytes(&cache, &family, css);
        for (other, bytes) in &seen {
            assert!(drawn != *bytes, "`{css}` draws the same face as `{other}`");
        }
        seen.push((css, drawn));
    }
    let at_700 = instance_bytes(&file, &[(*b"wght", 700.0)]);
    assert!(
        seen[4].1 == at_700,
        "`font-weight: 700` is the file's 700 instance"
    );
}

/// The same request twice is one face, not two: the instance is made once.
#[test]
fn the_same_weight_twice_is_one_face() {
    let (cache, family, _source, _file) = cache_with_file(&red_hat_mono_path());
    let first: FontId = drawn_face(&cache, &family, "font-weight: 650");
    let second: FontId = drawn_face(&cache, &family, "font-weight: 650");
    assert_eq!(first, second, "one request, one instance");
    let other: FontId = drawn_face(&cache, &family, "font-weight: 651");
    assert_ne!(first, other, "650 and 651 are two instances");
}

/// 350 and 400 are two chains: the key holds the whole request, not the bucket both weights fall
/// in. (They were one key, `Normal`, and the face chosen for the first served the second.)
#[test]
fn two_weights_of_one_bucket_are_two_chains() {
    let (cache, family, _source, _file) = cache_with_file(&red_hat_mono_path());
    let at = |weight: &str| {
        let styled = page(&family, "", &format!("font-weight: {weight}"), "W");
        let resolved = resolve(&styled, &cache);
        let chain = chain_of(&resolved, &family);
        let key = resolved
            .chains
            .keys()
            .find_map(|key| key.as_chain().filter(|k| k.font_families[0] == family).cloned())
            .expect("the chain's key");
        (key, first_face(&chain))
    };
    let (key_350, face_350) = at("350");
    let (key_400, face_400) = at("400");
    assert_ne!(key_350, key_400, "350 and 400 share one chain key");
    assert_ne!(face_350, face_400, "350 and 400 draw one face");
}
