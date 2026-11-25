//! Tests of the registered fonts (`fontreg.rs`). Each test registers in a registry it makes
//! itself: no other test sees a face of it or counts one of its generations, whatever runs
//! beside it.

use super::*;

/// The tables of a font, by tag.
fn tables_of(font: &[u8]) -> Vec<(u32, Vec<u8>)> {
    use allsorts::{binary::read::ReadScope, font_data::FontData, tables::FontTableProvider};
    let data = ReadScope::new(font).read::<FontData<'_>>().unwrap();
    let provider = data.table_provider(0).unwrap();
    let mut tags = provider.table_tags().unwrap();
    tags.sort_unstable();
    tags.into_iter()
        .map(|tag| (tag, provider.table_data(tag).unwrap().unwrap().into_owned()))
        .collect()
}

/// `font` as a WOFF 1.0 with its tables stored as they are (allowed: a table whose
/// compressed length is its length is not compressed).
fn woff_of(font: &[u8]) -> Vec<u8> {
    let tables = tables_of(font);
    let directory = 44 + 20 * tables.len();
    let (mut body, mut entries) = (Vec::new(), Vec::new());
    for (tag, data) in &tables {
        entries.push((*tag, (directory + body.len()) as u32, data.len() as u32));
        body.extend_from_slice(data);
        while body.len() % 4 != 0 {
            body.push(0);
        }
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"wOFF");
    out.extend_from_slice(&font[..4]);
    out.extend_from_slice(&((directory + body.len()) as u32).to_be_bytes());
    out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&(font.len() as u32).to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    for _ in 0..5 {
        out.extend_from_slice(&0u32.to_be_bytes());
    }
    for (tag, offset, len) in entries {
        for field in [tag, offset, len, len, 0] {
            out.extend_from_slice(&field.to_be_bytes());
        }
    }
    out.extend_from_slice(&body);
    out
}

/// The id `registered` put the face of `family` into `cache` under.
fn id_in(registered: &RegisteredFonts, cache: &FcFontCache, family: &str) -> FontId {
    let norm = rust_fontconfig::utils::normalize_family_name(family);
    registered
        .ensure_in(cache)
        .into_iter()
        .find(|(n, _)| *n == norm)
        .map(|(_, face)| face.font_match.id)
        .expect("the face is registered and put into the cache")
}

/// A mail's web font comes as a WOFF or a WOFF2: it is unpacked into the plain font, table
/// for table, and registered as one; a plain font is taken as it is.
#[test]
fn a_web_font_is_unpacked_into_its_plain_font_and_registered() {
    let font = crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes.to_vec();
    assert!(sfnt_of(&font).is_none(), "a plain font is used as it is");
    let web = woff_of(&font);
    let unpacked = sfnt_of(&web).expect("the WOFF unpacks");
    assert_eq!(tables_of(&unpacked), tables_of(&font));
    assert!(RegisteredFonts::default().register_font("Azul Fontreg Web Face", &web));
    assert!(sfnt_of(b"wOF2 not a font").is_none());
}

#[test]
fn bytes_that_are_no_font_change_nothing() {
    let registered = RegisteredFonts::default();
    assert!(!registered.register_font("Azul No Font Here", &[0, 1, 2, 3]));
    assert_eq!(registered.generation(), 0);
}

/// The same face registered again changes nothing: its generation stays, and a cache gets it
/// under one id however often it is put in. The registry is the test's own, so its generation
/// is counted by this test alone: no registration beside it can move it between the two reads.
#[test]
fn a_face_registered_once_is_put_in_a_cache_under_one_id() {
    let registered = RegisteredFonts::default();
    let bytes = &crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes;
    assert!(registered.register_font("Azul Fontreg Unit Face", bytes));
    assert_eq!(registered.generation(), 1, "a face registered, a generation counted");
    assert!(registered.register_font("Azul Fontreg Unit Face", bytes));
    assert_eq!(registered.generation(), 1, "the same face again changes nothing");
    let cache = FcFontCache::default();
    let first = registered.ensure_in(&cache);
    let second = registered.ensure_in(&cache);
    let ids = |v: &[(String, MemoryFace)]| -> Vec<FontId> {
        v.iter()
            .filter(|(n, _)| {
                *n == rust_fontconfig::utils::normalize_family_name("Azul Fontreg Unit Face")
            })
            .map(|(_, f)| f.font_match.id)
            .collect()
    };
    assert_eq!(ids(&first).len(), 1);
    assert_eq!(ids(&first), ids(&second));
    assert!(cache.is_memory_font(&ids(&first)[0]));
}

