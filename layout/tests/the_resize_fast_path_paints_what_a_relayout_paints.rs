//! The resize fast path paints what a relayout of the same DOM at the same size
//! paints: AzMeet's lobby card and its devices panel, step by step through a
//! window resize.
//!
//! The user, on the Mac: "the <input> field in AzMeet sometimes resizes (the
//! input works, its just that it sometimes 'forgets' to stretch?)" and "the
//! 'statistics' in the AzMeet view sometimes break lines if I resize the
//! window". A desktop window resizes through the FAST path
//! (`IncrementalRelayout::Resize` -> `resize_only_hint`: the retained tree
//! with its warm per-node caches - taffy's measure and final-layout memo, the
//! pure-measure cache - and a PATCHED display list); the headless backend
//! resized through the RESTYLE relayout (a reconcile whose clone drops every
//! measurement, no patch), which paints both screens right at every size
//! (debug-server probes, scripts/fb1/azmeet_resize_probe.py). Two windows here
//! live the SAME history, one resizing like a desktop shell and one like the
//! restyle relayout; after every step their display lists must be the same
//! items (geometry rounded to 0.01 px, which only absorbs the float noise of
//! translating a spliced item by its node's delta).

use azul_core::{
    dom::{Dom, DomId},
    geom::LogicalSize,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// One window and how it resizes.
struct Side {
    lw: LayoutWindow,
    fast: bool,
    rr: RendererResources,
    cb: ExternalSystemCallbacks,
}

impl Side {
    fn new(fonts: &FcFontCache, fast: bool, dom: StyledDom, size: (f32, f32)) -> Self {
        let mut side = Self {
            lw: LayoutWindow::new(fonts.clone()).unwrap(),
            fast,
            rr: RendererResources::default(),
            cb: ExternalSystemCallbacks::rust_internal(),
        };
        side.lay_out(dom, size, false);
        // Settled, as a window's second frame.
        let again = side.take_dom();
        side.lay_out(again, size, false);
        side
    }

    fn take_dom(&mut self) -> StyledDom {
        self.lw
            .layout_results
            .remove(&DomId::ROOT_ID)
            .expect("root layout result")
            .styled_dom
    }

    fn lay_out(&mut self, dom: StyledDom, size: (f32, f32), resize_only: bool) {
        let mut ws: FullWindowState = self.lw.current_window_state.clone();
        ws.size.dimensions = LogicalSize::new(size.0, size.1);
        self.lw.layout_cache.resize_only_hint = resize_only;
        let mut dbg = None;
        self.lw
            .layout_and_generate_display_list(dom, &ws, &self.rr, &self.cb, &mut dbg)
            .unwrap();
        self.lw.current_window_state = ws;
    }

    /// A window resize: the SAME StyledDom at the new size, the way this side
    /// resizes.
    fn resize(&mut self, size: (f32, f32)) {
        let dom = self.take_dom();
        self.lay_out(dom, size, self.fast);
        if self.fast {
            assert!(
                self.lw.layout_cache.last_reconcile_was_skipped,
                "harness: the fast side must take the resize fast path"
            );
        }
    }

    /// The app rebuilt its DOM (AzMeet's statistics tick): a new StyledDom,
    /// laid out the ordinary way on both sides.
    fn rebuild(&mut self, dom: StyledDom) {
        let size = (
            self.lw.current_window_state.size.dimensions.width,
            self.lw.current_window_state.size.dimensions.height,
        );
        let _ = self.take_dom();
        self.lay_out(dom, size, false);
    }

    /// The display list the window paints, one line per item, geometry
    /// rounded to 0.01 px.
    fn painted(&self) -> Vec<String> {
        let dl = self
            .lw
            .layout_cache
            .cached_display_list
            .as_ref()
            .map(|(_, _, _, _, _, dl)| dl.clone())
            .expect("a laid-out window caches its display list");
        dl.items
            .iter()
            .zip(dl.node_mapping.iter())
            .map(|(item, node)| format!("{} @ {node:?}", rounded_numbers(&format!("{item:?}"))))
            .collect()
    }
}

/// `s` with every number rounded to two decimals (`-0.00` as `0.00`).
fn rounded_numbers(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev_is_word = i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        let starts = !prev_is_word
            && (c.is_ascii_digit()
                || (c == '-' && chars.get(i + 1).is_some_and(|d| d.is_ascii_digit())));
        if !starts {
            out.push(c);
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < chars.len()
            && (chars[i].is_ascii_digit()
                || chars[i] == '.'
                || chars[i] == 'e'
                || (chars[i] == '-' && chars[i - 1] == 'e'))
        {
            i += 1;
        }
        let token: String = chars[start..i].iter().collect();
        match token.parse::<f64>() {
            Ok(v) => {
                let r = format!("{v:.2}");
                out.push_str(if r == "-0.00" { "0.00" } else { &r });
            }
            Err(_) => out.push_str(&token),
        }
    }
    out
}

/// The first difference between the two sides' pictures, if any.
fn first_difference(fast: &Side, restyle: &Side) -> Option<String> {
    let a = fast.painted();
    let b = restyle.painted();
    if a == b {
        return None;
    }
    let i = a
        .iter()
        .zip(b.iter())
        .position(|(x, y)| x != y)
        .unwrap_or(a.len().min(b.len()));
    Some(format!(
        "{} vs {} items; first difference at item {i}:\n  fast path: {}\n  relayout:  {}",
        a.len(),
        b.len(),
        a.get(i).map_or("<none>", String::as_str),
        b.get(i).map_or("<none>", String::as_str),
    ))
}

fn run(name: &str, dom: impl Fn(usize) -> StyledDom, steps: &[(f32, f32)], rebuild_every: usize) {
    let fonts = FcFontCache::build();
    let mut fast = Side::new(&fonts, true, dom(0), steps[0]);
    let mut restyle = Side::new(&fonts, false, dom(0), steps[0]);
    if let Some(d) = first_difference(&fast, &restyle) {
        panic!("{name}: the two windows differ before any resize: {d}");
    }
    let mut failures = Vec::new();
    for (n, &size) in steps.iter().enumerate().skip(1) {
        fast.resize(size);
        restyle.resize(size);
        if let Some(d) = first_difference(&fast, &restyle) {
            failures.push(format!("step {n} ({} x {}): {d}", size.0, size.1));
        }
        if rebuild_every > 0 && n % rebuild_every == 0 {
            fast.rebuild(dom(n));
            restyle.rebuild(dom(n));
            if let Some(d) = first_difference(&fast, &restyle) {
                failures.push(format!("step {n} rebuilt ({} x {}): {d}", size.0, size.1));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{name}: the resize fast path paints what a relayout does not:\n{}",
        failures.join("\n")
    );
}

/// AzMeet's start screen (examples/azul-meet `start_layout`): a 520 px card of
/// labels, two REAL TextInputs (one in a row beside a Button, `flex-grow: 1`)
/// and a Button, centred in the window.
fn lobby(_tick: usize) -> StyledDom {
    use azul_layout::widgets::{button::Button, text_input::TextInput};

    let card = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: column; width: 520px; padding: 28px; \
             border-radius: 14px; background: #17171f;",
        )
        .with_child(
            Dom::create_span_with_text("AzMeet")
                .with_css("font-size: 26px; font-weight: bold; margin-bottom: 4px;"),
        )
        .with_child(
            Dom::create_span_with_text("Meeting server")
                .with_css("font-size: 13px; color: #8890a8; margin-bottom: 6px;"),
        )
        .with_child(
            TextInput::create()
                .with_text("http://127.0.0.1:8787".into())
                .with_placeholder("http://127.0.0.1:8787".into())
                .dom()
                .with_css("margin-bottom: 4px;"),
        )
        .with_child(
            Dom::create_span_with_text(
                "The meeting server at http://127.0.0.1:9 does not answer (Connection refused). \
                 Type another one and press Enter.",
            )
            .with_css("font-size: 12px; color: #f0b060; margin-bottom: 22px;"),
        )
        .with_child(
            Button::create("New meeting".into())
                .dom()
                .with_css("margin-bottom: 26px;"),
        )
        .with_child(
            Dom::create_span_with_text("Join with a link")
                .with_css("font-size: 14px; color: #ccd; margin-bottom: 6px;"),
        )
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(
                    TextInput::create()
                        .with_text("".into())
                        .with_placeholder("azlin://meet/... or a code like xq4-8kd-2nm".into())
                        .dom()
                        .with_css("flex-grow: 1; margin-right: 8px;"),
                )
                .with_child(Button::create("Join".into()).dom()),
        )
        .with_child(
            Dom::create_span_with_text("Others see you as fschutt")
                .with_css("margin-top: 22px; font-size: 13px; color: #8890a8;"),
        );
    let mut dom = Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0; background: \
             #0e0e14; font-family: sans-serif; color: #e6e6f0;",
        )
        .with_child(Dom::create_div().with_css("height: 28px; flex-shrink: 0;"))
        .with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; align-items: center; justify-content: center; flex-grow: 1; \
                     min-height: 0px;",
                )
                .with_child(card),
        );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

