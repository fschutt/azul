// Headless, JSON-driven end-to-end test harness for `azul-layout`.
//
// This mirrors the DLL debug-server E2E workflow (`/e2e/*.json`) but runs
// ENTIRELY inside the layout crate: no WebRender, no GL, no DLL. Every op maps
// directly onto `LayoutWindow` methods, reproducing the effect the DLL's
// op-handlers (`dll/.../debug_server/full.rs`) and its event loop
// (`dll/.../common/event.rs`) achieve, but against the headless engine only.
//
// The point is coverage: the DLL owns the platform event loop, so the many
// cold paths in `layout/src/window.rs` (mount+first-layout, scroll, tab-focus,
// resize/dpi, focus/blur, get-dom) were never reached by a plain `cargo test`.
// This harness reaches them from a JSON scenario, the same way the debug
// server does at runtime.
//
// Fixtures live under `layout/tests/e2e_fixtures/*.json` and are auto-discovered
// by the single `#[test] fn run_all_e2e_fixtures`. They are stripped from the
// published crate by `exclude = ["tests", ...]` in `layout/Cargo.toml`.

use std::collections::BTreeMap;
use std::path::PathBuf;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize, PhysicalPositionI32},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    window::{VirtualKeyCode, VirtualKeyCodeVec, WindowPosition},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;
use serde::Deserialize;

// =========================================================================
// Scenario / Step schema (matches the /e2e JSON corpus)
// =========================================================================

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    #[serde(default)]
    #[allow(dead_code)]
    description: String,
    #[serde(default)]
    setup: Setup,
    steps: Vec<Step>,
}

#[derive(Debug, Deserialize)]
struct Setup {
    #[serde(default = "default_width")]
    window_width: f32,
    #[serde(default = "default_height")]
    window_height: f32,
    #[serde(default = "default_dpi")]
    dpi: u32,
}

impl Default for Setup {
    fn default() -> Self {
        Self {
            window_width: default_width(),
            window_height: default_height(),
            dpi: default_dpi(),
        }
    }
}

fn default_width() -> f32 {
    400.0
}
fn default_height() -> f32 {
    300.0
}
fn default_dpi() -> u32 {
    96
}

