//! Tests of the font manager (`font_manager.rs`): the registered fonts it draws with, the
//! faces it registers by itself and the memo of its chain keys. Each test builds the managers
//! and the registries it uses, so what one registers or counts is seen by no other.

use super::*;

type Manager = FontManager<azul_css::props::basic::FontRef>;

fn stack(family: &str, optical_size: u16) -> Vec<FontSelector> {
    vec![FontSelector {
        family: family.to_string(),
        request: FontRequest {
            optical_size,
            ..FontRequest::default()
        },
        ..FontSelector::default()
    }]
}

/// The fonts an app registers at startup (`AppInternal::create` sets
/// `AppConfig::bundled_fonts` in the app's registry) are in every font manager made on that
/// registry - each window's, the PDF writer's - with the coverage of their cmap, in the tier the
/// bundled faces have: they stand at their family's place in a stack, and an installed copy of
/// the same family wins. A manager of another registry does not have them.
#[test]
fn a_font_manager_built_on_the_apps_registry_has_the_apps_fonts() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fonts/azul-mock-prop.ttf"
    ))
    .expect("tests/fonts/azul-mock-prop.ttf");
    let registered = Arc::new(RegisteredFonts::default());
    registered.set_app_fonts(&[azul_core::resources::NamedFont::new(
        "Azul Startup Face".into(),
        bytes.into(),
    )]);
    let fonts = Manager::with_registered_fonts(FcFontCache::default(), registered)
        .expect("font manager");
    let norm = rust_fontconfig::utils::normalize_family_name("Azul Startup Face");
    let faces = fonts
        .memory_families
        .get(&norm)
        .expect("the app's family is registered");
    assert_eq!(faces.len(), 1, "one face");
    assert_eq!(faces[0].tier, MemoryFontTier::Fallback);
    assert!(
        faces[0]
            .font_match
            .unicode_ranges
            .iter()
            .any(|r| r.start <= u32::from('a') && u32::from('a') <= r.end),
        "with the coverage of its cmap: {:?}",
        faces[0].font_match.unicode_ranges
    );

    let alone = Manager::new(FcFontCache::default()).expect("font manager");
    assert!(
        !alone.memory_families.contains_key(&norm),
        "a manager of another registry has none of the app's faces"
    );
}

/// A manager has registered fonts of its own: what is registered through one is not seen by
/// another, a clone shares them, and two managers given one registry share it - the ones an app
/// shares between its windows - with the faces registered in it before and after.
#[test]
fn managers_share_registered_fonts_only_when_they_are_given_the_same_ones() {
    let bytes = &crate::text3::ui_fonts::bundled_ui_fonts()[0].bytes;
    let alone = Manager::new(FcFontCache::default()).expect("a manager");
    let other = Manager::new(FcFontCache::default()).expect("another manager");
    assert!(!Arc::ptr_eq(&alone.registered_fonts, &other.registered_fonts));
    assert!(alone.registered_fonts.register_font("Azul Manager Alone Face", bytes));
    assert_eq!(
        (alone.registered_fonts.generation(), other.registered_fonts.generation()),
        (1, 0),
        "a face registered through one manager is no registration of the other"
    );

    let clone = alone.clone_shared();
    assert!(Arc::ptr_eq(&alone.registered_fonts, &clone.registered_fonts), "a clone shares");
    assert_eq!(clone.registered_fonts.generation(), 1);

    let shared = Arc::new(RegisteredFonts::default());
    let first = Manager::with_registered_fonts(FcFontCache::default(), Arc::clone(&shared))
        .expect("a manager");
    let mut second = Manager::new(FcFontCache::default()).expect("a manager");
    second.set_registered_fonts(Arc::clone(&shared));
    assert!(first.registered_fonts.register_font("Azul Manager Shared Face", bytes));
    assert_eq!(second.registered_fonts.generation(), 1, "the registry of both is one");

    // A manager made on the registry afterwards, and one that takes it, index the face.
    let norm = rust_fontconfig::utils::normalize_family_name("Azul Manager Shared Face");
    let late = Manager::with_registered_fonts(FcFontCache::default(), Arc::clone(&shared))
        .expect("a manager");
    assert!(late.memory_families.contains_key(&norm), "made after the registration");
    let mut taker = Manager::new(FcFontCache::default()).expect("a manager");
    assert!(!taker.memory_families.contains_key(&norm));
    taker.set_registered_fonts(shared);
    assert!(taker.memory_families.contains_key(&norm), "given the registry afterwards");
}

