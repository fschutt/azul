//! The app shell runs the animation engine's REBUILD half.
//!
//! Enter animations (`-azul-animation-in`), exit animations
//! (`-azul-animation-out`, a departing node kept as a zombie) and CSS
//! transitions a rebuild captures were implemented in
//! `LayoutWindow::begin_reconciliation` / `finish_reconciliation` - and only
//! the E2E runner called those. Every desktop shell goes through
//! `regenerate_layout` (the headless window here is the same code), which
//! hand-rolled its own reconcile with the FLIP moves only: in the real app a
//! spinner never spun, a panel never slid in or out, and a rebuilt width never
//! tweened. And even the runner started enter tracks only for the ROOT of a
//! mounted subtree, never on the first frame - so a spinner that is in the
//! first DOM, or inside a card that mounts, stood still everywhere.

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::{common::event::PlatformWindow, headless::HeadlessWindow};
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo, RelayoutReason},
    dom::Dom,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::AppConfig,
};
use azul_layout::{widgets::spinner::Spinner, window_state::WindowCreateOptions};
use rust_fontconfig::FcFontCache;

/// What the scenarios' layout callback renders.
#[derive(Clone, Copy)]
enum Scene {
    /// `body > Spinner`.
    Spinner,
    /// `body > div.card`, with a Spinner inside it when `shown`.
    CardWithSpinner { shown: bool },
    /// `body > div` whose `width` tweens (`animation: width 200ms linear`).
    Width { px: u32 },
    /// `body > div` with an exit animation, present while `shown`.
    Leaving { shown: bool },
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let scene = data
        .downcast_ref::<Scene>()
        .map_or(Scene::Spinner, |s| *s);
    let body = Dom::create_body();
    match scene {
        Scene::Spinner => body.with_child(Spinner::create().dom()),
        Scene::CardWithSpinner { shown } => {
            let card = Dom::create_div().with_css("width: 100px; height: 100px;");
            let card = if shown {
                card.with_child(Spinner::create().dom())
            } else {
                card
            };
            body.with_child(card)
        }
        Scene::Width { px } => body.with_child(Dom::create_div().with_css(&format!(
            "animation: width 200ms linear; width: {px}px; height: 10px;"
        ))),
        Scene::Leaving { shown } => {
            let (css, _) = azul_css::parser2::new_from_str(
                "@keyframes fade { from { opacity: 1; } to { opacity: 0; } }",
            );
            let leaving = Dom::create_div()
                .with_css("width: 50px; height: 50px; -azul-animation-out: fade 200ms linear;")
                .with_component_css(css);
            if shown {
                body.with_child(leaving)
            } else {
                body
            }
        }
    }
}

fn window(scene: Scene) -> HeadlessWindow {
    let mut options = WindowCreateOptions::default();
    options.window_state.size.dimensions.width = 400.0;
    options.window_state.size.dimensions.height = 300.0;
    let cb: extern "C" fn(RefAny, LayoutCallbackInfo) -> Dom = layout;
    options.window_state.layout_callback = LayoutCallback::create(cb);
    let mut window = HeadlessWindow::new(
        options,
        Arc::new(RefCell::new(RefAny::new(scene))),
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        AppConfig::default(),
        SharedIconProvider::from_handle(azul_core::icon::IconProviderHandle::default()),
        Arc::new(FcFontCache::default()),
        None,
    )
    .expect("a headless window");
    window.regenerate_layout().expect("the first layout");
    window
}

/// Change the scene and rebuild, the way an app's `RefreshDom` does.
fn rebuild(window: &mut HeadlessWindow, scene: Scene) {
    if let Some(mut s) = window.common.app_data.borrow_mut().downcast_mut::<Scene>() {
        *s = scene;
    };
    window.request_regeneration(RelayoutReason::RefreshDom);
    window.regenerate_layout().expect("the rebuild");
}

#[test]
fn a_spinner_in_the_first_frame_is_spinning() {
    let w = window(Scene::Spinner);
    let lw = w.get_layout_window().expect("a layout window");
    assert!(
        !lw.live_tracks.is_empty(),
        "the spinner declares a looping -azul-animation-in; its tracks must run from the first frame"
    );
}

#[test]
fn a_spinner_a_rebuild_mounts_inside_a_card_starts_spinning() {
    let mut w = window(Scene::CardWithSpinner { shown: false });
    assert!(
        w.get_layout_window().expect("a layout window").live_tracks.is_empty(),
        "premise: nothing animates before the spinner exists"
    );
    rebuild(&mut w, Scene::CardWithSpinner { shown: true });
    assert!(
        !w.get_layout_window().expect("a layout window").live_tracks.is_empty(),
        "the spinner mounted INSIDE the card (not the mounted subtree's root) must spin"
    );
}

#[test]
fn a_rebuild_that_changes_a_transitioned_width_tweens_it() {
    let mut w = window(Scene::Width { px: 100 });
    rebuild(&mut w, Scene::Width { px: 200 });
    assert!(
        !w.get_layout_window().expect("a layout window").css_transitions.is_empty(),
        "a rebuilt width under `animation: width 200ms` must tween in the app shell"
    );
}

#[test]
fn a_node_a_rebuild_removes_plays_its_exit_animation() {
    let mut w = window(Scene::Leaving { shown: true });
    rebuild(&mut w, Scene::Leaving { shown: false });
    assert!(
        !w.get_layout_window().expect("a layout window").zombies.is_empty(),
        "a departing node that declares -azul-animation-out must be kept animating out"
    );
}