#[derive(Debug, Deserialize, Default, Clone)]
struct Modifiers {
    #[serde(default)]
    shift: bool,
    #[serde(default)]
    ctrl: bool,
    #[serde(default)]
    alt: bool,
    #[serde(default)]
    meta: bool,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Step {
    /// Mount an HTML+CSS document (line arrays) and run first layout.
    Mount {
        #[serde(default)]
        html: Vec<String>,
        #[serde(default)]
        css: Vec<String>,
    },
    /// No-op in headless mode (there is no async frame loop to await).
    WaitFrame,
    /// No-op in headless mode (kept for corpus compatibility).
    Wait {
        #[serde(default)]
        #[allow(dead_code)]
        ms: u64,
    },
    /// Press a key (sets keyboard state + runs keyboard default actions).
    KeyDown {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    /// Release a key.
    KeyUp {
        key: String,
        #[serde(default)]
        modifiers: Modifiers,
    },
    /// Scroll a node by a delta.
    ScrollNodeBy {
        selector: String,
        #[serde(default)]
        delta_x: f32,
        #[serde(default)]
        delta_y: f32,
    },
    /// Scroll a node to an absolute offset.
    ScrollNodeTo {
        selector: String,
        #[serde(default)]
        x: f32,
        #[serde(default)]
        y: f32,
    },
    /// Bring a node into view within its scroll container.
    ScrollIntoView {
        selector: String,
        #[serde(default)]
        block: Option<String>,
    },
    /// Focus a specific node by selector.
    Focus {
        #[serde(default)]
        selector: Option<String>,
    },
    /// Blur (remove window focus, or clear node focus if no selector logic).
    Blur,
    /// Move the window to (x, y).
    Move {
        x: i32,
        y: i32,
    },
    /// Resize the window (logical px) and re-layout.
    Resize {
        width: f32,
        height: f32,
    },
    /// Change DPI and re-layout.
    DpiChanged {
        dpi: u32,
    },
    /// Query the focused node → stored as the "last response".
    GetFocusState,
    /// Query the DOM → stored as the "last response".
    GetDom,

    // ---- assertions (panic on mismatch = test failure) ----
    AssertResponse {
        #[serde(rename = "type")]
        #[serde(default)]
        ty: Option<String>,
        #[serde(default)]
        contains: Option<String>,
    },
    AssertScroll {
        selector: String,
        #[serde(default)]
        x: Option<f32>,
        #[serde(default)]
        y: Option<f32>,
        #[serde(default = "one")]
        tolerance: f32,
    },
    AssertExists {
        selector: String,
    },
    AssertNotExists {
        selector: String,
    },
    AssertNodeCount {
        selector: String,
        expected: usize,
    },
    AssertLayout {
        selector: String,
        property: String,
        expected: f32,
        #[serde(default = "one")]
        tolerance: f32,
    },
    AssertWindowState {
        property: String,
        #[serde(default)]
        expected: serde_json::Value,
        #[serde(default = "small_tol")]
        tolerance: f32,
    },
    AssertDom {
        #[serde(default)]
        min_node_count: Option<usize>,
        #[serde(default)]
        contains: Option<String>,
        #[serde(default)]
        not_contains: Option<String>,
    },
}

fn one() -> f32 {
    1.0
}
fn small_tol() -> f32 {
    0.01
}

// =========================================================================
// Stored query response (for get_focus_state / get_dom + assert_response)
// =========================================================================

#[derive(Debug, Clone)]
enum Response {
    None,
    FocusState(String),
    Dom(String),
}

impl Response {
    fn type_tag(&self) -> &'static str {
        match self {
            Response::None => "none",
            Response::FocusState(_) => "focus_state",
            Response::Dom(_) => "dom",
        }
    }
    fn text(&self) -> &str {
        match self {
            Response::None => "",
            Response::FocusState(s) | Response::Dom(s) => s.as_str(),
        }
    }
}

// =========================================================================
// Harness — a headless LayoutWindow driven op-by-op
// =========================================================================

struct E2eHarness {
    layout_window: LayoutWindow,
    renderer_resources: RendererResources,
    system_callbacks: ExternalSystemCallbacks,
    window_state: FullWindowState,
    /// The last-mounted styled DOM, kept so resize/dpi can re-layout it.
    mounted_dom: Option<StyledDom>,
    /// Result of the most recent query op.
    last_response: Response,
}

impl E2eHarness {
    fn new(width: f32, height: f32, dpi: u32) -> Self {
        let font_cache = FcFontCache::build();
        let mut ws = FullWindowState::default();
        ws.size.dimensions = LogicalSize::new(width, height);
        ws.size.dpi = dpi;
        Self {
            layout_window: LayoutWindow::new(font_cache).expect("LayoutWindow::new"),
            renderer_resources: RendererResources::default(),
            system_callbacks: ExternalSystemCallbacks::rust_internal(),
            window_state: ws,
            mounted_dom: None,
            last_response: Response::None,
        }
    }

    fn now(&self) -> azul_core::task::Instant {
        (self.system_callbacks.get_system_time_fn.cb)()
    }

    /// Run the full layout pipeline for `styled_dom` and rebuild derived state.
    fn layout(&mut self, styled_dom: StyledDom) {
        let mut dbg = Some(Vec::new());
        self.layout_window
            .layout_and_generate_display_list(
                styled_dom,
                &self.window_state,
                &self.renderer_resources,
                &self.system_callbacks,
                &mut dbg,
            )
            .expect("layout_and_generate_display_list");
        self.register_scroll_nodes();
    }

