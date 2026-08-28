//! `<transient-window>`: a popup that is a REAL OS window, drawn from a subtree of
//! the one DOM. A colour picker that opens below its swatch, a tooltip with a
//! pointer arrow, a tear-off tool palette - each wants its own window surface (so it
//! can escape the parent's bounds, carry a shadow, sit above everything) but NOT its
//! own application: it needs the parent's state, callbacks and styling, and it must
//! open and close by flipping one attribute.

use alloc::vec::Vec;

use crate::geom::{LogicalSize, OptionLogicalSize};

/// Which edge of the anchor node a transient window opens from. Expressed as an
/// EDGE, never as coordinates.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TransientAnchor {
    /// Below the anchor, left edges aligned - what a dropdown or a colour picker
    /// does. The default, because it is what `<select>` does.
    #[default]
    Bottom,
    /// Above the anchor, left edges aligned.
    Top,
    /// To the left of the anchor, top edges aligned.
    Left,
    /// To the right of the anchor, top edges aligned - what a submenu does.
    Right,
    /// At the pointer position rather than the anchor rect - what a context menu
    /// does.
    Cursor,
}

/// What closes a transient window without the app asking.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TransientDismiss {
    /// A press outside the window closes it, and so does Escape. The default: it is
    /// how every popup a user has ever met behaves.
    #[default]
    Outside,
    /// Only Escape closes it. For a popup the user interacts with by clicking
    /// around it - a floating toolbar.
    Escape,
    /// Nothing closes it but the app. For palettes that stay up.
    None,
}

/// Whether - and how - the user may drag a transient window away from its anchor. A
/// torn-off window is a free toplevel that is STILL the same DOM subtree: Photoshop
/// palettes, GIMP tear-off menus, Firefox tear-off tabs.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TransientTearoff {
    /// The window stays where its anchor put it. The default.
    #[default]
    None,
    /// Dropping it anywhere off its anchor makes it a free toplevel; dragging the
    /// toplevel back over the anchor docks it again.
    Free,
    /// Like `Free`, and dropping it onto a DROP ZONE re-anchors the window there
    /// instead. Zones are the nodes matching the selector carried in the node's
    /// `tearoff-zone` attribute (`tearoff="zone:.sidebar"` in XML sets both); they
    /// are hit-tested in the PARENT's layout.
    Zone,
}

impl TransientTearoff {
    /// `"true"` / `"free"` / `"1"` / `""` -> `Free`, `"zone"` or
    /// `"zone:<selector>"` -> `Zone`, anything else -> `None`.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        let v = value.trim();
        if matches!(v, "true" | "free" | "1" | "") {
            Self::Free
        } else if v == "zone" || v.starts_with("zone:") {
            Self::Zone
        } else {
            Self::None
        }
    }
}

/// Where a transient window lives while it is NOT torn off.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum TransientDock {
    /// A popup window anchored to an edge of its parent (or of the drop zone it was
    /// dropped on). The default: menus, pickers, tooltips.
    #[default]
    Popup,
    /// Laid out INLINE as ordinary content of its parent - or of the drop zone it
    /// was dropped on, where it then scrolls, clips and reflows with that zone's
    /// layout. The Visual-Studio tool-window model: drag the grip out to float it
    /// (`tearoff`), drop it on another zone to move it there, and the app's DOM
    /// never changes - the engine re-parents the subtree in the layout tree.
    Inline,
}

impl TransientDock {
    /// `"inline"` -> `Inline`, anything else -> `Popup`.
    #[must_use]
    pub fn parse(value: &str) -> Self {
        if value.trim() == "inline" {
            Self::Inline
        } else {
            Self::Popup
        }
    }
}

