//! DOM transitions with **no timer and no animation state in the app**.
//!
//! Read `anim.rs` first, then this. They produce comparable motion from opposite
//! directions:
//!
//! * `anim.rs` installs a `Timer`, keeps a frame counter in app state, and
//!   computes a position from it every tick. The app owns the animation.
//! * this file has no timer, no counter, no interpolation and no notion of
//!   time. It returns a DIFFERENT DOM, and the engine animates the difference.
//!
//! That is the whole model: `layout()` stays a pure `f(&State) -> Dom`, the DOM
//! diff already computes which node became which, and any node whose rect moved
//! between the old and new layout gets a composited FLIP transform. The
//! application is **descriptive** — it says what the UI IS, never how it should
//! travel there.
//!
//! Three transitions, one per button, exercising different shapes of change:
//!
//! 1. **Sidebar** — a REAL unmount with a declared presence animation:
//!    `-azul-animation-out: sidebarFlyOut 0.5s` names a function the sidebar
//!    ATTACHED TO ITS OWN NODE (`with_animation_callback`) — the component
//!    ships its animation; there are no engine builtins and no global
//!    registry. The callback receives a full `TimerCallbackInfo` (the live
//!    dom, change queue, momentum API) plus the zombie info (raw `t`, the
//!    CSS-requested timing, the retained tree) and owns the easing math,
//!    while the content, laid out at its final width immediately, slides
//!    into the space. `-azul-animation-in: sidebarFlyIn` brings it back; a
//!    stylesheet `@keyframes` of the same name would shadow the function.
//! 2. **Screen swap** — two "pages" of an SPA. The header card is present in
//!    both but lands somewhere different, so it flies between positions instead
//!    of disappearing and reappearing. This is the case that needs
//!    reconciliation identity rather than tree position: the card is a
//!    different `NodeId` in the two DOMs.
//! 3. **Reorder** — the same list, shuffled. Every row is matched to its new
//!    slot and animates there, which is what makes a sort look like motion
//!    rather than a repaint.
//!
//! Run:
//!
//!     cargo run --release --example transitions
//!
//! Then click the buttons. The only mention of time in this file is the two
//! declarative `0.5s` durations in the sidebar's CSS.

use azul::dom::{IdOrClass, ZombieAnimCallback};
use azul::image::ZombieAnimInfo;
use azul::option::OptionF32;
use azul::prelude::*;
use azul::widgets::{Button, ZombieFrame};

/// Which demo screen is showing, and the toggles each one owns.
///
/// Note what is NOT here: no frame counter, no elapsed time, no "is animating"
/// flag, no per-node animation handles. This struct is the same size whether
/// something is moving or not.
struct AppState {
    sidebar_open: bool,
    screen: Screen,
    /// Row labels in their current order. Reordering means permuting this.
    rows: Vec<&'static str>,
    /// Bumped on each shuffle so the order visibly changes with one button.
    shuffles: usize,
}

#[derive(PartialEq, Clone, Copy)]
enum Screen {
    Overview,
    Detail,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sidebar_open: true,
            screen: Screen::Overview,
            rows: vec!["Alpha", "Bravo", "Charlie", "Delta", "Echo"],
            shuffles: 0,
        }
    }
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    background: #0e0e14; font-family: sans-serif;";
const TOOLBAR: &str = "display: flex; flex-direction: row; padding: 12px; \
    background: #16161f; border-bottom: 1px solid #2a2a3a;";
const BTN: &str = "padding: 8px 14px; margin-right: 10px; border-radius: 6px; \
    background: #2a2a3a; color: #e6e6f0; font-size: 14px;";
const BODY: &str = "display: flex; flex-direction: row; flex-grow: 1;";
// The presence animations belong to THE COMPONENT: the names below resolve
// to functions the sidebar attaches to its own node (see sidebar_fly_out /
// sidebar_fly_in). A stylesheet `@keyframes` of the same name would shadow
// them — the web mechanism is the only default name source.
const SIDEBAR_OPEN: &str = "width: 220px; background: #1b1b26; \
    border-right: 1px solid #2a2a3a; padding: 16px; display: flex; \
    flex-direction: column; -azul-animation-out: sidebarFlyOut 0.5s; \
    -azul-animation-in: sidebarFlyIn 0.5s;";