/// One column of AzMeet's devices panel (`device_col`).
fn device_col(title: &str, lines: &[String]) -> Dom {
    let mut col = Dom::create_div()
        .with_css("display: flex; flex-direction: column; margin: 0 28px;")
        .with_child(
            Dom::create_span_with_text(title)
                .with_css("font-size: 13px; color: #8890a8; margin-bottom: 4px;"),
        );
    for line in lines {
        col = col.with_child(
            Dom::create_span_with_text(line.as_str())
                .with_css("font-size: 13px; color: #ccd; padding: 2px 0;"),
        );
    }
    col
}

/// AzMeet's call view below the tiles (`call_layout`): a grid that takes the
/// room, the toolbar, and the devices panel - five columns of statistics in a
/// centred row; `tick` changes the counters as the app's statistics timer
/// does.
fn call(tick: usize) -> StyledDom {
    let packets = 1200 + 50 * tick;
    let panel = Dom::create_div()
        .with_css(
            "display: flex; justify-content: center; padding: 10px 12px 16px 12px; background: \
             #0e0e14; border-top: 1px solid #222;",
        )
        .with_child(device_col("Microphones", &["(none detected)".to_string()]))
        .with_child(device_col("Speakers", &["(none detected)".to_string()]))
        .with_child(device_col(
            "Video",
            &[
                "Video: H.264 (VideoToolbox)".to_string(),
                "iroh · waiting for others to join".to_string(),
                format!("sent {packets} frames at 320x180, 15 fps"),
            ],
        ))
        .with_child(device_col(
            "Audio",
            &[
                format!("Sending: 440 Hz test tone, 16-bit PCM, 20 ms packets, {packets} so far"),
                "Heard: nobody yet".to_string(),
            ],
        ))
        .with_child(device_col(
            "Network",
            &[
                "up 4.0 Mbps (estimated) · down 3.2 Mbps".to_string(),
                format!("RTT {} ms · loss 0.0 %", 12 + tick % 3),
            ],
        ));
    let toolbar = Dom::create_div()
        .with_css(
            "display: flex; flex-direction: row; justify-content: center; padding: 14px; \
             background: #15151c; flex-wrap: wrap;",
        )
        .with_child(Dom::create_span_with_text("Unmute mic").with_css(
            "padding: 10px 18px; margin: 0 6px; background: #3a3a4a; white-space: nowrap; \
             flex-shrink: 0;",
        ))
        .with_child(Dom::create_span_with_text("Start video").with_css(
            "padding: 10px 18px; margin: 0 6px; background: #3a3a4a; white-space: nowrap; \
             flex-shrink: 0;",
        ));
    let mut dom = Dom::create_body()
        .with_css(
            "display: flex; flex-direction: column; height: 100%; margin: 0; background: \
             #0e0e14; font-family: sans-serif; color: #e6e6f0;",
        )
        .with_child(
            Dom::create_span_with_text("AzMeet · meeting abc-def-ghi · fschutt")
                .with_css("padding: 12px; font-size: 18px; background: #15151c;"),
        )
        .with_child(
            Dom::create_div()
                .with_css("flex-grow: 1; min-height: 0px; display: flex; flex-wrap: wrap;"),
        )
        .with_child(toolbar)
        .with_child(panel);
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

#[test]
fn the_lobby_cards_inputs_stretch_on_the_resize_fast_path_as_on_a_relayout() {
    run(
        "lobby",
        lobby,
        &[
            (1100.0, 720.0),
            (900.0, 700.0),
            (700.0, 600.0),
            (576.0, 600.0),
            (560.0, 600.0),
            (500.0, 600.0),
            (420.0, 600.0),
            (350.0, 600.0),
            (420.0, 640.0),
            (560.0, 700.0),
            (700.0, 720.0),
            (900.0, 720.0),
            (1100.0, 720.0),
            (300.0, 720.0),
            (1100.0, 720.0),
            (1300.0, 800.0),
            (1100.0, 640.0),
            (1100.0, 720.0),
        ],
        0,
    );
}

#[test]
fn the_statistics_wrap_on_the_resize_fast_path_as_on_a_relayout() {
    run(
        "devices panel",
        call,
        &[
            (1100.0, 720.0),
            (1000.0, 720.0),
            (900.0, 700.0),
            (800.0, 700.0),
            (760.0, 680.0),
            (700.0, 650.0),
            (640.0, 650.0),
            (600.0, 640.0),
            (560.0, 620.0),
            (520.0, 620.0),
            (480.0, 620.0),
            (520.0, 620.0),
            (560.0, 620.0),
            (600.0, 640.0),
            (700.0, 700.0),
            (900.0, 720.0),
            (1100.0, 720.0),
            (620.0, 720.0),
            (1100.0, 720.0),
            (1100.0, 600.0),
            (1100.0, 720.0),
        ],
        0,
    );
}

#[test]
fn the_statistics_wrap_on_the_resize_fast_path_between_statistics_ticks() {
    // The app's statistics timer rebuilds the DOM between resizes, as AzMeet's
    // does every second while the window is dragged.
    run(
        "devices panel with ticks",
        call,
        &[
            (1100.0, 720.0),
            (900.0, 700.0),
            (700.0, 650.0),
            (640.0, 650.0),
            (600.0, 640.0),
            (560.0, 620.0),
            (520.0, 620.0),
            (560.0, 620.0),
            (640.0, 650.0),
            (800.0, 700.0),
            (1100.0, 720.0),
        ],
        2,
    );
}