/// The inline configuration of a `NodeType::TransientWindow`. `Copy` and small on
/// purpose: it rides inside `NodeType` the way `GeolocationProbeConfig` does, so
/// opening a popup needs no allocation and `NodeType` (48 bytes, set by its largest
/// payload) does not grow.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct TransientWindowConfig {
    /// Explicit size, or `None` to size the window to its content - the common
    /// case, and the reason a colour picker never has to guess how tall its own
    /// panel is. `OptionLogicalSize`, the `#[repr(C, u8)]` option, NOT `Option<_>`:
    /// this struct rides inside `NodeType`, which rides inside every `Dom` the C ABI
    /// passes by value, and a Rust `Option` has no stable layout - clippy's
    /// `improper_ctypes_definitions` flagged every `extern "C" fn -> Dom` in the
    /// tree the moment a plain `Option` went in here.
    pub size: OptionLogicalSize,
    /// Which edge of the anchor (the node's parent) it opens from.
    pub anchor: TransientAnchor,
    /// What closes it.
    pub dismiss: TransientDismiss,
    /// Whether the user may drag the window OUT of its anchor into a free toplevel
    /// that is still the same DOM subtree (see [`TransientTearoff`]).
    pub tearoff: TransientTearoff,
    /// Popup at an anchor edge (the default), or inline content of its parent /
    /// drop zone that can be torn off and dropped elsewhere (`dock="inline"`, see
    /// [`TransientDock`]).
    pub dock: TransientDock,
    /// The popup window's background material (`material="transparent"`):
    /// `Transparent` gives the window per-pixel alpha, and its shape follows what
    /// the content paints - rounded corners are real corners, a pointer arrow is
    /// part of the window, a click beside it falls through. A clip mask on the node
    /// (`set_clip_mask`, the same mask any DOM node can carry) implies
    /// `Transparent`: the mask IS the window's shape.
    pub material: crate::window::WindowBackgroundMaterial,
    /// The ONLY thing an application toggles. `true` materialises the subtree as a
    /// window; `false` tears it down and drops it from layout entirely.
    pub open: bool,
    /// The app's word on whether the window is currently torn off. Like `open`,
    /// this is a REQUEST the engine follows on every change: flipping it `true`
    /// tears the window off at its anchor position, flipping it `false` docks it.
    pub torn: bool,
}

impl Default for TransientWindowConfig {
    /// Closed, anchored below, dismissed by outside-click, content-sized.
    fn default() -> Self {
        Self {
            open: false,
            anchor: TransientAnchor::Bottom,
            dismiss: TransientDismiss::Outside,
            size: OptionLogicalSize::None,
            tearoff: TransientTearoff::None,
            dock: TransientDock::Popup,
            material: crate::window::WindowBackgroundMaterial::Opaque,
            torn: false,
        }
    }
}

impl TransientWindowConfig {
    /// Closed, with every other field at its default.
    #[must_use]
    pub const fn closed() -> Self {
        Self {
            open: false,
            anchor: TransientAnchor::Bottom,
            dismiss: TransientDismiss::Outside,
            size: OptionLogicalSize::None,
            tearoff: TransientTearoff::None,
            dock: TransientDock::Popup,
            material: crate::window::WindowBackgroundMaterial::Opaque,
            torn: false,
        }
    }

    /// Open, with every other field at its default.
    #[must_use]
    pub const fn opened() -> Self {
        Self { open: true, ..Self::closed() }
    }

    #[must_use]
    pub const fn with_anchor(mut self, anchor: TransientAnchor) -> Self {
        self.anchor = anchor;
        self
    }

    #[must_use]
    pub const fn with_dismiss(mut self, dismiss: TransientDismiss) -> Self {
        self.dismiss = dismiss;
        self
    }

    #[must_use]
    pub const fn with_size(mut self, size: LogicalSize) -> Self {
        self.size = OptionLogicalSize::Some(size);
        self
    }

    #[must_use]
    pub const fn with_tearoff(mut self, tearoff: TransientTearoff) -> Self {
        self.tearoff = tearoff;
        self
    }

    #[must_use]
    pub const fn with_torn(mut self, torn: bool) -> Self {
        self.torn = torn;
        self
    }

    #[must_use]
    pub const fn with_material(mut self, material: crate::window::WindowBackgroundMaterial) -> Self {
        self.material = material;
        self
    }

    #[must_use]
    pub const fn with_dock(mut self, dock: TransientDock) -> Self {
        self.dock = dock;
        self
    }
}