    /// Port of the DLL's `register_scroll_nodes` (dll/.../common/layout.rs):
    /// after layout, push each scrollable container's container/content bounds
    /// into the ScrollManager so scroll clamping + reads work.
    fn register_scroll_nodes(&mut self) {
        let now = self.now();
        let lw = &mut self.layout_window;
        // Collect first (immutable borrow of layout_results), apply after.
        let mut regs: Vec<(DomId, NodeId, LogicalRect, LogicalSize, f32, f32, bool, bool)> =
            Vec::new();
        for (dom_id, layout_result) in &lw.layout_results {
            for (node_idx, node) in layout_result.layout_tree.nodes.iter().enumerate() {
                let Some(sb) = layout_result
                    .layout_tree
                    .warm(node_idx)
                    .and_then(|w| w.scrollbar_info.as_ref())
                else {
                    continue;
                };
                if !(sb.needs_vertical || sb.needs_horizontal) {
                    continue;
                }
                let Some(dom_node_id) = node.dom_node_id else {
                    continue;
                };
                let border_box_size = node.used_size.unwrap_or_default();
                let resolved = node.box_props.unpack();
                let border = &resolved.border;
                let container_size = LogicalSize {
                    width: (border_box_size.width - border.left - border.right).max(0.0),
                    height: (border_box_size.height - border.top - border.bottom).max(0.0),
                };
                let container_origin = layout_result
                    .calculated_positions
                    .get(node_idx)
                    .copied()
                    .unwrap_or_else(LogicalPosition::zero);
                let container_rect = LogicalRect {
                    origin: container_origin,
                    size: container_size,
                };
                let content_size = layout_result.layout_tree.get_content_size(node_idx);
                let thickness = sb.scrollbar_width.max(sb.scrollbar_height);
                regs.push((
                    *dom_id,
                    dom_node_id,
                    container_rect,
                    content_size,
                    thickness,
                    sb.visual_width_px,
                    sb.needs_horizontal,
                    sb.needs_vertical,
                ));
            }
        }
        for (dom_id, node_id, container_rect, content_size, thickness, vis, h, v) in regs {
            lw.scroll_manager.register_or_update_scroll_node(
                dom_id,
                node_id,
                container_rect,
                content_size,
                now.clone(),
                thickness,
                vis,
                h,
                v,
            );
        }
        lw.scroll_manager.calculate_scrollbar_states();
    }

    fn root_layout(&self) -> Option<&azul_layout::window::DomLayoutResult> {
        self.layout_window.layout_results.get(&DomId { inner: 0 })
    }

    /// Resolve a CSS selector to the first matching NodeId (like the DLL's
    /// `resolve_node_target`).
    fn resolve_selector_first(&self, selector: &str) -> Option<NodeId> {
        self.resolve_selector_all(selector).into_iter().next()
    }

    /// Resolve a CSS selector to all matching NodeIds.
    fn resolve_selector_all(&self, selector: &str) -> Vec<NodeId> {
        use azul_core::style::matches_html_element;
        use azul_css::parser2::parse_css_path;

        let Some(lr) = self.root_layout() else {
            return Vec::new();
        };
        let Ok(css_path) = parse_css_path(selector) else {
            return Vec::new();
        };
        let styled_dom = &lr.styled_dom;
        let node_hierarchy = styled_dom.node_hierarchy.as_container();
        let node_data = styled_dom.node_data.as_container();
        let cascade_info = styled_dom.cascade_info.as_container();
        let mut out = Vec::new();
        for i in 0..node_data.len() {
            let nid = NodeId::new(i);
            if matches_html_element(&css_path, nid, &node_hierarchy, &node_data, &cascade_info, None)
            {
                out.push(nid);
            }
        }
        out
    }

    fn dom_node_id(node: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from(Some(node)),
        }
    }

    // -----------------------------------------------------------------
    // Op handlers
    // -----------------------------------------------------------------

    fn op_mount(&mut self, html: &[String], css: &[String]) {
        let xml = build_mount_document(html, css);
        let styled_dom =
            azul_layout::xml::parse_xml_to_styled_dom(&xml).expect("mount: XML parse error");
        self.mounted_dom = Some(styled_dom.clone());
        self.layout(styled_dom);
    }

