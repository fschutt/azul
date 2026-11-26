//! The handles the engine declares thread-safe by hand (`unsafe impl Send for X {}` and `unsafe impl Sync`)
//! are thread-safe in the generated Rust API too: a mirror struct that holds a raw pointer is neither, so an
//! app that moved a session to the thread of its microphone had to wrap it in its own `unsafe impl Send`.

use super::{generate_dll_dynamic, generate_dll_internal, generate_rust_public_api};
use crate::api::ApiData;

fn api() -> ApiData {
    ApiData::from_str(include_str!("../../../../api.json")).expect("api.json parses")
}

/// Whether `code` declares `unsafe impl <trait> for <type>` for the type under either of its names
/// (the C-ABI mirror's `Az` prefix, the public API's plain one).
fn declares(code: &str, trait_name: &str, name: &str) -> bool {
    code.contains(&format!("unsafe impl {trait_name} for Az{name} {{}}"))
        || code.contains(&format!("unsafe impl {trait_name} for {name} {{}}"))
}

/// The three generated Rust APIs: the engine's own mirror, the dynamic one and the public one.
fn generated() -> Vec<(&'static str, String)> {
    let api = api();
    vec![
        ("dll_internal", generate_dll_internal(&api).expect("the internal API generates")),
        ("dll_dynamic", generate_dll_dynamic(&api).expect("the dynamic API generates")),
        ("rust public api", generate_rust_public_api(&api).expect("the public API generates")),
    ]
}

#[test]
fn the_generated_apis_declare_the_speech_handles_send_and_sync() {
    for (what, code) in generated() {
        for handle in ["SpeechSession", "SpeechStream"] {
            for trait_name in ["Send", "Sync"] {
                assert!(declares(&code, trait_name, handle), "{what}: no `unsafe impl {trait_name}` for {handle}");
            }
        }
        // A type the engine does not declare thread-safe is not made so.
        for plain in ["SpeechWord", "SpeechLimits", "SpeechHeard"] {
            assert!(!declares(&code, "Send", plain), "{what}: {plain} was made Send");
            assert!(!declares(&code, "Sync", plain), "{what}: {plain} was made Sync");
        }
    }
}

/// The engine declares exactly what it means: `NodeData` is `Send` and not `Sync` there, an enum (the
/// window's raw handle) is declared like a struct, and the font handle that `dll/src/lib.rs` declared by
/// hand for the internal mirror comes out of the generator now.
#[test]
fn each_type_gets_the_traits_the_engine_declares_for_it_and_no_more() {
    for (what, code) in generated() {
        assert!(declares(&code, "Send", "NodeData"), "{what}: NodeData is not Send");
        assert!(!declares(&code, "Sync", "NodeData"), "{what}: NodeData was made Sync");
        assert!(declares(&code, "Send", "RawWindowHandle"), "{what}: the enum RawWindowHandle is not Send");
        for trait_name in ["Send", "Sync"] {
            assert!(declares(&code, trait_name, "FontRef"), "{what}: FontRef is not {trait_name}");
        }
    }
}

/// The reason travels with the impl: the generated comment names the engine's own declaration.
#[test]
fn the_generated_impl_says_whose_declaration_it_mirrors() {
    let code = generate_dll_dynamic(&api()).expect("the dynamic API generates");
    let at = code
        .find("unsafe impl Send for AzSpeechSession {}")
        .or_else(|| code.find("unsafe impl Send for SpeechSession {}"))
        .expect("SpeechSession is Send");
    let before = &code[at.saturating_sub(400)..at];
    assert!(before.contains("SAFETY"), "{before}");
    assert!(before.contains("azul_dll::unified::speech::SpeechSession"), "{before}");
}

/// Two impls of one trait for one type do not compile: the internal mirror of the font handle must be
/// declared once, by the generator.
#[test]
fn the_font_handle_is_declared_once_in_the_internal_mirror() {
    let code = generate_dll_internal(&api()).expect("the internal API generates");
    assert_eq!(code.matches("unsafe impl Send for AzFontRef {}").count(), 1);
    assert_eq!(code.matches("unsafe impl Sync for AzFontRef {}").count(), 1);
    let lib = include_str!("../../../../dll/src/lib.rs");
    assert!(!lib.contains("for dll::AzFontRef {}"), "dll/src/lib.rs still declares the font handle by hand");
}