impl TransientWindowConfig {
    /// Apply one XML/HTML attribute. Returns `true` if the key was one of ours.
    pub fn apply_attr(&mut self, key: &str, value: &str) -> bool {
        match key {
            "open" => {
                self.open = matches!(value.trim(), "true" | "open" | "1" | "");
                true
            }
            "anchor" => {
                self.anchor = TransientAnchor::parse(value);
                true
            }
            "dismiss" => {
                self.dismiss = TransientDismiss::parse(value);
                true
            }
            "tearoff" => {
                self.tearoff = TransientTearoff::parse(value);
                true
            }
            "torn" => {
                self.torn = matches!(value.trim(), "true" | "1" | "");
                true
            }
            "dock" => {
                self.dock = TransientDock::parse(value);
                true
            }
            "material" => {
                self.material = match value.trim() {
                    "transparent" => crate::window::WindowBackgroundMaterial::Transparent,
                    "sidebar" => crate::window::WindowBackgroundMaterial::Sidebar,
                    "menu" => crate::window::WindowBackgroundMaterial::Menu,
                    "hud" => crate::window::WindowBackgroundMaterial::HUD,
                    "titlebar" => crate::window::WindowBackgroundMaterial::Titlebar,
                    "mica-alt" => crate::window::WindowBackgroundMaterial::MicaAlt,
                    _ => crate::window::WindowBackgroundMaterial::Opaque,
                };
                true
            }
            "size" => {
                self.size = match value.trim().split_once('x') {
                    Some((w, h)) => match (w.trim().parse::<f32>(), h.trim().parse::<f32>()) {
                        (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 => {
                            OptionLogicalSize::Some(LogicalSize::new(w, h))
                        }
                        _ => OptionLogicalSize::None,
                    },
                    None => OptionLogicalSize::None, // "content" or garbage
                };
                true
            }
            _ => false,
        }
    }
}

impl TransientAnchor {
    /// The attribute value, as written in XML: `anchor="bottom"`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "bottom",
            Self::Top => "top",
            Self::Left => "left",
            Self::Right => "right",
            Self::Cursor => "cursor",
        }
    }

    /// Parse the attribute value. Unknown strings fall back to the default rather
    /// than erroring: a typo in a popup's anchor should degrade to "opens below",
    /// not to "does not open".
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "top" => Self::Top,
            "left" => Self::Left,
            "right" => Self::Right,
            "cursor" => Self::Cursor,
            _ => Self::Bottom,
        }
    }
}

impl TransientDismiss {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Outside => "outside",
            Self::Escape => "escape",
            Self::None => "none",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "escape" => Self::Escape,
            "none" => Self::None,
            _ => Self::Outside,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default must be CLOSED. A popup that is open by default would
    /// materialise a window for every `<transient-window>` in the tree on first
    /// layout.
    #[test]
    fn the_default_is_closed_and_anchored_below() {
        let c = TransientWindowConfig::default();
        assert!(!c.open);
        assert_eq!(c.anchor, TransientAnchor::Bottom);
        assert_eq!(c.dismiss, TransientDismiss::Outside);
        assert!(matches!(c.size, OptionLogicalSize::None), "content-sized unless told otherwise");
    }

    /// Attribute values round-trip, and unknown ones degrade to the default rather
    /// than failing.
    #[test]
    fn anchor_and_dismiss_round_trip_through_their_attribute_strings() {
        for a in [
            TransientAnchor::Bottom,
            TransientAnchor::Top,
            TransientAnchor::Left,
            TransientAnchor::Right,
            TransientAnchor::Cursor,
        ] {
            assert_eq!(TransientAnchor::parse(a.as_str()), a);
        }
        for d in [TransientDismiss::Outside, TransientDismiss::Escape, TransientDismiss::None] {
            assert_eq!(TransientDismiss::parse(d.as_str()), d);
        }
        assert_eq!(TransientAnchor::parse("sideways"), TransientAnchor::Bottom);
        assert_eq!(TransientDismiss::parse("maybe"), TransientDismiss::Outside);
    }

    /// Attributes as written in XML produce the config they name.
    #[test]
    fn attributes_apply_onto_the_config() {
        let mut c = TransientWindowConfig::closed();
        assert!(c.apply_attr("open", "true"));
        assert!(c.apply_attr("anchor", "right"));
        assert!(c.apply_attr("dismiss", "escape"));
        assert!(c.apply_attr("size", "320x240"));
        assert!(c.apply_attr("tearoff", "true"));
        assert!(c.apply_attr("torn", "true"));
        assert!(c.apply_attr("material", "transparent"));
        assert!(c.apply_attr("dock", "inline"));
        assert!(!c.apply_attr("class", "x"), "not ours - the caller keeps it");

        assert!(c.open);
        assert_eq!(c.anchor, TransientAnchor::Right);
        assert_eq!(c.dismiss, TransientDismiss::Escape);
        assert!(matches!(c.size, OptionLogicalSize::Some(s) if s.width == 320.0 && s.height == 240.0));
        assert_eq!(c.tearoff, TransientTearoff::Free);
        assert!(c.torn);
        assert_eq!(c.material, crate::window::WindowBackgroundMaterial::Transparent);
        assert_eq!(c.dock, TransientDock::Inline);
        c.apply_attr("dock", "popup");
        assert_eq!(c.dock, TransientDock::Popup);
        c.apply_attr("material", "opaque");
        assert_eq!(c.material, crate::window::WindowBackgroundMaterial::Opaque);
        c.apply_attr("tearoff", "zone:.sidebar");
        assert_eq!(c.tearoff, TransientTearoff::Zone);
        c.apply_attr("tearoff", "nope");
        assert_eq!(c.tearoff, TransientTearoff::None);

        // Degrade, never refuse: a bad size means content-sized.
        c.apply_attr("size", "big");
        assert!(matches!(c.size, OptionLogicalSize::None));
        c.apply_attr("open", "false");
        assert!(!c.open);
    }