    fn op_key(&mut self, key: &str, mods: &Modifiers, down: bool) {
        let kc = parse_key(key);
        let mut pressed: Vec<VirtualKeyCode> = self
            .window_state
            .keyboard_state
            .pressed_virtual_keycodes
            .iter()
            .copied()
            .collect();
        if down {
            if let Some(kc) = kc {
                if !pressed.contains(&kc) {
                    pressed.push(kc);
                }
                self.window_state.keyboard_state.current_virtual_keycode = Some(kc).into();
            }
            if mods.shift && !pressed.contains(&VirtualKeyCode::LShift) {
                pressed.push(VirtualKeyCode::LShift);
            }
            if mods.ctrl && !pressed.contains(&VirtualKeyCode::LControl) {
                pressed.push(VirtualKeyCode::LControl);
            }
            if mods.alt && !pressed.contains(&VirtualKeyCode::LAlt) {
                pressed.push(VirtualKeyCode::LAlt);
            }
            if mods.meta && !pressed.contains(&VirtualKeyCode::LWin) {
                pressed.push(VirtualKeyCode::LWin);
            }
        } else {
            if let Some(kc) = kc {
                pressed.retain(|k| *k != kc);
            }
            self.window_state.keyboard_state.current_virtual_keycode = None.into();
            if !mods.shift {
                pressed.retain(|k| {
                    *k != VirtualKeyCode::LShift && *k != VirtualKeyCode::RShift
                });
            }
        }
        self.window_state.keyboard_state.pressed_virtual_keycodes =
            VirtualKeyCodeVec::from_vec(pressed);

        if down {
            self.run_keyboard_default_action();
        }
    }

    /// Mirrors the DLL event loop's keyboard-default-action pass
    /// (dll/.../common/event.rs ~5000): Tab → FocusNext/Previous → resolve →
    /// set_focused_node, Escape → ClearFocus.
    fn run_keyboard_default_action(&mut self) {
        use azul_core::events::DefaultAction;
        use azul_layout::default_actions::{
            default_action_to_focus_target, determine_keyboard_default_action,
        };
        use azul_layout::managers::focus_cursor::resolve_focus_target;
        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;

        let ks = self.window_state.keyboard_state.clone();
        let now = self.now();
        let lw = &mut self.layout_window;
        let focused = lw.focus_manager.get_focused_node().copied();
        let result =
            determine_keyboard_default_action(&ks, focused, &lw.layout_results, false);
        if !result.has_action() {
            lw.finalize_pending_focus_changes();
            return;
        }

        // Resolve into owned values (drops the immutable borrow of layout_results).
        let mut new_focus: Option<DomNodeId> = None;
        let mut do_clear = false;
        match &result.action {
            DefaultAction::FocusNext
            | DefaultAction::FocusPrevious
            | DefaultAction::FocusFirst
            | DefaultAction::FocusLast => {
                if let Some(target) = default_action_to_focus_target(&result.action) {
                    if let Ok(resolved) =
                        resolve_focus_target(&target, &lw.layout_results, focused)
                    {
                        new_focus = resolved;
                    }
                }
            }
            DefaultAction::ClearFocus => do_clear = true,
            _ => {}
        }

        if let Some(nf) = new_focus {
            lw.focus_manager.set_focused_node(Some(nf));
            lw.scroll_node_into_view(nf, ScrollIntoViewOptions::nearest(), now);
        } else if do_clear {
            lw.focus_manager.set_focused_node(None);
        }
        lw.finalize_pending_focus_changes();
    }

    fn op_scroll_by(&mut self, selector: &str, dx: f32, dy: f32) {
        let node = self
            .resolve_selector_first(selector)
            .unwrap_or_else(|| panic!("scroll_node_by: no node for selector {selector}"));
        let dom_id = DomId { inner: 0 };
        let now = self.now();
        let current = self
            .layout_window
            .scroll_manager
            .get_current_offset(dom_id, node)
            .unwrap_or_default();
        let target = LogicalPosition {
            x: current.x + dx,
            y: current.y + dy,
        };
        self.layout_window.scroll_manager.scroll_to(
            dom_id,
            node,
            target,
            std::time::Duration::from_millis(0).into(),
            azul_core::events::EasingFunction::Linear,
            now,
        );
        self.layout_window.scroll_manager.calculate_scrollbar_states();
    }

    fn op_scroll_to(&mut self, selector: &str, x: f32, y: f32) {
        let node = self
            .resolve_selector_first(selector)
            .unwrap_or_else(|| panic!("scroll_node_to: no node for selector {selector}"));
        let dom_id = DomId { inner: 0 };
        let now = self.now();
        self.layout_window.scroll_manager.scroll_to(
            dom_id,
            node,
            LogicalPosition { x, y },
            std::time::Duration::from_millis(0).into(),
            azul_core::events::EasingFunction::Linear,
            now,
        );
        self.layout_window.scroll_manager.calculate_scrollbar_states();
    }

