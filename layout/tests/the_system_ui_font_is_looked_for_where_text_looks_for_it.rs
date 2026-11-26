//! The check of the system UI font looks where the text looks: the `system-ui` generic family of
//! the platform, by the first family of its list - not the name the OS reports for its UI font.
//!
//! macOS answers `.AppleSystemUIFont` there, the private alias CoreText has for San Francisco: no
//! font file carries it in its name table (`SFNS.ttf` is "System Font"), so the check that looked
//! it up by that name warned, at every regeneration of the layout, that the system UI font was NOT
//! found by the scan - while the UI text was set in `system-ui`, in the face the scan had found.
//!
//! The faces are the mock face `azul-mock-kern.ttf` registered under the family names of the
//! platform lists (`css/src/system.rs`); the caches are empty but for them.

use azul_css::props::basic::FontRef;
use azul_layout::{
    font::loading::{browser_generic_families, check_system_ui_font, SystemUiFontCheck},
    text3::{cache::FontManager, mock_fonts::mock_font_ranges},
};
use rust_fontconfig::{FcFontCache, OperatingSystem};

use crate::text_rig::face_bytes;

/// A font manager over an empty cache that has the `system-ui` list of `os` and the faces
/// `families`.
fn manager(families: &[&str], os: OperatingSystem) -> FontManager<FontRef> {
    let mut fonts = FontManager::new(FcFontCache::default()).expect("a font manager");
    let bytes = face_bytes("azul-mock-kern.ttf");
    for family in families {
        fonts.register_named_font(family, &bytes, mock_font_ranges());
    }
    // after the manager is made: it sets the list of the machine the test runs on
    let mut config = fonts.fc_cache.fallback_config();
    browser_generic_families(&mut config, os);
    let _ = fonts.fc_cache.set_fallback_config(config);
    fonts
}

#[test]
fn the_alias_coretext_reports_is_no_name_to_look_up_on_macos() {
    let fonts = manager(&["System Font"], OperatingSystem::MacOS);
    assert_eq!(
        check_system_ui_font(&fonts.fc_cache, ".AppleSystemUIFont", OperatingSystem::MacOS),
        SystemUiFontCheck::Found
    );
}

#[test]
fn macos_without_system_font_says_which_family_the_list_starts_with() {
    // Helvetica Neue stands in (the list goes on with it), the font of the platform is missing
    let fonts = manager(&["Helvetica Neue"], OperatingSystem::MacOS);
    assert_eq!(
        check_system_ui_font(&fonts.fc_cache, ".AppleSystemUIFont", OperatingSystem::MacOS),
        SystemUiFontCheck::PrimaryMissing("System Font".to_string())
    );
}

#[test]
fn a_cache_without_any_family_of_the_list_resolves_nothing() {
    let fonts = manager(&[], OperatingSystem::MacOS);
    assert_eq!(
        check_system_ui_font(&fonts.fc_cache, ".AppleSystemUIFont", OperatingSystem::MacOS),
        SystemUiFontCheck::GenericMissing
    );
}

#[test]
fn on_linux_the_desktops_own_font_is_looked_up_by_its_name() {
    // Cantarell is the list's first family; the desktop's font heads the list at run time
    let fonts = manager(&["Cantarell"], OperatingSystem::Linux);
    assert_eq!(
        check_system_ui_font(&fonts.fc_cache, "Cantarell", OperatingSystem::Linux),
        SystemUiFontCheck::Found
    );
    assert_eq!(
        check_system_ui_font(&fonts.fc_cache, "Fancy Sans", OperatingSystem::Linux),
        SystemUiFontCheck::PrimaryMissing("Fancy Sans".to_string())
    );
}

/// Every window and every regeneration of an app asks for the check; only the first is let make
/// it, so the warning of a failed one is logged once.
#[test]
fn the_check_is_claimed_once_by_all_the_windows_of_an_app() {
    let fonts = manager(&[], OperatingSystem::MacOS);
    assert!(fonts.registered_fonts.claim_system_ui_font_check());
    assert!(!fonts.registered_fonts.claim_system_ui_font_check());
    let second_window = fonts.clone_shared();
    assert!(
        !second_window.registered_fonts.claim_system_ui_font_check(),
        "the windows of an app share the one registry"
    );
    // an app of its own has its own check
    let other_app = manager(&[], OperatingSystem::MacOS);
    assert!(other_app.registered_fonts.claim_system_ui_font_check());
}

/// On a macOS machine: the scan of the real system finds the face the platform draws its UI in,
/// and prints what it looks at (the run that settles whether the scan misses it).
#[cfg(target_os = "macos")]
#[test]
fn the_scan_of_this_macos_finds_the_system_ui_font() {
    use rust_fontconfig::{FcPattern, FcWeight, PatternMatch};
    let cache = azul_layout::font::loading::build_font_cache();
    let mut trace = Vec::new();
    for name in [".AppleSystemUIFont", "System Font", ".SF NS", "SF Pro", "San Francisco", "SFNS", "Helvetica Neue"] {
        let found = cache
            .query(&FcPattern { name: Some(name.to_string()), ..Default::default() }, &mut trace)
            .and_then(|m| cache.get_metadata_by_id(&m.id));
        eprintln!(
            "PROBE query name {name:?}: {:?}",
            found.map(|p| (p.name, p.family))
        );
    }
    let chain = cache.resolve_font_chain(
        &["system-ui".to_string()],
        FcWeight::Normal,
        PatternMatch::DontCare,
        PatternMatch::DontCare,
        &mut trace,
    );
    for group in &chain.css_fallbacks {
        let resolved: Vec<_> = group
            .fonts
            .iter()
            .take(3)
            .filter_map(|m| cache.get_metadata_by_id(&m.id).map(|p| (p.name, p.family)))
            .collect();
        eprintln!("PROBE system-ui group {:?}: {} fonts, first {:?}", group.css_name, group.fonts.len(), resolved);
    }
    let names: Vec<String> = cache
        .list()
        .into_iter()
        .filter_map(|(pattern, _)| pattern.name.or(pattern.family))
        .filter(|n| n.starts_with('.') || n.contains("SF") || n.contains("System"))
        .collect();
    eprintln!("PROBE names with a dot, SF or System: {names:?}");
    eprintln!("PROBE fonts in the cache: {}", cache.list().len());
    assert_eq!(
        check_system_ui_font(&cache, ".AppleSystemUIFont", OperatingSystem::MacOS),
        SystemUiFontCheck::Found
    );
}
