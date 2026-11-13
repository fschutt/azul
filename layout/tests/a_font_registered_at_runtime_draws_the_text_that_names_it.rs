//! A font registered at runtime by family name draws the text that names
//! it, in a window that laid that text out before, and keeps doing so when
//! the window's font cache is replaced (T1: AzMail fetches a mail's remote
//! fonts after it shows the mail; `CallbackInfo::register_font`).
//!
//! The family is unknown when the window first lays the text out, so the
//! text falls back. Registering the face must reach every window: its font
//! chains are resolved again, and what it shaped with the fallback is not
//! reused.

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, font::parsed::ParsedFont,
    solver3::display_list::DisplayListItem, text3::fontreg, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// A family no other test registers: the registry is the process's.
const FAMILY: &str = "Azul Runtime Registered Face";

fn page() -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css(&format!(
                "font-family: \"{FAMILY}\", \"EB Garamond\", serif; font-size: 20px;"
            ))
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("Gardasee")),
    )
}

fn lay_out(lw: &mut LayoutWindow) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 100.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(page()),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
}

/// The face of every glyph the window paints.
fn faces(lw: &LayoutWindow) -> Vec<u64> {
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .flat_map(|item| match item {
            DisplayListItem::Text {
                glyphs, font_hash, ..
            } => vec![font_hash.font_hash; glyphs.len()],
            _ => Vec::new(),
        })
        .collect()
}

#[test]
fn a_font_registered_at_runtime_draws_the_text_that_names_it() {
    let bytes = azul_fonts_calmet::faces()[0].to_vec();
    let face = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
        .expect("the face parses")
        .hash;

    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw);
    let before = faces(&lw);
    assert!(!before.is_empty(), "the text paints in a fallback face first");
    assert!(
        before.iter().all(|h| *h != face),
        "harness: the family is not known yet"
    );

    assert!(
        fontreg::register_font(FAMILY, &bytes),
        "the face parses and is registered"
    );
    lay_out(&mut lw);
    let after = faces(&lw);
    assert!(
        !after.is_empty() && after.iter().all(|h| *h == face),
        "the same window draws the text in the registered face now: {after:x?}"
    );

    lw.font_manager.replace_fc_cache(FcFontCache::default());
    lay_out(&mut lw);
    let replaced = faces(&lw);
    assert!(
        !replaced.is_empty() && replaced.iter().all(|h| *h == face),
        "a replaced font cache keeps the runtime face: {replaced:x?}"
    );

    let mut fresh = LayoutWindow::new(FcFontCache::default()).expect("another window");
    lay_out(&mut fresh);
    assert!(
        faces(&fresh).iter().all(|h| *h == face),
        "a window opened later knows the face too"
    );
}

#[test]
fn bytes_that_are_no_font_are_refused() {
    assert!(!fontreg::register_font("Azul Not A Font", b"not a font"));
}