    /// The config must not GROW `NodeType`. `NodeType` is 48 bytes today (measured
    /// 2026-08-22); the largest payload sets that.
    #[test]
    fn the_config_does_not_grow_node_type() {
        let cfg = core::mem::size_of::<TransientWindowConfig>();
        let node = core::mem::size_of::<crate::dom::NodeType>();
        assert!(
            cfg < node,
            "TransientWindowConfig is {cfg} bytes, NodeType is {node}: the config \
             has become the largest payload and is now what sets NodeType's size"
        );
    }
}

/// Rebuild the subtree under `root` as a standalone [`crate::dom::Dom`], for laying
/// out as the root of a transient window. This is how a popup stays part of the ONE
/// tree while owning its own surface.
#[must_use]
pub fn extract_subtree_as_dom(
    styled_dom: &crate::styled_dom::StyledDom,
    root: crate::id::NodeId,
) -> Option<crate::dom::Dom> {
    use crate::dom::NodeType;

    {
        let nodes = styled_dom.node_data.as_container();
        let root_data = nodes.get(root)?;
        if !matches!(root_data.get_node_type(), NodeType::TransientWindow(_)) {
            return None;
        }
    }

    let mut dom = build_subtree(styled_dom, root, true, 0)?;
    // The container inside its own window is just a block.
    dom.root.set_node_type(NodeType::Div);
    Some(dom)
}

/// Clones `id` and its descendants into a fresh `Dom`, styles baked in.
fn build_subtree(
    styled_dom: &crate::styled_dom::StyledDom,
    id: crate::id::NodeId,
    is_root: bool,
    depth: usize,
) -> Option<crate::dom::Dom> {
    use crate::styled_dom::NodeHierarchyItem;

    if depth > 256 {
        return None; // a malformed tree must not recurse forever
    }
    let nodes = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let mut data = nodes.get(id)?.clone();
    bake_resolved_style(styled_dom, id, is_root, &mut data);
    let mut dom = crate::dom::Dom::create_from_data(data);
    let mut child = hierarchy.get(id).and_then(|h| h.first_child_id(id));
    while let Some(c) = child {
        if let Some(cd) = build_subtree(styled_dom, c, false, depth + 1) {
            dom.add_child(cd);
        }
        child = hierarchy.get(c).and_then(NodeHierarchyItem::next_sibling_id);
    }
    Some(dom)
}

/// Copies the style the PARENT tree's cascade resolved for `id` onto the extracted
/// node as inline properties, so the copy looks exactly as the original did in
/// place. A `Dom::with_css(..)` sheet is *scoped*: it lives on the `Dom` subtree it
/// was attached to and is selector-matched into the property cache when the
/// `StyledDom` is built, after which the sheet itself is gone.
fn bake_resolved_style(
    styled_dom: &crate::styled_dom::StyledDom,
    id: crate::id::NodeId,
    is_root: bool,
    data: &mut crate::dom::NodeData,
) {
    use azul_css::dynamic_selector::{CssPropertyWithConditions, DynamicSelector, PseudoStateType};

    let cache = styled_dom.get_css_property_cache();
    let i = id.index(); // `get_slice` is empty past the end, so no guard needed
    let with_state = |p: &crate::prop_cache::StatefulCssProperty| CssPropertyWithConditions {
        property: p.property.clone(),
        apply_if: if p.state == PseudoStateType::Normal {
            Vec::new().into()
        } else {
            vec![DynamicSelector::PseudoState(p.state)].into()
        },
    };
    // Inherited first, so the node's own matched rules win on a clash (later inline
    // rules outrank earlier ones at equal priority). `computed_values` holds the
    // resolved Normal-state value of every property the node ends up with; the
    // INHERITABLE ones are exactly "what the ancestors gave it" (font-size already
    // resolved to px, so an `em` chain stays intact).
    if is_root {
        if let Some(computed) = cache.computed_values.get(i) {
            for (prop_type, p) in computed {
                if prop_type.is_inheritable() {
                    data.add_css_property(CssPropertyWithConditions {
                        property: p.property.clone(),
                        apply_if: Vec::new().into(),
                    });
                }
            }
        }
    }
    for p in cache.css_props.get_slice(i) {
        data.add_css_property(with_state(p));
    }
}

