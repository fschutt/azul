//! Tests of the font request (`font_request.rs`): the coordinate, the bucket of a weight, the
//! builders, and what makes two requests two keys.

use super::*;

fn hash_of<T: Hash>(value: &T) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

/// What a stylesheet writes with up to six binary fraction digits is exact, a value closer to
/// another than a step is the same coordinate, and the odd floats do not panic.
#[test]
fn a_coordinate_is_exact_in_steps_of_one_sixty_fourth() {
    for value in [1.0_f32, 350.0, 400.0, 650.0, 1000.0, 87.5, 12.25, 0.015_625, -14.0, 0.0] {
        assert_eq!(FontCoord::from_f32(value).to_f32(), value, "{value} survives the round trip");
    }
    assert_eq!(FontCoord::from_f32(350.0).steps(), 350 * 64);
    assert_eq!(FontCoord::from_units(350), FontCoord::from_f32(350.0));
    assert_eq!(FontCoord::from_f32(87.5).steps(), 5600);
    // closer than a step: the same coordinate
    assert_eq!(FontCoord::from_f32(350.0), FontCoord::from_f32(350.005));
    assert_ne!(FontCoord::from_f32(350.0), FontCoord::from_f32(350.02));
    // half a step rounds away from zero, both ways
    assert_eq!(FontCoord::from_f32(0.5 / 64.0).steps(), 1);
    assert_eq!(FontCoord::from_f32(-0.5 / 64.0).steps(), -1);
    assert_eq!(FontCoord::from_f32(0.4 / 64.0).steps(), 0);
    // not a number is 0, an infinity saturates, neither panics
    assert_eq!(FontCoord::from_f32(f32::NAN), FontCoord::default());
    assert_eq!(FontCoord::from_f32(f32::INFINITY).steps(), i32::MAX);
    assert_eq!(FontCoord::from_f32(f32::NEG_INFINITY).steps(), i32::MIN);
    assert_eq!(FontCoord::from_units(i32::MAX).steps(), i32::MAX);
}

#[test]
fn a_coordinate_is_ordered_by_its_value_and_hashes_by_it() {
    assert!(FontCoord::from_f32(300.0) < FontCoord::from_f32(300.5));
    assert!(FontCoord::from_f32(-1.0) < FontCoord::from_f32(0.0));
    assert_eq!(hash_of(&FontCoord::from_f32(350.0)), hash_of(&FontCoord::from_units(350)));
    assert_ne!(hash_of(&FontCoord::from_f32(350.0)), hash_of(&FontCoord::from_f32(351.0)));
}

/// The default request is the initial value of every font property.
#[test]
fn the_default_request_is_the_initial_value_of_every_property() {
    let request = FontRequest::default();
    assert_eq!(request.weight.to_f32(), 400.0);
    assert_eq!(request.stretch.to_f32(), 100.0);
    assert_eq!(request.style, FontStyle::Normal);
    assert_eq!(request.oblique_degrees, FontCoord::default());
    assert_eq!(request.optical_size, 0);
    assert_eq!(request.settings, None);
    assert_eq!(request.fc_weight(), FcWeight::Normal);
    assert!(!request.is_italic() && !request.is_oblique());
}

/// The bucket a STATIC face is chosen by: the nearest of the nine, the weights of the hundreds
/// in their own, the middle going to the heavier; every weight a stylesheet can write has one.
#[test]
fn the_bucket_of_a_weight_is_the_nearest_of_the_nine() {
    let table = [
        (100, FcWeight::Thin),
        (200, FcWeight::ExtraLight),
        (300, FcWeight::Light),
        (400, FcWeight::Normal),
        (500, FcWeight::Medium),
        (600, FcWeight::SemiBold),
        (700, FcWeight::Bold),
        (800, FcWeight::ExtraBold),
        (900, FcWeight::Black),
    ];
    for (weight, bucket) in table {
        assert_eq!(FontRequest::default().with_weight(weight as f32).fc_weight(), bucket, "{weight}");
        assert_eq!(FontRequest::default().with_weight_bucket(bucket).weight.to_f32(), weight as f32);
    }
    // the same buckets rust-fontconfig names for a whole weight, over the whole range
    for weight in 1..=1000_u16 {
        assert_eq!(
            FontRequest::default().with_weight(f32::from(weight)).fc_weight(),
            FcWeight::from_u16(weight),
            "weight {weight}"
        );
    }
    // between two buckets: the nearest, the middle going to the heavier
    let at = |weight: f32| FontRequest::default().with_weight(weight).fc_weight();
    assert_eq!(at(349.9), FcWeight::Light);
    assert_eq!(at(350.0), FcWeight::Normal);
    assert_eq!(at(449.9), FcWeight::Normal);
    assert_eq!(at(450.0), FcWeight::Medium);
    assert_eq!(at(1.0), FcWeight::Thin);
    assert_eq!(at(149.9), FcWeight::Thin);
    assert_eq!(at(1000.0), FcWeight::Black);
}

#[test]
fn the_weights_that_ask_for_a_bold_face_are_exact() {
    let request = |weight: f32| FontRequest::default().with_weight(weight);
    assert!(!request(599.9).weight_at_least(600));
    assert!(request(600.0).weight_at_least(600));
    assert!(!request(699.0).weight_at_least(700));
    assert!(request(700.0).weight_at_least(700));
    assert!(request(1000.0).weight_at_least(700));
}