/// The sidebar's exit, shipped WITH the sidebar (USER ruling: no engine
/// builtins, no global registry — the component attaches its own animation
/// functions to its own node): slide left by our own width. `z.t` is RAW
/// linear progress and `z.timing` the CSS-requested curve — the callback
/// owns the easing math (`evaluate` honours it; a `cubic-bezier(...)` in
/// the CSS would arrive here too). The `TimerCallbackInfo` gives the
/// callback the LIVE dom, the change queue and the momentum API.
extern "C" fn sidebar_fly_out(
    _data: &mut RefAny,
    _live: &mut TimerCallbackInfo,
    z: &ZombieAnimInfo,
) -> ZombieFrame {
    ZombieFrame {
        translate_x: -z.rect.size.width * z.timing.evaluate(z.t),
        translate_y: 0.0,
        opacity: 1.0,
        width: OptionF32::None,
        clip_to_frozen_rect: true, // the slide must not paint over the body
    }
}

/// …and its entrance: the same path reversed.
extern "C" fn sidebar_fly_in(
    _data: &mut RefAny,
    _live: &mut TimerCallbackInfo,
    z: &ZombieAnimInfo,
) -> ZombieFrame {
    ZombieFrame {
        translate_x: -z.rect.size.width * (1.0 - z.timing.evaluate(z.t)),
        translate_y: 0.0,
        opacity: 1.0,
        width: OptionF32::None,
        clip_to_frozen_rect: true,
    }
}
const CONTENT: &str = "flex-grow: 1; padding: 24px; display: flex; \
    flex-direction: column;";
const CARD: &str = "background: #202030; border-radius: 10px; padding: 18px; \
    margin-bottom: 16px; color: #e6e6f0; font-size: 16px;";
const ROW: &str = "background: #23233a; border-radius: 8px; padding: 12px; \
    margin-bottom: 8px; color: #cfd2e0; font-size: 14px;";
const HINT: &str = "color: #6a7080; font-size: 12px; margin-top: 4px;";
const SIDE_ITEM: &str = "color: #9aa0b4; font-size: 13px; margin-bottom: 10px;";

extern "C" fn on_toggle_sidebar(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.sidebar_open = !s.sidebar_open;
    }
    // The app's entire contribution to the animation: "the DOM changed".
    Update::RefreshDom
}

extern "C" fn on_swap_screen(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        s.screen = match s.screen {
            Screen::Overview => Screen::Detail,
            Screen::Detail => Screen::Overview,
        };
    }
    Update::RefreshDom
}

extern "C" fn on_shuffle(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<AppState>() {
        // Deterministic rotation rather than a random shuffle: a demo that
        // reorders the same way every run is one you can actually eyeball, and
        // the e2e test can assert against it.
        s.rows.rotate_left(1);
        s.shuffles += 1;
    }
    Update::RefreshDom
}

fn button(label: &str, cb: extern "C" fn(RefAny, CallbackInfo) -> Update, data: &RefAny) -> Dom {
    let mut b = Button::create(label);
    b.set_on_click(data.clone(), cb);
    b.dom().with_css(BTN)
}

/// Give a node a stable identity for reconciliation, via its CSS id.
///
/// The diff matches on explicit key first and CSS id second. The FFI `Dom` the
/// bindings expose has NEITHER a `with_key` nor an id setter — only `NodeData`
/// does — so a node has to be built data-first to be identifiable at all. That
/// is a real gap for engine-driven transitions: without stable identity the
/// diff matches structurally, and a reordered list looks like five nodes whose
/// text changed rather than five nodes that moved.
fn div_with_id(id: &str, css: &str) -> Dom {
    let mut nd = NodeData::create_div();
    nd.set_ids_and_classes(vec![IdOrClass::Id(id.to_string().into())]);
    Dom::create_from_data(nd).with_css(css)
}