/// Two registries of one process see neither the faces nor the generations of each other: a
/// registry is the state of one app (or one test), not of the process.
#[test]
fn two_registries_do_not_see_each_others_faces_or_generations() {
    let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
    let (one, other) = (RegisteredFonts::default(), RegisteredFonts::default());
    assert!(one.register_font("Azul Fontreg Registry One", &fonts[0].bytes));
    assert!(one.register_font("Azul Fontreg Registry One Bold", &fonts[1].bytes));
    assert_eq!((one.generation(), other.generation()), (2, 0));

    let cache = FcFontCache::default();
    assert!(
        other.ensure_in(&cache).is_empty(),
        "the other registry holds no face to hand over"
    );
    assert!(cache.is_empty(), "and puts none into a cache");
    assert_eq!(one.ensure_in(&cache).len(), 2, "the first holds its two");

    // A registration of the other is counted by the other alone, and the same face under the
    // same family is new to it: each registry registers for itself.
    assert!(other.register_font("Azul Fontreg Registry Other", &fonts[2].bytes));
    assert_eq!((one.generation(), other.generation()), (2, 1));
    assert!(other.register_font("Azul Fontreg Registry One", &fonts[0].bytes));
    assert_eq!((one.generation(), other.generation()), (2, 2));
    assert_eq!(other.ensure_in(&FcFontCache::default()).len(), 2);
    assert_eq!(one.ensure_in(&FcFontCache::default()).len(), 2);
}

/// The faces an app ships are the registry's too, and a registry has its own: those of
/// another are not in it, and setting them is no registration (the generation stays).
#[test]
fn the_faces_an_app_ships_belong_to_the_registry_they_were_set_in() {
    let mock = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts/azul-mock-prop.ttf"))
        .expect("tests/fonts/azul-mock-prop.ttf");
    let (shipped, bare) = (RegisteredFonts::default(), RegisteredFonts::default());
    shipped.set_app_fonts(&[azul_core::resources::NamedFont::new(
        "Azul Fontreg Shipped Face".into(),
        mock.into(),
    )]);
    let faces = shipped.app_faces();
    assert_eq!(faces.len(), 1);
    assert_eq!(faces[0].family, "Azul Fontreg Shipped Face");
    assert!(!faces[0].ranges.is_empty(), "with the code points of its cmap");
    assert!(bare.app_faces().is_empty(), "no face of another registry");
    assert_eq!((shipped.generation(), bare.generation()), (0, 0));
    shipped.set_app_fonts(&[]);
    assert!(shipped.app_faces().is_empty(), "set again, it replaces what was installed");
}

/// A8-08: a runtime face's style is parsed once, when it is registered, however often it is put
/// into font caches. `CallbackInfo::register_font` calls `ensure_in` after each registration,
/// and every window's chain resolution calls it twice: it parsed every registered face each
/// time (n registrations: n (n + 1) / 2 parses).
/// Mutation: `let style = face.style;` -> `self.parse_style(&face.bytes, &face.family)` in
/// `ensure_in` fails it.
#[test]
fn a_registered_faces_style_is_parsed_once() {
    let registered = RegisteredFonts::default();
    let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
    let parses = || registered.style_parses.load(Ordering::Relaxed);
    for i in 0..8 {
        let bytes = &fonts[i % fonts.len()].bytes;
        assert!(registered.register_font(&format!("Azul A8 Style Face {i}"), bytes));
        drop(registered.ensure_in(&FcFontCache::default()));
    }
    assert_eq!(parses(), 8, "one parse per face registered, none per `ensure_in`");
    for _ in 0..4 {
        drop(registered.ensure_in(&FcFontCache::default()));
    }
    assert_eq!(parses(), 8);
}