    fn op_scroll_into_view(&mut self, selector: &str, block: Option<&str>) {
        use azul_core::events::ScrollLogicalPosition;
        use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
        let node = self
            .resolve_selector_first(selector)
            .unwrap_or_else(|| panic!("scroll_into_view: no node for selector {selector}"));
        let mut opts = ScrollIntoViewOptions::nearest();
        match block {
            Some("start") => opts.block = ScrollLogicalPosition::Start,
            Some("center") => opts.block = ScrollLogicalPosition::Center,
            Some("end") => opts.block = ScrollLogicalPosition::End,
            _ => {}
        }
        let now = self.now();
        self.layout_window
            .scroll_node_into_view(Self::dom_node_id(node), opts, now);
    }

    fn op_focus(&mut self, selector: Option<&str>) {
        // Window-level focus.
        self.window_state.window_focused = true;
        self.window_state.flags.has_focus = true;
        // Optional node focus.
        if let Some(sel) = selector {
            if let Some(node) = self.resolve_selector_first(sel) {
                self.layout_window
                    .focus_manager
                    .set_focused_node(Some(Self::dom_node_id(node)));
                self.layout_window.finalize_pending_focus_changes();
            }
        }
    }

    fn op_blur(&mut self) {
        self.window_state.window_focused = false;
        self.window_state.flags.has_focus = false;
    }

    fn op_move(&mut self, x: i32, y: i32) {
        self.window_state.position =
            WindowPosition::Initialized(PhysicalPositionI32 { x, y });
    }

    fn op_resize(&mut self, width: f32, height: f32) {
        self.window_state.size.dimensions = LogicalSize::new(width, height);
        if let Some(dom) = self.mounted_dom.clone() {
            let mut dbg = Some(Vec::new());
            let _ = self
                .layout_window
                .resize_window(
                    dom,
                    LogicalSize::new(width, height),
                    &self.renderer_resources,
                    &self.system_callbacks,
                    &mut dbg,
                )
                .expect("resize_window");
            self.register_scroll_nodes();
        }
    }

    fn op_dpi_changed(&mut self, dpi: u32) {
        self.window_state.size.dpi = dpi;
        // A scale change forces a full relayout: clear caches then re-layout.
        self.layout_window.clear_caches();
        if let Some(dom) = self.mounted_dom.clone() {
            self.layout(dom);
        }
    }

    fn op_get_focus_state(&mut self) {
        let focused = self.layout_window.focus_manager.get_focused_node().copied();
        let mut s = String::from("focus_state ");
        match focused {
            Some(dn) => {
                s.push_str(&format!("focused=true node={dn:?} "));
                if let Some(nid) = dn.node.into_crate_internal() {
                    s.push_str(&self.node_id_class_string(nid));
                }
            }
            None => s.push_str("focused=false"),
        }
        self.last_response = Response::FocusState(s);
    }

    fn op_get_dom(&mut self) {
        let s = self.serialize_dom();
        self.last_response = Response::Dom(s);
    }

    /// A compact string of a node's ids + classes (for focus/dom `contains`).
    fn node_id_class_string(&self, node: NodeId) -> String {
        let Some(lr) = self.root_layout() else {
            return String::new();
        };
        let node_data = lr.styled_dom.node_data.as_container();
        let Some(data) = node_data.get(node) else {
            return String::new();
        };
        let mut out = String::new();
        for ioc in data.get_ids_and_classes().iter() {
            if let Some(id) = ioc.as_id() {
                out.push_str(&format!("#{id} "));
            }
            if let Some(cls) = ioc.as_class() {
                out.push_str(&format!(".{cls} "));
            }
        }
        out
    }

    /// Serialize the current root DOM to a string carrying node types, ids,
    /// classes and text (for `assert_dom` / `assert_response` `contains`).
    fn serialize_dom(&self) -> String {
        use azul_core::dom::NodeType;
        let Some(lr) = self.root_layout() else {
            return String::from("{\"children\":[]}");
        };
        let node_data = lr.styled_dom.node_data.as_container();
        let mut out = String::from("{\"children\":[");
        for i in 0..node_data.len() {
            let data = &node_data[NodeId::new(i)];
            out.push_str(&format!("{{\"index\":{i},\"type\":\"{:?}\"", data.get_node_type()));
            let ids_classes = data.get_ids_and_classes();
            for ioc in ids_classes.iter() {
                if let Some(id) = ioc.as_id() {
                    out.push_str(&format!(",\"id\":\"{id}\""));
                }
                if let Some(cls) = ioc.as_class() {
                    out.push_str(&format!(",\"class\":\"{cls}\""));
                }
            }
            if let NodeType::Text(t) = data.get_node_type() {
                out.push_str(&format!(",\"text\":\"{}\"", t.as_str()));
            }
            out.push('}');
            if i + 1 < node_data.len() {
                out.push(',');
            }
        }
        out.push_str("]}");
        out
    }

