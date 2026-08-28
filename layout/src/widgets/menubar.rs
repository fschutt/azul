//! Software menu-bar widget — the Linux fallback when there is no native global
//! menu (GNOME/KDE export their own; Windows uses `HMENU`, macOS the app menu).
//!
//! Renders the window's [`Menu`] (declared by the user via
//! `Dom::with_menu_bar(menu)`) as a horizontal bar of top-level items. Clicking a
//! top-level item opens its children as a dropdown popup positioned directly
//! below the item via [`CallbackInfo::open_menu_for_hit_node`] — which looks up
//! the clicked node's on-screen rect and opens a child window at its bottom-left
//! (the unified [`WindowPosition::RelativeToParentWindow`] path).
//!
//! ## Styling
//!
//! All styling is inline via `with_css(&str)` using the `system:` color namespace
//! (`system:window-background`, `system:text`, `system:selection-background`, …)
//! and the `system:ui` font, so the bar matches the OS theme without threading a
//! `SystemStyle` through. The bar MUST be injected at the *Dom* level (before
//! `StyledDom::create_from_dom`) so `scope_inline_css` scopes these rules in the
//! same flatten pass as the rest of the window — see
//! `shell2::common::layout::regenerate_layout`.
//!
//! ## Backreference pattern
//!
//! The dropdown's items are the user's own [`MenuItem`]s, each carrying the
//! `(RefAny, Callback)` backreference the user attached with
//! `StringMenuItem::with_callback(data, cb)`. Clicking a leaf fires that callback
//! with the user's data — so the bar is a thin shell and behaviour stays in user
//! code (see `doc/guide/en/architecture.md`, "the backreference pattern").
//!
//! [`WindowPosition::RelativeToParentWindow`]: azul_core::window::WindowPosition::RelativeToParentWindow

use azul_core::{
    callbacks::{CoreCallback, CoreCallbackData},
    dom::{Dom, EventFilter, HoverEventFilter, IdOrClass::Class, IdOrClassVec},
    menu::{Menu, MenuItem, MenuItemVec, StringMenuItem},
    refany::{OptionRefAny, RefAny},
};

/// Class on the injected bar root — also the detection marker the renderer / tests
/// use to recognise an injected software menu bar.
pub const MENUBAR_CLASS: &str = "__azul-native-menubar";
/// Class on each top-level bar item.
pub const MENUBAR_ITEM_CLASS: &str = "azul-menubar-item";

/// Inline CSS for the bar root: a full-width horizontal flex row themed from the
/// OS (`system:` colors + `system:ui` font). Bare declarations, so the rule is
/// scoped node-only at flatten time.
const MENUBAR_CSS: &str = "display: flex; \
     flex-direction: row; \
     align-items: stretch; \
     width: 100%; \
     height: 26px; \
     background: system:window-background; \
     color: system:text; \
     font-family: system:ui; \
     font-size: 14px; \
     padding-left: 2px;";

/// Inline CSS for a top-level item: vertically-centered click target with hover
/// feedback (the `:hover` block nests via CSS nesting in `parse_inline`).
const MENUBAR_ITEM_CSS: &str = "display: flex; \
     flex-direction: row; \
     align-items: center; \
     padding-left: 10px; \
     padding-right: 10px; \
     color: system:text; \
     cursor: pointer; \
     :hover { background: system:selection-background; color: system:selection-text; }";

/// Build the software menu-bar DOM from a [`Menu`].
///
/// The bar is a flex row of one item per top-level `MenuItem::String`
/// (separators / break-lines are not rendered in the bar). Inject the returned
/// `Dom` at the Dom level so its `with_css` rules are scoped in the main flatten.
#[must_use]
pub fn build_menubar_dom(menu: &Menu) -> Dom {
    let mut bar = Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![
            Class(MENUBAR_CLASS.into()),
            Class("azul-menubar".into()),
        ]))
        .with_css(MENUBAR_CSS);

    for item in menu.items.as_slice() {
        if let MenuItem::String(s) = item {
            bar = bar.with_child(build_menubar_item(s));
        }
    }

    bar
}

/// One clickable top-level bar item. Its `MouseUp` callback opens the item's
/// submenu (its children, or — for a top-level leaf — a one-item menu of itself
/// so the leaf's own callback still fires) below the item.
fn build_menubar_item(item: &StringMenuItem) -> Dom {
    // The submenu carried (by value) into the click callback as its RefAny.
    let submenu = if item.children.as_slice().is_empty() {
        Menu::create(MenuItemVec::from_vec(vec![MenuItem::String(item.clone())]))
    } else {
        Menu::create(item.children.clone())
    };

    Dom::create_div()
        .with_ids_and_classes(IdOrClassVec::from_vec(vec![Class(
            MENUBAR_ITEM_CLASS.into(),
        )]))
        .with_css(MENUBAR_ITEM_CSS)
        .with_child(crate::widgets::widget_p_with_text(item.label.clone()))
        .with_callbacks(
            vec![CoreCallbackData {
                event: EventFilter::Hover(HoverEventFilter::MouseUp),
                callback: CoreCallback {
                    cb: callbacks::menubar_item_click as usize,
                    ctx: OptionRefAny::None,
                },
                refany: RefAny::new(submenu),
            }]
            .into(),
        )
}