#[test]
fn a_style_has_an_angle_only_when_it_is_oblique() {
    let base = FontRequest::default();
    assert_eq!(base.clone().with_style(FontStyle::Oblique).oblique_degrees.to_f32(), 14.0);
    assert_eq!(base.clone().with_oblique(10.0).oblique_degrees.to_f32(), 10.0);
    assert_eq!(base.clone().with_oblique(10.0).style, FontStyle::Oblique);
    // the angle of an oblique request is not part of an italic or an upright one
    let leaned = base.clone().with_oblique(10.0);
    assert_eq!(leaned.clone().with_style(FontStyle::Italic).oblique_degrees, FontCoord::default());
    assert_eq!(leaned.with_style(FontStyle::Normal), base);
}

#[test]
fn the_optical_size_is_the_font_size_in_whole_px() {
    let base = FontRequest::default();
    assert_eq!(base.clone().at_font_size(13.0).optical_size, 13);
    assert_eq!(base.clone().at_font_size(12.4).optical_size, 12);
    assert_eq!(base.clone().at_font_size(12.5).optical_size, 13);
    assert_eq!(base.clone().at_font_size(0.0).optical_size, 0);
    assert_eq!(base.clone().at_font_size(f32::NAN).optical_size, 0);
    assert_eq!(base.at_font_size(-3.0).optical_size, 0);
}

/// `font-variation-settings`: sorted by tag, a tag written twice keeps its last value, `normal`
/// (none) is no list, and the order they were written in does not make two requests.
#[test]
fn the_settings_are_sorted_one_per_tag_and_the_last_wins() {
    let base = FontRequest::default();
    let a = base.clone().with_settings(&[(*b"wght", 650.0), (*b"wdth", 80.0)]);
    let b = base.clone().with_settings(&[(*b"wdth", 80.0), (*b"wght", 650.0)]);
    assert_eq!(a, b, "the order of the settings is not a request");
    assert_eq!(hash_of(&a), hash_of(&b));
    let tags: Vec<FourCc> = a.settings.as_deref().unwrap().iter().map(|(tag, _)| *tag).collect();
    assert_eq!(tags, [*b"wdth", *b"wght"], "sorted by tag");

    let last = base.clone().with_settings(&[(*b"wght", 300.0), (*b"wght", 650.0)]);
    assert_eq!(last.setting(*b"wght").map(FontCoord::to_f32), Some(650.0));
    assert_eq!(last.settings.as_deref().unwrap().len(), 1, "one per tag");

    assert_eq!(base.clone().with_settings(&[]), base, "no settings is `normal`");
    assert_eq!(a.clone().with_settings(&[]), base, "and resets the settings it had");
    assert_eq!(a.setting(*b"opsz"), None);

    let more = a.clone().with_setting(*b"opsz", 20.0).with_setting(*b"wght", 700.0);
    assert_eq!(more.setting(*b"opsz").map(FontCoord::to_f32), Some(20.0));
    assert_eq!(more.setting(*b"wght").map(FontCoord::to_f32), Some(700.0));
    assert_eq!(more.setting(*b"wdth").map(FontCoord::to_f32), Some(80.0));
    assert_ne!(a, more);
}

/// What makes two requests two keys: every coordinate, and the settings.
#[test]
fn requests_that_differ_in_one_coordinate_are_two_keys() {
    let base = FontRequest::default();
    let others = [
        base.clone().with_weight(350.0),
        base.clone().with_weight(401.0),
        base.clone().with_stretch(87.5),
        base.clone().with_style(FontStyle::Italic),
        base.clone().with_oblique(10.0),
        base.clone().with_oblique(11.0),
        base.clone().at_font_size(13.0),
        base.clone().with_settings(&[(*b"wght", 650.0)]),
        base.clone().with_settings(&[(*b"wght", 651.0)]),
        base.clone().with_settings(&[(*b"wdth", 650.0)]),
    ];
    for (i, one) in others.iter().enumerate() {
        assert_ne!(one, &base, "request {i} equals the default");
        assert_ne!(hash_of(one), hash_of(&base), "request {i} hashes like the default");
        for (j, two) in others.iter().enumerate().skip(i + 1) {
            assert_ne!(one, two, "requests {i} and {j} are one request");
            assert_ne!(hash_of(one), hash_of(two), "requests {i} and {j} hash alike");
        }
    }
    // a chain key holds the whole request
    let key = |request: &FontRequest| {
        FontChainKey::from_selectors(&[FontSelector {
            family: "X".to_string(),
            request: request.clone(),
            ..FontSelector::default()
        }])
    };
    assert_eq!(key(&base), key(&FontRequest::default()));
    for (i, one) in others.iter().enumerate() {
        assert_ne!(key(one), key(&base), "request {i} shares the default's chain key");
    }
}

/// Two instances of one variable file are two faces by the identity hash of the parsed font (a
/// sample of the head and the tail of the bytes, and their length): the `name` table of an
/// instance says which one it is, and the table directory carries its checksum.
#[test]
fn two_instances_of_one_font_have_different_hashes() {
    let file = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../doc/fonts/RedHatMono-VariableFont_wght.ttf"
    ))
    .expect("the face is checked in");
    let hash_at = |wght: f32| {
        let bytes = rust_fontconfig::FcCreateVariableInstance(&file, 0, &[(*b"wght", wght)])
            .expect("the file has an instance");
        crate::font::parsed::ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
            .expect("the instance parses")
            .hash
    };
    let hashes = [hash_at(300.0), hash_at(400.0), hash_at(650.0), hash_at(651.0), hash_at(700.0)];
    for (i, one) in hashes.iter().enumerate() {
        for two in &hashes[i + 1..] {
            assert_ne!(one, two, "two instances of one font share a hash: {hashes:x?}");
        }
    }
}