    // -----------------------------------------------------------------
    // Assertions
    // -----------------------------------------------------------------

    fn assert_response(&self, ty: Option<&str>, contains: Option<&str>) {
        if let Some(ty) = ty {
            assert_eq!(
                self.last_response.type_tag(),
                ty,
                "assert_response: expected response type {ty:?}, got {:?}",
                self.last_response.type_tag()
            );
        }
        if let Some(sub) = contains {
            assert!(
                self.last_response.text().contains(sub),
                "assert_response: response {:?} does not contain {sub:?} (text={:?})",
                self.last_response.type_tag(),
                self.last_response.text()
            );
        }
    }

    fn assert_scroll(&self, selector: &str, x: Option<f32>, y: Option<f32>, tol: f32) {
        let node = self
            .resolve_selector_first(selector)
            .unwrap_or_else(|| panic!("assert_scroll: no node for selector {selector}"));
        let offset = self
            .layout_window
            .scroll_manager
            .get_current_offset(DomId { inner: 0 }, node)
            .unwrap_or_else(|| {
                panic!("assert_scroll: node {selector} is not a scrollable container (no offset)")
            });
        if let Some(x) = x {
            assert!(
                (offset.x - x).abs() <= tol,
                "assert_scroll {selector}: x expected {x} got {} (tol {tol})",
                offset.x
            );
        }
        if let Some(y) = y {
            assert!(
                (offset.y - y).abs() <= tol,
                "assert_scroll {selector}: y expected {y} got {} (tol {tol})",
                offset.y
            );
        }
    }

    fn assert_exists(&self, selector: &str) {
        assert!(
            !self.resolve_selector_all(selector).is_empty(),
            "assert_exists: no node matches {selector}"
        );
    }

    fn assert_not_exists(&self, selector: &str) {
        assert!(
            self.resolve_selector_all(selector).is_empty(),
            "assert_not_exists: a node matches {selector}"
        );
    }

    fn assert_node_count(&self, selector: &str, expected: usize) {
        let got = self.resolve_selector_all(selector).len();
        assert_eq!(
            got, expected,
            "assert_node_count {selector}: expected {expected}, got {got}"
        );
    }

    fn assert_layout(&self, selector: &str, property: &str, expected: f32, tol: f32) {
        let node = self
            .resolve_selector_first(selector)
            .unwrap_or_else(|| panic!("assert_layout: no node for selector {selector}"));
        let lr = self.root_layout().expect("assert_layout: no layout result");
        let idx = *lr
            .layout_tree
            .dom_to_layout
            .get(&node)
            .and_then(|v| v.first())
            .unwrap_or_else(|| panic!("assert_layout: node {selector} has no layout box"));
        let size = lr.layout_tree.nodes[idx]
            .used_size
            .unwrap_or_else(|| panic!("assert_layout: node {selector} has no used_size"));
        let pos = lr
            .calculated_positions
            .get(idx)
            .copied()
            .unwrap_or_else(LogicalPosition::zero);
        let got = match property {
            "width" => size.width,
            "height" => size.height,
            "x" => pos.x,
            "y" => pos.y,
            other => panic!("assert_layout: unknown property {other}"),
        };
        assert!(
            (got - expected).abs() <= tol,
            "assert_layout {selector}.{property}: expected {expected} got {got} (tol {tol})"
        );
    }

