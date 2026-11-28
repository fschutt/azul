//! The AzPaint toolbar fits a phone and its keys are touch targets.
//!
//! A phone has no menu bar, so the commands the desktop's menus hold (undo, redo, clear, the effect, export,
//! settings) are six icon keys in a row under the canvas. At 360 dp (the narrowest phone this checks) the row
//! must not run out of the screen, and every key is 48 dp in each direction on Android.
//!
//! The app is an example crate this one cannot link, so this reads its SOURCE, the way
//! `azul_widgets_video_card_fits_a_phone.rs` does: the row's style is taken from
//! `examples/azul-paint/src/lib.rs`, the keys are the engine's own Buttons.

use std::sync::Arc;

use azul_core::{
    dom::{Dom, DomId},
    geom::{LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_css::{system::defaults, AzString};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, widgets::button::Button, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `examples/azul-paint/src/lib.rs`, verbatim, at compile time.
const PAINT: &str = include_str!("../../examples/azul-paint/src/lib.rs");

/// The value of `const <name>: &str = "...";` as the compiler reads it.
fn const_css(name: &str) -> String {
    let head = format!("const {name}: &str =");
    let start = PAINT.find(&head).unwrap_or_else(|| panic!("lib.rs has `{head}`"));
    let rest = &PAINT[start + head.len()..];
    let open = rest.find('"').expect("the constant is a string literal");
    let mut out = String::new();
    let mut chars = rest[open + 1..].chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => return out,
            '\\' => match chars.next() {
                Some('\n') => {
                    while chars.peek().is_some_and(|n| n.is_whitespace()) {
                        chars.next();
                    }
                }
                Some(other) => out.push(other),
                None => break,
            },
            _ => out.push(c),
        }
    }
    panic!("the literal of {name} does not end");
}

/// The six icons of `fn toolbar` (undo, redo, delete, auto_fix_high, save_alt, settings).
const KEYS: [&str; 6] = ["undo", "redo", "delete", "auto_fix_high", "save_alt", "settings"];

/// body > bar > six keys, in a window `width` wide with the style of an Android phone.
fn lay_out(width: f32) -> LayoutWindow {
    let mut bar = Dom::create_div().with_css(&const_css("TOOLBAR"));
    for icon in KEYS {
        bar.add_child(
            Button::create(AzString::from(""))
                .with_icon(AzString::from(icon))
                .dom()
                .with_css("flex-shrink: 0;"),
        );
    }
    let dom = Dom::create_body()
        .with_css("margin: 0px; width: 100%; height: 100%; display: flex; flex-direction: column; justify-content: flex-end;")
        .with_child(bar);
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lw.set_system_style(Arc::new(defaults::android_material_light()));
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(width, 800.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        StyledDom::create_from_dom(dom),
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("layout");
    lw
}

/// Every laid-out node, by DOM index (body 0, bar 1, then each key and its children).
fn rects(w: &LayoutWindow) -> Vec<(usize, LogicalRect)> {
    let lr = w.layout_results.get(&DomId::ROOT_ID).expect("root layout");
    let mut out = Vec::new();
    for (dom_node, indices) in lr.layout_tree.dom_to_layout.iter() {
        for idx in indices {
            let hot = lr.layout_tree.get(*idx).expect("layout node");
            if let Some(size) = hot.used_size {
                out.push((
                    dom_node.index(),
                    LogicalRect { origin: lr.calculated_positions[idx.index()], size },
                ));
            }
        }
    }
    out.sort_by_key(|(node, _)| *node);
    out
}

#[test]
fn the_six_keys_fit_the_narrowest_phone_and_are_48_dp_each() {
    for width in [360.0, 393.0, 411.0] {
        let w = lay_out(width);
        let all = rects(&w);
        let bar = all.iter().find(|(node, _)| *node == 1).expect("the bar").1;
        assert!(
            bar.origin.x >= 0.0 && bar.origin.x + bar.size.width <= width + 0.5,
            "{width} dp: the bar runs from {} to {}",
            bar.origin.x,
            bar.origin.x + bar.size.width
        );
        // the keys are the bar's children: the nodes whose box is a key-sized square inside the bar
        let keys: Vec<LogicalRect> = all
            .iter()
            .map(|(_, r)| *r)
            .filter(|r| {
                r.origin.y >= bar.origin.y - 0.5
                    && r.origin.y + r.size.height <= bar.origin.y + bar.size.height + 0.5
                    && r.size.width >= 47.9
                    && r.size.width <= 80.0
                    && r.size.height >= 47.9
            })
            .collect();
        assert!(keys.len() >= KEYS.len(), "{width} dp: {} key-sized boxes in the bar: {keys:?}", keys.len());
        for key in &keys {
            assert!(
                key.origin.x >= -0.5 && key.origin.x + key.size.width <= width + 0.5,
                "{width} dp: a key from {} to {} crosses the screen's edge",
                key.origin.x,
                key.origin.x + key.size.width
            );
        }
    }
}