/// A8-09: a runtime face a font manager loads shares the registry's bytes (one allocation),
/// instead of a copy of the font cache's copy: the cache holds the registry's allocation
/// (rust-fontconfig 5.1), and the loader is handed it.
/// Mutation: a copy of the bytes in `ensure_in` (`bytes: Arc::from(&face.bytes[..])`) and the
/// loaded bytes are another allocation.
#[test]
fn a_loaded_runtime_face_shares_the_registered_bytes() {
    let registered = RegisteredFonts::default();
    let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
    let family = "Azul A8 Shared Bytes Face";
    assert!(registered.register_font(family, &fonts[0].bytes));
    let cache = FcFontCache::default();
    let id = id_in(&registered, &cache, family);
    let ids = std::collections::HashSet::from([id]);
    let result = crate::solver3::getters::load_fonts_from_disk(&ids, &cache, |bytes, index| {
        assert_eq!(index, 0);
        Ok(bytes)
    });
    let loaded = result.loaded.get(&id).expect("the face loads");
    let held = registered.bytes_of(&id).expect("a runtime face");
    let rust_fontconfig::FontBytes::Owned(a) = &**loaded else {
        panic!("a runtime face's bytes are owned");
    };
    assert!(Arc::ptr_eq(a, &held), "the loaded face shares the registry's bytes");
    assert_eq!(&a[..], &fonts[0].bytes[..]);
}

/// A8-26: a face handed over as an `Arc<[u8]>` is kept as that allocation: the registry, and
/// the bytes every loader is handed, are the caller's (no copy); registering it again keeps
/// nothing more.
/// Mutation: `|| Arc::clone(&bytes)` -> `|| Arc::from(&bytes[..])` in `register_font_shared`
/// fails the `ptr_eq`.
#[test]
fn a_face_handed_over_shared_is_kept_without_a_copy() {
    let registered = RegisteredFonts::default();
    let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
    let family = "Azul A8 Handed Over Face";
    let bytes: Arc<[u8]> = Arc::from(&fonts[1 % fonts.len()].bytes[..]);
    let kept = || registered.bytes_kept.load(Ordering::Relaxed);
    assert_eq!(kept(), 0);
    assert!(registered.register_font_shared(family, Arc::clone(&bytes)));
    assert!(registered.register_font_shared(family, Arc::clone(&bytes)), "again: known");
    assert_eq!(kept(), 1, "one face kept");
    let id = id_in(&registered, &FcFontCache::default(), family);
    let held = registered.bytes_of(&id).expect("a runtime face");
    assert!(Arc::ptr_eq(&held, &bytes), "the registry holds the caller's allocation");
}

/// A8-26 (the C API's type): a face registered through `SharedBytes` shares the caller's
/// bytes - the registry and every loaded face hold the very allocation (pointer equality) -
/// and the bytes outlive the caller's handle.
/// Mutation: `bytes.to_arc()` -> `Arc::from(bytes.as_slice())` in the path
/// `CallbackInfo::register_font_shared` takes fails the `ptr_eq`.
#[test]
fn a_face_registered_through_shared_bytes_shares_and_outlives_the_callers_handle() {
    use azul_css::shared_bytes::SharedBytes;
    let registered = RegisteredFonts::default();
    let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
    let family = "Azul A8 C Api Handle Face";
    let handle = SharedBytes::from_arc(Arc::from(&fonts[2 % fonts.len()].bytes[..]));
    let data = handle.as_slice().as_ptr();
    // what `CallbackInfo::register_font_shared` does with the handle
    assert!(registered.register_font_shared(family, handle.to_arc()));
    drop(handle);
    let id = id_in(&registered, &FcFontCache::default(), family);
    let held = registered.bytes_of(&id).expect("a runtime face");
    assert!(core::ptr::eq(held.as_ptr(), data), "the caller's allocation, not a copy");
    assert_eq!(&held[..], &fonts[2 % fonts.len()].bytes[..], "alive after the handle is gone");
}

