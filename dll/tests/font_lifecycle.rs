//! A font no display list draws for a few frames is deleted from the
//! renderer, as an image is (`image_lifecycle.rs`).
//!
//! Before, a window kept every font it had ever drawn - registered in
//! `RendererResources` and in WebRender for the life of the window - so a
//! document that brings its own fonts (the pages of a PDF: each page's SVG
//! embeds its fonts) grew the renderer's fonts with every page scrolled past.

use std::{
    cell::RefCell,
    collections::BTreeMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul::desktop::{
    shell2::{common::PlatformWindow, headless::HeadlessWindow},
    wr_translate2::{collect_font_resource_updates, collect_stale_font_deletes},
};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::Dom,
    icon::{IconProviderHandle, SharedIconProvider},
    refany::RefAny,
    resources::{AppConfig, DpiScaleFactor, ResourceUpdate},
};
use azul_css::props::basic::FloatValue;
use azul_layout::{text3::cache::ParsedFontTrait as _, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

#[derive(Clone)]
struct Ctx {
    show_text: Arc<AtomicBool>,
}

extern "C" fn layout_cb(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let show = data
        .downcast_ref::<Ctx>()
        .is_some_and(|ctx| ctx.show_text.load(Ordering::SeqCst));
    if show {
        Dom::create_body().with_child(Dom::create_p_with_text("Hello"))
    } else {
        Dom::create_body()
    }
}

fn make_window(ctx: Ctx) -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.layout_callback = LayoutCallback {
        cb: layout_cb,
        ctx: azul_core::refany::OptionRefAny::None,
    };
    HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(ctx))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("HeadlessWindow construction must succeed")
}

#[test]
fn a_font_no_longer_drawn_is_deleted_after_the_retention_window() {
    let show_text = Arc::new(AtomicBool::new(true));
    let mut window = make_window(Ctx {
        show_text: show_text.clone(),
    });
    let dpi = DpiScaleFactor {
        inner: FloatValue::new(1.0),
    };

    // Frame 0: the text is drawn. Register its font as
    // `register_frame_resources` does (the harness runs no WebRender
    // transaction).
    window.regenerate_layout().expect("frame 0");
    let (font_hash, font_key, instance_keys) = {
        let lw = window.common.layout_window.as_mut().expect("layout_window");
        let (updates, live) = collect_font_resource_updates(lw, &lw.renderer_resources, dpi);
        assert_eq!(live.len(), 1, "one font draws the text: {live:?}");
        let rr = &mut lw.renderer_resources;
        let mut font = None;
        let mut instances = Vec::new();
        for update in &updates {
            match update {
                ResourceUpdate::AddFont(add) => {
                    rr.font_hash_map.insert(add.font.get_hash(), add.key);
                    rr.currently_registered_fonts
                        .entry(add.key)
                        .or_insert_with(|| (add.font.clone(), BTreeMap::default().into()));
                    font = Some((add.font.get_hash(), add.key));
                }
                ResourceUpdate::AddFontInstance(add) => {
                    if let Some((_, registered)) =
                        rr.currently_registered_fonts.get_mut(&add.font_key)
                    {
                        registered.insert(add.glyph_size, add.key);
                    }
                    instances.push(add.key);
                }
                _ => {}
            }
        }
        let (hash, key) = font.expect("the font is added");
        assert!(!instances.is_empty(), "an instance of it is added");
        assert!(
            collect_stale_font_deletes(lw, &live).is_empty(),
            "a drawn font is not deleted"
        );
        (hash, key, instances)
    };

    // The text goes: no display list draws the font any more.
    show_text.store(false, Ordering::SeqCst);
    window.regenerate_layout().expect("frame 1");
    let empty = azul_core::FastBTreeSet::new();
    for _ in 0..2 {
        let lw = window.common.layout_window.as_mut().expect("layout_window");
        lw.epoch.increment();
        assert!(
            collect_stale_font_deletes(lw, &empty).is_empty(),
            "the font survives the retention window"
        );
        assert!(lw.renderer_resources.font_hash_map.contains_key(&font_hash));
    }

    // One frame past it: its instances, then the font.
    let lw = window.common.layout_window.as_mut().expect("layout_window");
    lw.epoch.increment();
    let deletes = collect_stale_font_deletes(lw, &empty);
    let mut expected: Vec<ResourceUpdate> = instance_keys
        .iter()
        .map(|key| ResourceUpdate::DeleteFontInstance(*key))
        .collect();
    expected.push(ResourceUpdate::DeleteFont(font_key));
    assert_eq!(deletes, expected, "its instances, then the font");
    let rr = &lw.renderer_resources;
    assert!(!rr.font_hash_map.contains_key(&font_hash), "evicted from the hash map");
    assert!(
        !rr.currently_registered_fonts.contains_key(&font_key),
        "evicted from the registered fonts"
    );
    assert!(
        !rr.font_last_seen_epoch.contains_key(&font_hash),
        "evicted from the gc map"
    );
}