pub(crate) mod callbacks {
    use azul_core::{callbacks::Update, menu::Menu, refany::RefAny};

    use crate::callbacks::CallbackInfo;

    /// Top-level bar item clicked → open its submenu under the item. The submenu
    /// `Menu` is the callback's `data` (a backreference set at build time); its
    /// items carry the user's own callbacks, fired by the menu system on click.
    pub(super) extern "C" fn menubar_item_click(
        mut data: RefAny,
        mut info: CallbackInfo,
    ) -> Update {
        if let Some(menu) = data.downcast_ref::<Menu>() {
            info.open_menu_for_hit_node(menu.clone());
        }
        Update::DoNothing
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        sync::{Arc, Mutex},
    };

    use azul_core::{
        callbacks::Update,
        dom::{DomId, DomNodeId, IdOrClass, NodeId, NodeType},
        geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        menu::{CoreMenuCallback, MenuItemIcon, MenuItemState},
        resources::RendererResources,
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{
            MonitorVec, RawWindowHandle, VirtualKeyCode, VirtualKeyCodeCombo, VirtualKeyCodeVec,
        },
    };
    use azul_css::AzString;
    use rust_fontconfig::FcFontCache;

    use super::callbacks::menubar_item_click;
    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfo, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{
            display_list::{DisplayList, DisplayListItem, WindowLogicalRect},
            layout_tree::LayoutTree,
        },
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Menu fixtures
    // ------------------------------------------------------------------

    fn leaf(label: &str) -> StringMenuItem {
        StringMenuItem::create(AzString::from(label))
    }

    fn string_item(label: &str) -> MenuItem {
        MenuItem::String(leaf(label))
    }

    fn menu_of(items: Vec<MenuItem>) -> Menu {
        Menu::create(MenuItemVec::from_vec(items))
    }

    /// A leaf whose every optional field is populated — so "the leaf is wrapped
    /// verbatim" is a claim about *all* of `StringMenuItem`, not just its label.
    fn decorated_leaf(label: &str) -> StringMenuItem {
        let mut it = leaf(label);
        it.accelerator = Some(VirtualKeyCodeCombo {
            keys: VirtualKeyCodeVec::from_vec(vec![VirtualKeyCode::Q]),
        })
        .into();
        it.callback = Some(CoreMenuCallback {
            refany: RefAny::new(0xDEAD_BEEF_u64),
            callback: CoreCallback {
                cb: 0x1234_usize,
                ctx: OptionRefAny::None,
            },
        })
        .into();
        it.menu_item_state = MenuItemState::Greyed;
        it.icon = Some(MenuItemIcon::Checkbox(true)).into();
        it
    }

    /// `depth` levels of single-child nesting under one top-level item.
    /// Kept modest on purpose: the builders clone/drop recursively, and a test
    /// that blows the stack aborts the whole harness instead of failing.
    fn nested(depth: usize) -> StringMenuItem {
        let mut cur = leaf("bottom");
        for i in 0..depth {
            cur = leaf(&format!("lvl{i}")).with_child(MenuItem::String(cur));
        }
        cur
    }

    /// Labels/markers of a menu's items, in menu order.
    fn labels_of(menu: &Menu) -> Vec<String> {
        menu.items
            .as_slice()
            .iter()
            .map(|i| match i {
                MenuItem::String(s) => s.label.as_str().to_string(),
                MenuItem::Separator => "<sep>".to_string(),
                MenuItem::BreakLine => "<br>".to_string(),
            })
            .collect()
    }

    // ------------------------------------------------------------------
    // Dom inspection
    // ------------------------------------------------------------------

    fn text_of(node: &Dom) -> Option<&str> {
        match node.root.get_node_type() {
            NodeType::Text(t) => Some(t.as_ref().as_str()),
            _ => None,
        }
    }

    fn classes_of(node: &Dom) -> Vec<String> {
        node.root
            .get_ids_and_classes()
            .as_ref()
            .iter()
            .filter_map(|c| match c {
                Class(s) => Some(s.as_str().to_string()),
                IdOrClass::Id(_) => None,
            })
            .collect()
    }

    /// The label rendered by a bar item: a `<p>` block wrapping the text.
    ///
    /// The bar item used to hold a BARE text node. A text leaf owns no box, so
    /// in a flex/grid parent it competes as an item with no line box of its own
    /// — browsers wrap such runs in an anonymous block, azul does not. Every
    /// widget label is a `<p>` now, so this walks one level deeper.
    fn rendered_label(item_dom: &Dom) -> &str {
        let children = item_dom.children.as_ref();
        assert_eq!(
            children.len(),
            1,
            "a bar item renders exactly one label block"
        );
        let label = &children[0];
        assert!(
            matches!(label.root.get_node_type(), NodeType::P),
            "a bar item's label must be wrapped in a <p>, not a bare text node"
        );
        let inner = label.children.as_ref();
        assert_eq!(inner.len(), 1, "a label <p> wraps exactly one text node");
        text_of(&inner[0]).expect("the <p>'s only child must be a text node")
    }