/// A8-12: asking again for the stack asked last builds no chain key, a
/// change of stack builds the new one's, and the key handed out is always
/// `FontChainKey::from_selectors` of the selectors asked. The memo is the manager's own: a
/// clone asks for itself.
/// Mutation: drop the `known` check (build every time) and the count is
/// 102, not 3; compare only the families and the two sizes share a key.
#[test]
fn a_stack_asked_again_keeps_its_chain_key() {
    let fonts = Manager::new(FcFontCache::default()).expect("a manager");
    let built = |fonts: &Manager| fonts.chain_key_memo.lock().expect("the memo").built;
    let (a, b) = (stack("Azul Mock Mono", 12), stack("Azul Mock Mono", 13));
    let before = built(&fonts);
    for _ in 0..100 {
        fonts.with_chain_key(&a, |key| assert_eq!(*key, FontChainKey::from_selectors(&a)));
    }
    assert_eq!(built(&fonts) - before, 1, "one stack asked 100 times: one key");
    fonts.with_chain_key(&b, |key| assert_eq!(*key, FontChainKey::from_selectors(&b)));
    fonts.with_chain_key(&a, |key| assert_eq!(*key, FontChainKey::from_selectors(&a)));
    assert_eq!(built(&fonts) - before, 3, "each change of stack builds the new one's key");

    let clone = fonts.clone_shared();
    assert_eq!(built(&clone), 0, "a clone's memo is its own and starts empty");
    clone.with_chain_key(&a, |key| assert_eq!(*key, FontChainKey::from_selectors(&a)));
    assert_eq!(built(&clone), 1);
    assert_eq!(built(&fonts) - before, 3, "what a clone is asked is no business of the manager");
}

/// A8-14: the built-in faces' styles are declared, and the declaration is the style a parse of
/// the face gives - what the font manager registers them with in place of the parse each
/// manager would pay for every face (a window, a text image, an SVG painter).
/// Mutation: a flipped `bold` or `italic` in the bundle's table, or a mock face that is not a
/// regular upright cut, fails it.
#[test]
fn the_built_in_faces_declare_the_style_a_parse_gives() {
    for (family, bytes) in crate::text3::mock_fonts::BUILTIN_MOCK_FONTS {
        assert_eq!(
            format!("{:?}", FaceStyle::default()),
            format!("{:?}", parse_face_style(bytes, family)),
            "{family}"
        );
    }
    for face in crate::text3::ui_fonts::bundled_ui_fonts() {
        assert_eq!(
            format!("{:?}", bundled_face_style(face)),
            format!("{:?}", parse_face_style(&face.bytes, face.family)),
            "{} (bold: {}, italic: {})",
            face.family,
            face.bold,
            face.italic
        );
    }
}

/// A8-15 (rust-fontconfig 5.1): the style of a face is read from its patterns alone: the
/// patterns of a parse of the bytes are those of the parse of the face's record.
#[test]
fn the_style_of_a_face_is_read_from_its_patterns_alone() {
    for (family, bytes) in crate::text3::mock_fonts::BUILTIN_MOCK_FONTS {
        let patterns = rust_fontconfig::FcParseFontPatterns(bytes).expect("parses");
        let records = rust_fontconfig::FcParseFontBytes(bytes, family).expect("parses");
        assert_eq!(patterns.len(), records.len());
        assert_eq!(format!("{:?}", patterns[0]), format!("{:?}", records[0].0));
    }
}