    fn assert_window_state(&self, property: &str, expected: &serde_json::Value, tol: f32) {
        let ws = &self.window_state;
        let hidpi = ws.size.get_hidpi_factor().inner.get();
        let phys = ws.size.get_physical_size();
        match property {
            "focused" => assert_bool(property, ws.flags.has_focus, expected),
            "window_focused" => assert_bool(property, ws.window_focused, expected),
            "dpi" => assert_num(property, f64::from(ws.size.dpi), expected, f64::from(tol)),
            "hidpi_factor" => {
                assert_num(property, f64::from(hidpi), expected, f64::from(tol));
            }
            "logical_width" => assert_num(
                property,
                f64::from(ws.size.dimensions.width),
                expected,
                f64::from(tol),
            ),
            "logical_height" => assert_num(
                property,
                f64::from(ws.size.dimensions.height),
                expected,
                f64::from(tol),
            ),
            "physical_width" => {
                assert_num(property, f64::from(phys.width), expected, f64::from(tol));
            }
            "physical_height" => {
                assert_num(property, f64::from(phys.height), expected, f64::from(tol));
            }
            "position_x" | "position_y" => {
                let (px, py) = match ws.position {
                    WindowPosition::Initialized(p)
                    | WindowPosition::RelativeToParentWindow(p) => (p.x, p.y),
                    WindowPosition::Uninitialized => {
                        panic!("assert_window_state {property}: window position is not initialized")
                    }
                };
                let v = if property == "position_x" { px } else { py };
                assert_num(property, f64::from(v), expected, f64::from(tol));
            }
            other => panic!("assert_window_state: unknown property {other}"),
        }
    }

    fn assert_dom(
        &self,
        min_node_count: Option<usize>,
        contains: Option<&str>,
        not_contains: Option<&str>,
    ) {
        let serialized = self.serialize_dom();
        if let Some(min) = min_node_count {
            let count = self
                .root_layout()
                .map(|lr| lr.styled_dom.node_data.as_container().len())
                .unwrap_or(0);
            assert!(
                count >= min,
                "assert_dom: node count {count} < min {min}"
            );
        }
        if let Some(sub) = contains {
            assert!(
                serialized.contains(sub),
                "assert_dom: DOM does not contain {sub:?}"
            );
        }
        if let Some(sub) = not_contains {
            assert!(
                !serialized.contains(sub),
                "assert_dom: DOM unexpectedly contains {sub:?}"
            );
        }
    }

    // -----------------------------------------------------------------
    // Driver
    // -----------------------------------------------------------------

    fn run_step(&mut self, step: &Step) {
        match step {
            Step::Mount { html, css } => self.op_mount(html, css),
            Step::WaitFrame | Step::Wait { .. } => {}
            Step::KeyDown { key, modifiers } => self.op_key(key, modifiers, true),
            Step::KeyUp { key, modifiers } => self.op_key(key, modifiers, false),
            Step::ScrollNodeBy {
                selector,
                delta_x,
                delta_y,
            } => self.op_scroll_by(selector, *delta_x, *delta_y),
            Step::ScrollNodeTo { selector, x, y } => self.op_scroll_to(selector, *x, *y),
            Step::ScrollIntoView { selector, block } => {
                self.op_scroll_into_view(selector, block.as_deref());
            }
            Step::Focus { selector } => self.op_focus(selector.as_deref()),
            Step::Blur => self.op_blur(),
            Step::Move { x, y } => self.op_move(*x, *y),
            Step::Resize { width, height } => self.op_resize(*width, *height),
            Step::DpiChanged { dpi } => self.op_dpi_changed(*dpi),
            Step::GetFocusState => self.op_get_focus_state(),
            Step::GetDom => self.op_get_dom(),
            Step::AssertResponse { ty, contains } => {
                self.assert_response(ty.as_deref(), contains.as_deref());
            }
            Step::AssertScroll {
                selector,
                x,
                y,
                tolerance,
            } => self.assert_scroll(selector, *x, *y, *tolerance),
            Step::AssertExists { selector } => self.assert_exists(selector),
            Step::AssertNotExists { selector } => self.assert_not_exists(selector),
            Step::AssertNodeCount { selector, expected } => {
                self.assert_node_count(selector, *expected);
            }
            Step::AssertLayout {
                selector,
                property,
                expected,
                tolerance,
            } => self.assert_layout(selector, property, *expected, *tolerance),
            Step::AssertWindowState {
                property,
                expected,
                tolerance,
            } => self.assert_window_state(property, expected, *tolerance),
            Step::AssertDom {
                min_node_count,
                contains,
                not_contains,
            } => self.assert_dom(*min_node_count, contains.as_deref(), not_contains.as_deref()),
        }
    }
}