    /// The true recursive descendant count — what `estimated_total_children`
    /// caches, and what `convert_dom_into_compact_dom` sizes its arenas from.
    fn recursive_descendants(node: &Dom) -> usize {
        node.children
            .as_ref()
            .iter()
            .map(|c| 1 + recursive_descendants(c))
            .sum()
    }

    fn assert_children_count_is_in_sync(dom: &Dom) {
        assert_eq!(
            dom.estimated_total_children,
            recursive_descendants(dom),
            "estimated_total_children must equal the real descendant count — a \
             too-small value makes convert_dom_into_compact_dom under-allocate",
        );
    }

    /// The `RefAny` a bar item carries on its click callback.
    fn refany_of(item_dom: &Dom) -> RefAny {
        let cbs = item_dom.root.callbacks.as_ref();
        assert_eq!(cbs.len(), 1, "a bar item registers exactly one callback");
        cbs[0].refany.clone()
    }

    /// Decodes a bar item's backreference back into the `Menu` it will open.
    fn submenu_of(item_dom: &Dom) -> Menu {
        let mut refany = refany_of(item_dom);
        let guard = refany
            .downcast_ref::<Menu>()
            .expect("a bar item's backreference must be a Menu");
        guard.clone()
    }

    /// DOM node ids of the bar items inside a flattened bar, in document order.
    fn item_node_ids(styled: &StyledDom) -> Vec<NodeId> {
        styled
            .node_data
            .as_ref()
            .iter()
            .enumerate()
            .filter(|(_, nd)| {
                nd.get_ids_and_classes()
                    .as_ref()
                    .iter()
                    .any(|c| matches!(c, Class(s) if s.as_str() == MENUBAR_ITEM_CLASS))
            })
            .map(|(i, _)| NodeId::new(i))
            .collect()
    }

    // ------------------------------------------------------------------
    // CallbackInfo harness (mirrors the one in `drop_down.rs`)
    // ------------------------------------------------------------------