#[cfg(test)]
mod extract_tests {
    use super::*;
    use crate::{
        dom::{Dom, NodeData, NodeType},
        styled_dom::StyledDom,
    };

    /// The extracted DOM holds exactly the popup's content, re-rooted as a plain
    /// block, and nothing from outside the subtree.
    #[test]
    fn extracts_exactly_the_subtree_re_rooted_as_a_div() {
        let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
            TransientWindowConfig::opened(),
        )))
        .with_child(Dom::create_p_with_text("inside"));
        let full = Dom::create_body()
            .with_child(Dom::create_p_with_text("outside"))
            .with_child(Dom::create_div().with_child(popup));
        let styled = StyledDom::create_from_dom(full);

        // body=0, p=1, text=2, div=3, transient=4, p=5, text=6
        let nodes = styled.node_data.as_container();
        let tw = nodes
            .linear_iter()
            .find(|n| matches!(nodes.get(*n).map(NodeData::get_node_type), Some(NodeType::TransientWindow(_))))
            .expect("the transient node");

        let out = extract_subtree_as_dom(&styled, tw).expect("extracts");
        assert!(
            matches!(out.root.get_node_type(), NodeType::Div),
            "the popup's own root must be a plain container in its window"
        );
        // root + p + text = 3 nodes; "outside" must not be among them.
        let out_styled = StyledDom::create_from_dom(out);
        let texts: Vec<String> = out_styled
            .node_data
            .as_ref()
            .iter()
            .filter_map(|n| match n.get_node_type() {
                NodeType::Text(t) => Some(t.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(texts, vec!["inside".to_string()]);
    }

    /// Only a TransientWindow can be extracted - asking for a div is a caller bug
    /// and must not quietly open a window onto arbitrary content.
    #[test]
    fn refuses_a_non_transient_root() {
        let styled = StyledDom::create_from_dom(Dom::create_body().with_child(Dom::create_div()));
        assert!(extract_subtree_as_dom(&styled, crate::id::NodeId::new(1)).is_none());
    }

    /// Author CSS attached with `Dom::with_css` is SCOPED - it is consumed into the
    /// property cache when the tree is styled - so cloning node data alone would
    /// drop it. The extracted copy must carry the resolved style, including what the
    /// root inherited from ancestors it leaves behind.
    #[test]
    fn resolved_style_travels_with_the_extracted_subtree() {
        use azul_css::props::{
            basic::{ColorU, PixelValue},
            layout::LayoutWidth,
            property::{CssProperty, CssPropertyType},
            style::StyleTextColor,
        };

        let popup = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(
            TransientWindowConfig::opened(),
        )))
        .with_child(Dom::create_div().with_css("width: 240px;"));
        let full = Dom::create_body()
            .with_css("color: #123456;") // inherited by the popup root from OUTSIDE the subtree
            .with_child(Dom::create_div().with_child(popup));
        let styled = StyledDom::create_from_dom(full);

        let nodes = styled.node_data.as_container();
        let tw = nodes
            .linear_iter()
            .find(|n| matches!(nodes.get(*n).map(NodeData::get_node_type), Some(NodeType::TransientWindow(_))))
            .expect("the transient node");
        let out = extract_subtree_as_dom(&styled, tw).expect("extracts");

        // The popup's root inherited the body's colour.
        let root_color = out.root.style.iter_inline_properties().find_map(|(p, _)| match p {
            CssProperty::TextColor(c) => c.get_property().copied(),
            _ => None,
        });
        assert_eq!(
            root_color,
            Some(StyleTextColor { inner: ColorU { r: 0x12, g: 0x34, b: 0x56, a: 255 } }),
            "the root must carry what it inherited from outside the subtree"
        );

        // The child's own `width: 240px` came along as a matched author rule.
        let child = out.children.as_ref().first().expect("the sized child");
        let width = child.root.style.iter_inline_properties().find_map(|(p, _)| match p {
            CssProperty::Width(w) => w.get_property().cloned(),
            _ => None,
        });
        assert_eq!(
            width,
            Some(LayoutWidth::Px(PixelValue::px(240.0))),
            "a scoped `with_css` rule must survive extraction"
        );
        assert!(
            child.root.style.iter_inline_properties().all(|(p, _)| p.get_type() != CssPropertyType::TextColor),
            "a non-root node carries only its OWN matched rules; inheritance is re-derived"
        );
    }
}
