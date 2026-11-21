//! The flora theme's icon pack (`IconProviderHandle::set_pack_condition`),
//! as azul-icons-haiku registers it: Haiku's icons under the Material names,
//! in a pack ranked first and conditioned on `theme=flora`. Resolving `inbox`
//! with the default resolver then draws Haiku's HVIF mail folder in a flora
//! window and Material's glyph in a flat one - and a theme switch between
//! two passes switches it (the window's context carries the theme chain;
//! the `SystemStyle` stays the same).

#![cfg(all(feature = "cpurender", feature = "text_layout"))]

use azul_core::{
    dom::{Dom, NodeType},
    icon::{resolve_icons_in_dom_with_context, IconMeta, SharedIconProvider},
};
use azul_css::{
    dynamic_selector::DynamicSelectorContext, props::basic::FontRef, system::SystemStyle,
};
use azul_layout::icon::{create_default_icon_provider, register_font_icon, register_hvif_icon};

/// Material's `inbox` codepoint (material-icons 0.3).
const INBOX_GLYPH: &str = "\u{e156}";

/// Haiku's mail folder, the icon azul-icons-haiku draws `inbox` as.
fn haiku_mail_folder() -> Vec<u8> {
    let path = format!(
        "{}/../examples/azul-icons-haiku/icons/Folder_mail.hvif",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {path}: {e}"))
}

/// A face nothing dereferences: resolving a glyph icon only NAMES its font
/// (the cascade shapes the glyph later), so no font file is needed here.
fn material_font() -> FontRef {
    static NO_FONT: u8 = 0;
    extern "C" fn keep(_: *mut core::ffi::c_void) {}
    FontRef::new(core::ptr::addr_of!(NO_FONT).cast::<core::ffi::c_void>(), keep)
}

/// A window's context under the app theme chain `names`.
fn window(names: &[&str]) -> DynamicSelectorContext {
    DynamicSelectorContext {
        theme_chain: azul_css::StringVec::from_vec(
            names
                .iter()
                .map(|t| azul_css::AzString::from((*t).to_string()))
                .collect(),
        ),
        ..Default::default()
    }
}

/// What `<icon>inbox</icon>` becomes in a window of the theme chain `names`.
fn inbox_in(shared: &SharedIconProvider, names: &[&str]) -> Dom {
    let mut dom = Dom::create_div().with_child(Dom::create_icon("inbox"));
    resolve_icons_in_dom_with_context(&mut dom, shared, &SystemStyle::default(), Some(&window(names)));
    dom
}

fn nodes(dom: &Dom) -> Vec<NodeType> {
    let mut out = vec![dom.root.get_node_type().clone()];
    for child in dom.children.as_ref() {
        out.extend(nodes(child));
    }
    out
}

/// Haiku's artwork: an HVIF icon resolves to the picture it draws.
fn draws_haiku(dom: &Dom) -> bool {
    nodes(dom).iter().any(|n| matches!(n, NodeType::Image(_)))
}

/// Material's artwork: a glyph icon resolves to its character.
fn glyph(dom: &Dom) -> Option<String> {
    nodes(dom).into_iter().find_map(|n| match n {
        NodeType::Text(t) => Some(t.as_str().to_string()),
        _ => None,
    })
}

#[test]
fn inbox_is_haikus_mail_folder_under_flora_and_materials_glyph_under_flat() {
    let mut provider = create_default_icon_provider();
    // As an app does it: its packs first, the Material icons after them
    // (`App::create` registers those).
    assert!(register_hvif_icon(&mut provider, "haiku", "inbox", &haiku_mail_folder(), IconMeta::for_image()));
    provider.set_pack_rank("haiku", 0);
    provider.set_pack_condition("haiku", "theme=flora");
    register_font_icon(&mut provider, "material-icons", "inbox", material_font(), INBOX_GLYPH);
    let shared = SharedIconProvider::from_handle(provider);

    let flora = inbox_in(&shared, &["flora", "flat"]);
    assert!(draws_haiku(&flora), "flora: Haiku's mail folder, drawn as a picture");
    assert_eq!(glyph(&flora), None, "flora: no Material glyph");

    let flat = inbox_in(&shared, &["flat"]);
    assert!(!draws_haiku(&flat), "flat: no Haiku artwork");
    assert_eq!(glyph(&flat).as_deref(), Some(INBOX_GLYPH), "flat: Material's inbox glyph");

    // Back to flora on the next pass: Haiku's again.
    assert!(draws_haiku(&inbox_in(&shared, &["flora", "flat"])));
}