/// A8-15 (rust-fontconfig 5.1): the font cache a runtime face is put into holds the registry's
/// allocation, not a copy of it: what the cache hands out for the face (`get_font_bytes`) is
/// the registry's `Arc`.
/// Mutation: `bytes: Arc::from(&face.bytes[..])` in `ensure_in` fails it.
#[test]
fn a_font_cache_holds_the_registrys_allocation_of_a_runtime_face() {
    let registered = RegisteredFonts::default();
    let fonts = crate::text3::ui_fonts::bundled_ui_fonts();
    let family = "Azul A8 Cache Shares Face";
    assert!(registered.register_font(family, &fonts[3 % fonts.len()].bytes));
    let cache = FcFontCache::default();
    let id = id_in(&registered, &cache, family);
    let held = registered.bytes_of(&id).expect("a runtime face");
    let from_cache = cache.get_font_bytes(&id).expect("the cache holds the face");
    let rust_fontconfig::FontBytes::Owned(b) = &*from_cache else {
        panic!("a memory face's bytes are owned");
    };
    assert!(Arc::ptr_eq(&held, b), "the cache shares the registry's allocation");
}

/// Path C of a variable face entering a chain: registered at runtime (`CallbackInfo::register_font`).
/// It was never made static - the raw variable file reached shaping and the PDF - and now keeps its
/// `wght` range in its index entry and is drawn as the static instance at the weight the CSS asks.
#[test]
fn a_variable_face_registered_at_runtime_is_drawn_as_a_static_instance() {
    use azul_core::{dom::Dom, styled_dom::StyledDom};

    use crate::{
        solver3::getters::{collect_font_stacks_from_styled_dom, resolve_font_chains_with_registry},
        text3::cache::{FontChainKeyOrRef, FontManager},
    };

    const FAMILY: &str = "Azul Runtime Variable";
    let file = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../doc/fonts/RedHatMono-VariableFont_wght.ttf"
    ))
    .expect("the face is checked in");
    let registered = Arc::new(RegisteredFonts::default());
    assert!(registered.register_font(FAMILY, &file));
    let fonts = FontManager::<azul_css::props::basic::FontRef>::with_registered_fonts(
        FcFontCache::default(),
        Arc::clone(&registered),
    )
    .expect("font manager");

    let norm = rust_fontconfig::utils::normalize_family_name(FAMILY);
    let faces = fonts
        .memory_families
        .get(&norm)
        .expect("the runtime face is indexed");
    assert_eq!(faces.len(), 1);
    assert_eq!(
        faces[0].weight_axis,
        Some((300.0, 700.0)),
        "the face says which weights it covers"
    );

    let drawn_at = |css: &str| -> Vec<u8> {
        let mut dom = Dom::create_body().with_child(
            Dom::create_div()
                .with_css(&format!("font-family: \"{FAMILY}\"; font-size: 16px; {css}"))
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("W")),
        );
        let styled = StyledDom::create(&mut dom, azul_css::css::Css::empty());
        let collected =
            collect_font_stacks_from_styled_dom(&styled, &azul_css::system::Platform::current());
        let resolved = resolve_font_chains_with_registry(
            &collected,
            &fonts.fc_cache,
            None,
            Some(&[]),
            &fonts.memory_families,
            &fonts.settings,
        );
        let chain = resolved
            .chains
            .iter()
            .find_map(|(key, chain)| match key {
                FontChainKeyOrRef::Chain(key)
                    if key.font_families.first().map(String::as_str) == Some(FAMILY) =>
                {
                    Some(chain)
                }
                _ => None,
            })
            .expect("the family's chain");
        let face = chain
            .css_fallbacks
            .iter()
            .find_map(|group| group.fonts.first())
            .expect("the family matched")
            .id;
        fonts
            .fc_cache
            .get_font_bytes(&face)
            .expect("the face drawn has bytes")
            .as_slice()
            .to_vec()
    };
    for (css, coordinates) in [
        ("font-weight: 400", Vec::new()),
        ("font-weight: 700", vec![(*b"wght", 700.0)]),
        ("font-weight: 650", vec![(*b"wght", 650.0)]),
    ] {
        let drawn = drawn_at(css);
        assert!(
            rust_fontconfig::FcReadVariation(&drawn, 0).is_none(),
            "`{css}`: the face drawn is still a variable font"
        );
        let expected = rust_fontconfig::FcCreateVariableInstance(&file, 0, &coordinates)
            .expect("the file has an instance");
        assert!(drawn == expected, "`{css}`: not the instance of the file at {coordinates:?}");
    }
}