/// The card that exists on BOTH screens.
///
/// Same CSS id, so reconciliation matches it across the swap even though
/// it sits at a different depth and index in the two trees. Without a stable
/// identity the diff would call this "one node removed, one added" and there
/// would be nothing to animate BETWEEN.
fn shared_card(title: &str) -> Dom {
    let mut card = div_with_id("shared-card", CARD);
    card.add_child(Dom::create_div_with_text(title));
    card.add_child(Dom::create_div_with_text("same node, different screen").with_css(HINT));
    card
}

fn overview(state: &AppState) -> Dom {
    let mut content = Dom::create_div().with_css(CONTENT);
    content.add_child(shared_card("Overview"));
    for label in &state.rows {
        let mut row = div_with_id(label, ROW);
        row.add_child(Dom::create_p_with_text(*label));
        content.add_child(row);
    }
    content
}

fn detail(state: &AppState) -> Dom {
    let mut content = Dom::create_div().with_css(CONTENT);
    let mut spacer = Dom::create_div().with_css(
        "height: 90px; background: #191926; border-radius: 10px; \
         margin-bottom: 16px; padding: 14px; color: #6a7080; font-size: 13px;",
    );
    spacer.add_child(Dom::create_p_with_text("Detail header"));
    content.add_child(spacer);
    // The shared card is BELOW a header here and at the very top on Overview,
    // so swapping screens moves it — that displacement is the animation.
    content.add_child(shared_card("Detail"));
    let mut note = Dom::create_div().with_css(CARD);
    note.add_child(Dom::create_p_with_text(
        format!("Shuffles so far: {}", state.shuffles).as_str(),
    ));
    content.add_child(note);
    content
}

extern "C" fn layout(data: RefAny, _: LayoutCallbackInfo) -> Dom {
    let mut d = data.clone();
    let Some(state) = d.downcast_ref::<AppState>() else {
        return Dom::create_body();
    };

    let mut toolbar = Dom::create_div().with_css(TOOLBAR);
    toolbar.add_child(button("Toggle sidebar", on_toggle_sidebar, &data));
    toolbar.add_child(button("Swap screen", on_swap_screen, &data));
    toolbar.add_child(button("Reorder list", on_shuffle, &data));

    let mut body = Dom::create_div().with_css(BODY);
    // Transition 1 is a REAL unmount: closed means the node does not exist.
    // The engine retains the departing subtree and plays its declared
    // `-azul-animation-out`; the content area is laid out at its final width
    // immediately (text reflows once, no squash) while the exit paints on
    // top. Reopening mounts a fresh node, driven by `-azul-animation-in`.
    if state.sidebar_open {
        let mut sidebar = div_with_id("sidebar", SIDEBAR_OPEN)
            .with_animation_callback(
                "sidebarFlyOut",
                ZombieAnimCallback {
                    cb: sidebar_fly_out as usize,
                },
                RefAny::new(()),
            )
            .with_animation_callback(
                "sidebarFlyIn",
                ZombieAnimCallback {
                    cb: sidebar_fly_in as usize,
                },
                RefAny::new(()),
            );
        for item in ["Inbox", "Drafts", "Archive", "Trash"] {
            sidebar.add_child(Dom::create_div_with_text(item).with_css(SIDE_ITEM));
        }
        body.add_child(sidebar);
    }
    body.add_child(match state.screen {
        Screen::Overview => overview(&state),
        Screen::Detail => detail(&state),
    });

    let mut root = Dom::create_div().with_css(ROOT);
    root.add_child(toolbar);
    root.add_child(body);

    Dom::create_body().with_child(root)
}

fn main() {
    let data = RefAny::new(AppState::default());
    let app = App::create(data, AppConfig::create());
    let window = WindowCreateOptions::create(layout);
    app.run(window);
}