    struct Env<'a> {
        ref_data: &'a CallbackInfoRefData<'a>,
        changes: &'a Arc<Mutex<Vec<CallbackChange>>>,
        default_hit: DomNodeId,
    }

    impl Env<'_> {
        fn info(&self) -> CallbackInfo {
            self.info_at(self.default_hit)
        }

        fn info_at(&self, hit: DomNodeId) -> CallbackInfo {
            CallbackInfo::new(
                self.ref_data,
                self.changes,
                hit,
                OptionLogicalPosition::None,
                OptionLogicalPosition::None,
            )
        }

        fn take_changes(&self) -> Vec<CallbackChange> {
            self.changes
                .lock()
                .map(|mut c| core::mem::take(&mut *c))
                .unwrap_or_default()
        }

        fn take_one(&self) -> CallbackChange {
            let mut changes = self.take_changes();
            assert_eq!(changes.len(), 1, "expected exactly one change: {changes:?}");
            changes.remove(0)
        }
    }

    fn dom_node(node: NodeId) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(node)),
        }
    }

    fn no_hit_node() -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::NONE,
        }
    }

    /// The tag the hit-tester would use for `node`. `open_menu_for_node` resolves
    /// the anchor rect through this mapping, so a forged hit-test area must reuse
    /// the id the styling pass actually assigned.
    fn tag_of(styled_dom: &StyledDom, node: NodeId) -> u64 {
        let nid = NodeHierarchyItemId::from_crate_internal(Some(node));
        styled_dom
            .tag_ids_to_node_ids
            .iter()
            .find(|m| m.node_id == nid)
            .expect("a menubar item must be hit-testable")
            .tag_id
            .inner
    }

    /// A `DomLayoutResult` carrying only a `styled_dom` plus forged hit-test
    /// areas. `menubar_item_click` reaches exactly one geometry query
    /// (`get_node_hit_test_bounds`), which reads the display list only — so no
    /// real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom, anchors: &[(NodeId, LogicalRect)]) -> DomLayoutResult {
        let mut display_list = DisplayList::default();
        for (node, rect) in anchors {
            let tag = tag_of(&styled_dom, *node);
            display_list.items.push(DisplayListItem::HitTestArea {
                bounds: WindowLogicalRect::new(rect.origin, rect.size),
                // The tag TYPE matters: `get_node_hit_test_bounds` looks for
                // a DOM-node area specifically (text-run cursor areas share
                // the `tag.0` numbering space), so a forged area must carry
                // the same type the display list builder stamps.
                tag: (tag, azul_core::hit_test::TAG_TYPE_DOM_NODE),
            });
        }

        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(display_list),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Empty `LayoutWindow`, no hit node — the "nothing to anchor to" case.
    fn with_env<R>(f: impl FnOnce(&Env<'_>) -> R) -> R {
        with_env_cfg(None, &[], None, f)
    }

    fn with_anchored_env<R>(
        styled_dom: StyledDom,
        anchors: &[(NodeId, LogicalRect)],
        hit: NodeId,
        f: impl FnOnce(&Env<'_>) -> R,
    ) -> R {
        with_env_cfg(Some(styled_dom), anchors, Some(hit), f)
    }

    fn with_env_cfg<R>(
        styled: Option<StyledDom>,
        anchors: &[(NodeId, LogicalRect)],
        hit: Option<NodeId>,
        f: impl FnOnce(&Env<'_>) -> R,
    ) -> R {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd, anchors));
        }

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(azul_css::system::SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));
        let env = Env {
            ref_data: &ref_data,
            changes: &changes,
            default_hit: hit.map_or_else(no_hit_node, dom_node),
        };
        f(&env)
    }

    // ==================================================================
    // build_menubar_dom
    // ==================================================================

    #[test]
    fn build_menubar_dom_on_an_empty_menu_yields_a_childless_bar() {
        let bar = build_menubar_dom(&menu_of(Vec::new()));

        assert!(
            bar.children.as_ref().is_empty(),
            "an empty menu must produce no bar items — not a placeholder"
        );
        assert_eq!(bar.estimated_total_children, 0);
        assert_eq!(
            classes_of(&bar),
            vec![MENUBAR_CLASS.to_string(), "azul-menubar".to_string()],
            "the detection marker class must be present even on an empty bar"
        );
        assert!(
            bar.root.callbacks.as_ref().is_empty(),
            "the bar root itself is inert — only its items are clickable"
        );
    }

    #[test]
    fn build_menubar_dom_attaches_the_bar_css_exactly_once() {
        let bar = build_menubar_dom(&menu_of(vec![string_item("File")]));

        let css = bar.css.as_ref();
        assert_eq!(
            css.len(),
            1,
            "with_css must push exactly one component stylesheet"
        );
        assert!(
            !css[0].rules.as_ref().is_empty(),
            "MENUBAR_CSS must not silently parse to nothing"
        );

        let item_css = bar.children.as_ref()[0].css.as_ref();
        assert_eq!(item_css.len(), 1, "each item carries its own stylesheet");
        assert!(
            !item_css[0].rules.as_ref().is_empty(),
            "MENUBAR_ITEM_CSS (incl. its nested :hover block) must not parse to nothing"
        );
    }

    #[test]
    fn build_menubar_dom_renders_only_string_items() {
        // Separators / break-lines are documented as *not* rendered in the bar,
        // and — the real trap — must not shift the item↦child mapping.
        let m = menu_of(vec![
            MenuItem::Separator,
            string_item("File"),
            MenuItem::BreakLine,
            MenuItem::Separator,
            string_item("Edit"),
            MenuItem::BreakLine,
        ]);
        let bar = build_menubar_dom(&m);

        let labels: Vec<&str> = bar.children.as_ref().iter().map(rendered_label).collect();
        assert_eq!(
            labels,
            vec!["File", "Edit"],
            "order must survive the skipped items"
        );

        for child in bar.children.as_ref() {
            assert_eq!(classes_of(child), vec![MENUBAR_ITEM_CLASS.to_string()]);
        }
    }

    #[test]
    fn build_menubar_dom_on_only_non_string_items_yields_a_childless_bar() {
        for items in [
            vec![MenuItem::Separator],
            vec![MenuItem::BreakLine],
            vec![
                MenuItem::Separator,
                MenuItem::BreakLine,
                MenuItem::Separator,
            ],
        ] {
            let bar = build_menubar_dom(&menu_of(items));
            assert!(
                bar.children.as_ref().is_empty(),
                "separators/break-lines are never rendered in the bar"
            );
            assert_children_count_is_in_sync(&bar);
        }
    }

    #[test]
    fn build_menubar_dom_preserves_pathological_labels_byte_for_byte() {
        let huge = "x".repeat(100_000);
        let cases = vec![
            "",
            " ",
            "\t\n\r",
            "a\u{0}b",                // interior NUL
            "\u{0}",                  // lone NUL
            "👨‍👩‍👧‍👦",                     // ZWJ sequence
            "مرحبا",                  // RTL
            "e\u{301}\u{301}\u{301}", // stacked combining marks
            "\u{200b}\u{feff}",       // zero-width / BOM
            "\u{1f600}\u{fe0f}",      // astral + variation selector
            "&<>\"'",                 // markup metacharacters
            "{ color: red }",         // CSS-looking label
            huge.as_str(),
        ];
        let m = menu_of(cases.iter().map(|c| string_item(c)).collect::<Vec<_>>());
        let bar = build_menubar_dom(&m);

        let labels: Vec<&str> = bar.children.as_ref().iter().map(rendered_label).collect();
        assert_eq!(
            labels, cases,
            "labels must round-trip into the DOM verbatim"
        );
        assert_eq!(
            labels[cases.len() - 1].len(),
            100_000,
            "a 100k-char label must not be truncated"
        );
        assert_children_count_is_in_sync(&bar);
    }

    #[test]
    fn build_menubar_dom_keeps_estimated_total_children_in_sync() {
        for n in [0_usize, 1, 2, 17, 256] {
            let m = menu_of((0..n).map(|i| string_item(&format!("m{i}"))).collect());
            let bar = build_menubar_dom(&m);

            assert_eq!(bar.children.as_ref().len(), n);
            assert_eq!(
                bar.estimated_total_children,
                3 * n,
                "each item contributes itself + its label <p> + the text node"
            );
            assert_children_count_is_in_sync(&bar);
        }
    }

    #[test]
    fn build_menubar_dom_scales_to_a_wide_menu() {
        const N: usize = 1_000;
        let m = menu_of((0..N).map(|i| string_item(&format!("item{i}"))).collect());
        let bar = build_menubar_dom(&m);

        assert_eq!(bar.children.as_ref().len(), N);
        for (i, child) in bar.children.as_ref().iter().enumerate() {
            assert_eq!(
                rendered_label(child),
                format!("item{i}"),
                "item {i} is misplaced"
            );
        }
        assert_children_count_is_in_sync(&bar);
    }

    #[test]
    fn build_menubar_dom_round_trips_every_item_into_its_own_submenu() {
        // Duplicate labels + interleaved separators: if the builder ever indexed
        // `menu.items` instead of iterating the *rendered* items, this mapping
        // would slip by one.
        let with_kids = leaf("File").with_children(MenuItemVec::from_vec(vec![
            string_item("New"),
            MenuItem::Separator,
            string_item("Open"),
        ]));
        let dup_a = leaf("Same").with_child(string_item("a"));
        let dup_b = leaf("Same").with_child(string_item("b"));
        let bare = leaf("Help");

        let m = menu_of(vec![
            MenuItem::Separator,
            MenuItem::String(with_kids.clone()),
            MenuItem::String(dup_a.clone()),
            MenuItem::BreakLine,
            MenuItem::String(dup_b.clone()),
            MenuItem::String(bare.clone()),
        ]);
        let bar = build_menubar_dom(&m);
        let children = bar.children.as_ref();
        assert_eq!(children.len(), 4);

        assert_eq!(
            labels_of(&submenu_of(&children[0])),
            vec!["New", "<sep>", "Open"]
        );
        assert_eq!(labels_of(&submenu_of(&children[1])), vec!["a"]);
        assert_eq!(labels_of(&submenu_of(&children[2])), vec!["b"]);
        // A top-level leaf opens a one-item menu of *itself* so its own callback
        // still fires.
        assert_eq!(
            submenu_of(&children[3]),
            Menu::create(MenuItemVec::from_vec(vec![MenuItem::String(bare)])),
        );
    }

    #[test]
    fn build_menubar_dom_gives_identical_items_distinct_backreferences() {
        let m = menu_of(vec![string_item("Same"), string_item("Same")]);
        let bar = build_menubar_dom(&m);

        let a = refany_of(&bar.children.as_ref()[0]);
        let b = refany_of(&bar.children.as_ref()[1]);
        assert!(
            a != b,
            "two bar items must not share one RefAny — dropping one would then \
             invalidate the other's submenu"
        );
        assert_eq!(
            a.get_type_id(),
            b.get_type_id(),
            "…while still both being Menus"
        );
    }

    #[test]
    fn build_menubar_dom_only_renders_the_top_level_of_a_deep_menu() {
        let deep = nested(64);
        let bar = build_menubar_dom(&menu_of(vec![MenuItem::String(deep.clone())]));

        assert_eq!(
            bar.children.as_ref().len(),
            1,
            "nesting depth is not bar width"
        );
        assert_children_count_is_in_sync(&bar);

        // Only the *direct* children are carried into the popup.
        let sub = submenu_of(&bar.children.as_ref()[0]);
        assert_eq!(sub.items.as_slice().len(), 1);
        assert_eq!(labels_of(&sub), vec!["lvl62"]);
    }

    #[test]
    fn build_menubar_dom_flattens_into_a_styled_dom_with_hit_testable_items() {
        let m = menu_of(vec![
            string_item("File"),
            MenuItem::Separator,
            string_item("Edit"),
        ]);
        let styled = StyledDom::create_from_dom(build_menubar_dom(&m));

        let items = item_node_ids(&styled);
        assert_eq!(items.len(), 2, "one flat node per rendered bar item");
        for id in items {
            // Panics with a clear message if the item lost its tag.
            let _tag = tag_of(&styled, id);
        }
    }

    // ==================================================================
    // build_menubar_item
    // ==================================================================

    #[test]
    fn build_menubar_item_wraps_a_leaf_verbatim_including_every_optional_field() {
        let item = decorated_leaf("Quit");
        let dom = build_menubar_item(&item);

        let sub = submenu_of(&dom);
        assert_eq!(
            sub,
            Menu::create(MenuItemVec::from_vec(vec![MenuItem::String(item.clone())])),
            "a leaf must be wrapped byte-for-byte — accelerator, callback, state \
             and icon included — or its own callback can never fire",
        );

        let MenuItem::String(wrapped) = &sub.items.as_slice()[0] else {
            panic!("the wrapped item must still be a String item");
        };
        assert_eq!(wrapped.menu_item_state, MenuItemState::Greyed);
        assert!(wrapped.accelerator.is_some());
        assert!(wrapped.icon.is_some());
        assert!(wrapped.callback.is_some());
    }

    #[test]
    fn build_menubar_item_with_children_opens_exactly_those_children() {
        let children = vec![
            string_item("New"),
            MenuItem::Separator,
            string_item("Open"),
            MenuItem::BreakLine,
            MenuItem::String(decorated_leaf("Quit")),
        ];
        let item = leaf("File").with_children(MenuItemVec::from_vec(children.clone()));
        let dom = build_menubar_item(&item);

        let sub = submenu_of(&dom);
        assert_eq!(
            sub,
            Menu::create(MenuItemVec::from_vec(children)),
            "a parent must open its children verbatim, not itself",
        );
        assert!(
            !labels_of(&sub).contains(&"File".to_string()),
            "a parent must NOT wrap itself — that would duplicate the top-level entry",
        );
    }

    #[test]
    fn build_menubar_item_registers_exactly_one_mouseup_hover_callback() {
        let dom = build_menubar_item(&leaf("File"));
        let cbs = dom.root.callbacks.as_ref();

        assert_eq!(cbs.len(), 1);
        assert_eq!(cbs[0].event, EventFilter::Hover(HoverEventFilter::MouseUp));
        assert_eq!(
            cbs[0].callback.cb, menubar_item_click as usize,
            "the item must be wired to the menubar click handler",
        );
        assert!(
            matches!(cbs[0].callback.ctx, OptionRefAny::None),
            "a native Rust callback carries no FFI context",
        );
    }

    #[test]
    fn build_menubar_item_shape_is_one_class_one_text_child() {
        let huge = "z".repeat(100_000);
        for label in ["", "File", "👨‍👩‍👧‍👦", "a\u{0}b", huge.as_str()] {
            let dom = build_menubar_item(&leaf(label));

            assert_eq!(classes_of(&dom), vec![MENUBAR_ITEM_CLASS.to_string()]);
            assert_eq!(
                rendered_label(&dom),
                label,
                "the label must survive verbatim"
            );
            // The item, its label <p>, and the text leaf inside it. This was 1
            // when the item held a BARE text node; every widget label is a <p>
            // now, because a text leaf owns no box and misbehaves as a
            // flex/grid item.
            assert_eq!(dom.estimated_total_children, 2);
            assert_children_count_is_in_sync(&dom);
        }
    }

    #[test]
    fn build_menubar_item_is_pure_and_repeatable() {
        let item = leaf("File").with_child(string_item("New"));

        let a = build_menubar_item(&item);
        let b = build_menubar_item(&item);

        assert_eq!(submenu_of(&a), submenu_of(&b), "same input ⇒ same submenu");
        assert!(
            refany_of(&a) != refany_of(&b),
            "each build must allocate its own backreference",
        );
        // The source item is untouched (it is only ever cloned).
        assert_eq!(item.label.as_str(), "File");
        assert_eq!(item.children.as_slice().len(), 1);
    }

    #[test]
    fn build_menubar_item_treats_an_empty_child_vec_as_a_leaf() {
        let empty = leaf("Help").with_children(MenuItemVec::from_vec(Vec::new()));
        let sub = submenu_of(&build_menubar_item(&empty));

        assert_eq!(
            sub.items.as_slice().len(),
            1,
            "an explicitly-empty child vec is still a leaf, so it self-wraps \
             rather than opening an empty popup",
        );
        assert_eq!(labels_of(&sub), vec!["Help"]);
    }

    // ==================================================================
    // menubar_item_click
    // ==================================================================

    #[test]
    fn menubar_item_click_without_a_hit_node_queues_nothing() {
        let data = refany_of(&build_menubar_item(&leaf("File")));

        with_env(|env| {
            assert_eq!(
                menubar_item_click(data.clone(), env.info()),
                Update::DoNothing
            );
            assert!(
                env.take_changes().is_empty(),
                "no anchor ⇒ no half-opened menu",
            );
        });
    }

    #[test]
    fn menubar_item_click_without_layout_geometry_queues_nothing() {
        let m = menu_of(vec![string_item("File")]);
        let bar = build_menubar_dom(&m);
        let data = refany_of(&bar.children.as_ref()[0]);
        let styled = StyledDom::create_from_dom(bar);
        let item = item_node_ids(&styled)[0];

        // Flattened DOM, but no hit-test area for the item: the geometry lookup
        // must fail closed.
        with_anchored_env(styled, &[], item, |env| {
            assert_eq!(
                menubar_item_click(data.clone(), env.info()),
                Update::DoNothing
            );
            assert!(env.take_changes().is_empty());
        });
    }

    #[test]
    fn menubar_item_click_on_a_foreign_payload_is_a_no_op() {
        for mut data in [
            RefAny::new(0_u32),
            RefAny::new(String::from("not a menu")),
            RefAny::new(Vec::<u8>::new()),
        ] {
            with_env(|env| {
                assert_eq!(
                    menubar_item_click(data.clone(), env.info()),
                    Update::DoNothing,
                    "a wrong-typed backreference must be ignored, not downcast blindly",
                );
                assert!(env.take_changes().is_empty());
            });
            assert!(
                data.downcast_ref::<Menu>().is_none(),
                "…and the payload is still not a Menu afterwards",
            );
        }
    }

    #[test]
    fn menubar_item_click_opens_the_submenu_at_the_bottom_left_of_the_item() {
        let m = menu_of(vec![MenuItem::String(leaf("File").with_children(
            MenuItemVec::from_vec(vec![
                string_item("New"),
                MenuItem::Separator,
                string_item("Open"),
            ]),
        ))]);
        let bar = build_menubar_dom(&m);
        let data = refany_of(&bar.children.as_ref()[0]);
        let expected = submenu_of(&bar.children.as_ref()[0]);
        let styled = StyledDom::create_from_dom(bar);
        let item = item_node_ids(&styled)[0];

        let rect = LogicalRect::new(
            LogicalPosition::new(10.0, 20.0),
            LogicalSize::new(100.0, 26.0),
        );
        with_anchored_env(styled, &[(item, rect)], item, |env| {
            assert_eq!(
                menubar_item_click(data.clone(), env.info()),
                Update::DoNothing
            );

            let CallbackChange::OpenMenu { menu, position } = env.take_one() else {
                panic!("expected exactly one OpenMenu change");
            };
            assert_eq!(
                menu, expected,
                "the queued menu must be the item's backreference"
            );

            let p = position.expect("the popup must be pinned under the bar item");
            assert_eq!((p.x, p.y), (10.0, 46.0), "bottom-left of the item rect");
        });
    }

    #[test]
    fn menubar_item_click_opens_each_bar_items_own_menu() {
        let m = menu_of(vec![
            MenuItem::String(leaf("File").with_child(string_item("New"))),
            MenuItem::Separator,
            MenuItem::String(leaf("Edit").with_child(string_item("Undo"))),
            MenuItem::String(leaf("Help")),
        ]);
        let bar = build_menubar_dom(&m);
        let payloads: Vec<RefAny> = bar.children.as_ref().iter().map(refany_of).collect();
        let styled = StyledDom::create_from_dom(bar);
        let items = item_node_ids(&styled);
        assert_eq!(items.len(), 3);

        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(40.0, 26.0));
        let anchors: Vec<(NodeId, LogicalRect)> = items.iter().map(|n| (*n, rect)).collect();
        let expected = [vec!["New"], vec!["Undo"], vec!["Help"]];

        with_anchored_env(styled, &anchors, items[0], |env| {
            for (i, node) in items.iter().enumerate() {
                assert_eq!(
                    menubar_item_click(payloads[i].clone(), env.info_at(dom_node(*node))),
                    Update::DoNothing,
                );
                let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                    panic!("item {i}: expected an OpenMenu change");
                };
                assert_eq!(
                    labels_of(&menu),
                    expected[i],
                    "item {i} opened the wrong menu"
                );
            }
        });
    }

    #[test]
    fn menubar_item_click_is_repeatable_and_does_not_consume_the_payload() {
        let bar = build_menubar_dom(&menu_of(vec![string_item("File")]));
        let mut data = refany_of(&bar.children.as_ref()[0]);
        let styled = StyledDom::create_from_dom(bar);
        let item = item_node_ids(&styled)[0];

        let rect = LogicalRect::new(LogicalPosition::new(1.0, 2.0), LogicalSize::new(3.0, 4.0));
        with_anchored_env(styled, &[(item, rect)], item, |env| {
            for round in 0..5 {
                assert_eq!(
                    menubar_item_click(data.clone(), env.info()),
                    Update::DoNothing
                );
                let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                    panic!("round {round}: expected an OpenMenu change");
                };
                assert_eq!(labels_of(&menu), vec!["File"]);
            }
        });

        assert!(
            data.downcast_ref::<Menu>().is_some(),
            "the backreference must survive repeated clicks",
        );
    }

    #[test]
    fn menubar_item_click_rejects_degenerate_anchor_rects() {
        // `get_node_hit_test_bounds` only accepts strictly positive extents, so
        // each of these must fail closed instead of queueing a menu at nowhere.
        let degenerate = [
            LogicalSize::new(0.0, 26.0),
            LogicalSize::new(100.0, 0.0),
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-100.0, -26.0),
            LogicalSize::new(f32::NAN, 26.0),
            LogicalSize::new(100.0, f32::NAN),
            LogicalSize::new(f32::NAN, f32::NAN),
        ];

        for size in degenerate {
            let bar = build_menubar_dom(&menu_of(vec![string_item("File")]));
            let data = refany_of(&bar.children.as_ref()[0]);
            let styled = StyledDom::create_from_dom(bar);
            let item = item_node_ids(&styled)[0];
            let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), size);

            with_anchored_env(styled, &[(item, rect)], item, |env| {
                assert_eq!(
                    menubar_item_click(data.clone(), env.info()),
                    Update::DoNothing
                );
                assert!(
                    env.take_changes().is_empty(),
                    "a {size:?} anchor must not queue a menu",
                );
            });
        }
    }

    #[test]
    fn menubar_item_click_survives_non_finite_anchor_origins() {
        // Origin/height arithmetic (`origin.y + size.height`) is unchecked — it
        // must saturate to an infinity or NaN rather than panic.
        let cases = [
            (
                LogicalPosition::new(f32::MAX, f32::MAX),
                LogicalSize::new(1.0, f32::MAX),
            ),
            (
                LogicalPosition::new(f32::MIN, f32::MIN),
                LogicalSize::new(1.0, 1.0),
            ),
            (
                LogicalPosition::new(f32::NAN, f32::NAN),
                LogicalSize::new(1.0, 1.0),
            ),
            (
                LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
                LogicalSize::new(1.0, f32::INFINITY),
            ),
        ];

        for (origin, size) in cases {
            let bar = build_menubar_dom(&menu_of(vec![string_item("File")]));
            let data = refany_of(&bar.children.as_ref()[0]);
            let styled = StyledDom::create_from_dom(bar);
            let item = item_node_ids(&styled)[0];
            let rect = LogicalRect::new(origin, size);

            with_anchored_env(styled, &[(item, rect)], item, |env| {
                assert_eq!(
                    menubar_item_click(data.clone(), env.info()),
                    Update::DoNothing
                );

                let CallbackChange::OpenMenu { position, .. } = env.take_one() else {
                    panic!("expected an OpenMenu change for {origin:?}/{size:?}");
                };
                let p = position.expect("a positive-extent anchor still yields a position");
                let expected_y = origin.y + size.height;

                assert_eq!(p.x.is_nan(), origin.x.is_nan());
                if !p.x.is_nan() {
                    assert_eq!(p.x, origin.x);
                }
                assert_eq!(
                    p.y.is_nan(),
                    expected_y.is_nan(),
                    "NaN must propagate, not turn into a bogus finite coordinate",
                );
                if !p.y.is_nan() {
                    assert_eq!(p.y, expected_y, "y must saturate to origin.y + height");
                }
            });
        }
    }

    #[test]
    fn menubar_item_click_queues_an_empty_menu_for_an_empty_backreference() {
        // Not reachable through the builders (a leaf self-wraps), but the handler
        // must still not panic on a menu with no items.
        let bar = build_menubar_dom(&menu_of(vec![string_item("File")]));
        let data = RefAny::new(Menu::create(MenuItemVec::from_vec(Vec::new())));
        let styled = StyledDom::create_from_dom(bar);
        let item = item_node_ids(&styled)[0];

        let rect = LogicalRect::new(LogicalPosition::new(5.0, 5.0), LogicalSize::new(5.0, 5.0));
        with_anchored_env(styled, &[(item, rect)], item, |env| {
            assert_eq!(
                menubar_item_click(data.clone(), env.info()),
                Update::DoNothing
            );
            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected an OpenMenu change");
            };
            assert!(
                menu.items.as_slice().is_empty(),
                "an empty menu is not a panic"
            );
        });
    }

    #[test]
    fn menubar_item_click_queues_a_wide_menu_without_truncation() {
        const N: usize = 500;
        let children: Vec<MenuItem> = (0..N).map(|i| string_item(&format!("c{i}"))).collect();
        let m = menu_of(vec![MenuItem::String(
            leaf("File").with_children(MenuItemVec::from_vec(children)),
        )]);
        let bar = build_menubar_dom(&m);
        let data = refany_of(&bar.children.as_ref()[0]);
        let styled = StyledDom::create_from_dom(bar);
        let item = item_node_ids(&styled)[0];

        let rect = LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(1.0, 1.0));
        with_anchored_env(styled, &[(item, rect)], item, |env| {
            assert_eq!(
                menubar_item_click(data.clone(), env.info()),
                Update::DoNothing
            );
            let CallbackChange::OpenMenu { menu, .. } = env.take_one() else {
                panic!("expected an OpenMenu change");
            };
            assert_eq!(menu.items.as_slice().len(), N);
            assert_eq!(labels_of(&menu)[N - 1], format!("c{}", N - 1));
        });
    }
}