// =========================================================================
// Helpers
// =========================================================================

/// Port of the DLL's `build_mount_document` (full.rs): line-array html + css →
/// one XML document with the CSS in a `<head><style>`.
fn build_mount_document(html: &[String], css: &[String]) -> String {
    let html_src = html.join("\n");
    let css_src = css.join("\n");
    let trimmed = html_src.trim_start();
    let has_body = trimmed.starts_with("<body") || trimmed.starts_with("<html");
    let body = if has_body {
        html_src.clone()
    } else {
        format!("<body>\n{html_src}\n</body>")
    };
    format!("<html>\n<head>\n<style>\n{css_src}\n</style>\n</head>\n{body}\n</html>")
}

fn parse_key(key: &str) -> Option<VirtualKeyCode> {
    let k = key.to_ascii_lowercase();
    Some(match k.as_str() {
        "tab" => VirtualKeyCode::Tab,
        "return" | "enter" => VirtualKeyCode::Return,
        "escape" | "esc" => VirtualKeyCode::Escape,
        "space" => VirtualKeyCode::Space,
        "up" => VirtualKeyCode::Up,
        "down" => VirtualKeyCode::Down,
        "left" => VirtualKeyCode::Left,
        "right" => VirtualKeyCode::Right,
        "backspace" | "back" => VirtualKeyCode::Back,
        "delete" | "del" => VirtualKeyCode::Delete,
        "home" => VirtualKeyCode::Home,
        "end" => VirtualKeyCode::End,
        "a" => VirtualKeyCode::A,
        "b" => VirtualKeyCode::B,
        "c" => VirtualKeyCode::C,
        _ => return None,
    })
}

fn assert_bool(prop: &str, got: bool, expected: &serde_json::Value) {
    let want = expected
        .as_bool()
        .unwrap_or_else(|| panic!("assert_window_state {prop}: expected a bool, got {expected:?}"));
    assert_eq!(got, want, "assert_window_state {prop}: expected {want}, got {got}");
}

fn assert_num(prop: &str, got: f64, expected: &serde_json::Value, tol: f64) {
    let want = expected
        .as_f64()
        .unwrap_or_else(|| panic!("assert_window_state {prop}: expected a number, got {expected:?}"));
    assert!(
        (got - want).abs() <= tol,
        "assert_window_state {prop}: expected {want}, got {got} (tol {tol})"
    );
}

// =========================================================================
// Test entry point: discover + run every fixture
// =========================================================================

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/e2e_fixtures")
}

fn run_scenario(scenario: &Scenario) {
    let mut h = E2eHarness::new(
        scenario.setup.window_width,
        scenario.setup.window_height,
        scenario.setup.dpi,
    );
    for (i, step) in scenario.steps.iter().enumerate() {
        eprintln!("  [{}] step {i}: {:?}", scenario.name, step);
        h.run_step(step);
    }
}

#[test]
fn run_all_e2e_fixtures() {
    let dir = fixtures_dir();
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
        .collect();
    entries.sort();

    assert!(
        !entries.is_empty(),
        "no *.json fixtures found under {}",
        dir.display()
    );

    let mut ran = 0usize;
    let mut failures: Vec<String> = Vec::new();
    for path in &entries {
        let src = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
        let scenario: Scenario = serde_json::from_str(&src)
            .unwrap_or_else(|e| panic!("cannot parse {}: {e}", path.display()));
        eprintln!("=== running fixture: {} ({}) ===", scenario.name, path.display());
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_scenario(&scenario);
        }));
        match res {
            Ok(()) => ran += 1,
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
                    .unwrap_or_else(|| "<non-string panic>".to_string());
                failures.push(format!("{}: {msg}", scenario.name));
            }
        }
    }

    eprintln!("e2e_json: {ran}/{} fixtures passed", entries.len());
    assert!(
        failures.is_empty(),
        "e2e_json fixtures failed:\n{}",
        failures.join("\n")
    );
    // Silence unused warning on the BTreeMap import if optimization removes it.
    let _ = BTreeMap::<u8, u8>::new();
}
