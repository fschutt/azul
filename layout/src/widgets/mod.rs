//! Built-in widgets for the Azul GUI system

/// Implements `Display, Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Hash`
/// for a Callback with a `.cb` field.
///
/// This is necessary to work around for <https://github.com/rust-lang/rust/issues/54508>
///
/// # Host-invoker plumbing for managed-FFI bindings
///
/// Widget callbacks have varying shapes — some are
/// `(RefAny, CallbackInfo) -> Update` (Button), others add a state
/// struct (CheckBox/Tab/etc.), a few have two extras (`ListView`). The
/// macro therefore does **not** auto-emit an `impl_managed_callback!`
/// invocation; per-widget files apply it themselves with the right
/// extras list. The base invocation still produces the standard
/// `Display`/`Debug`/`Clone`/`From<CallbackType>`/`From<Callback>` impls
/// that all widget callbacks share.
#[macro_export]
macro_rules! impl_widget_callback {
    (
        $callback_wrapper:ident,
        $option_callback_wrapper:ident,
        $callback_value:ident,
        $callback_ty:ident
    ) => {
        #[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
        #[repr(C)]
        pub struct $callback_wrapper {
            pub refany: RefAny,
            pub callback: $callback_value,
        }

        #[repr(C)]
        pub struct $callback_value {
            pub cb: $callback_ty,
            /// For FFI: stores the foreign callable (e.g., `PyFunction`)
            /// Native Rust code sets this to None
            pub ctx: azul_core::refany::OptionRefAny,
        }

        azul_css::impl_option!(
            $callback_wrapper,
            $option_callback_wrapper,
            copy = false,
            [Debug, Clone, PartialEq, PartialOrd, Eq, Ord, Hash]
        );

        impl $callback_value {
            /// Create a new callback with just a function pointer (for native Rust code)
            pub fn create<I: Into<$callback_value>>(cb: I) -> $callback_value {
                cb.into()
            }
        }

        impl ::core::fmt::Display for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                write!(f, "{:?}", self)
            }
        }

        impl ::core::fmt::Debug for $callback_value {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                let callback = stringify!($callback_value);
                write!(f, "{} @ 0x{:x}", callback, self.cb as *const () as usize)
            }
        }

        impl Clone for $callback_value {
            fn clone(&self) -> Self {
                $callback_value {
                    cb: self.cb.clone(),
                    ctx: self.ctx.clone(),
                }
            }
        }

        impl core::hash::Hash for $callback_value {
            fn hash<H>(&self, state: &mut H)
            where
                H: ::core::hash::Hasher,
            {
                state.write_usize(self.cb as *const () as usize);
            }
        }

        impl PartialEq for $callback_value {
            fn eq(&self, rhs: &Self) -> bool {
                self.cb as *const () as usize == rhs.cb as usize
            }
        }

        impl PartialOrd for $callback_value {
            fn partial_cmp(&self, other: &Self) -> Option<::core::cmp::Ordering> {
                Some((self.cb as *const () as usize).cmp(&(other.cb as usize)))
            }
        }

        impl Ord for $callback_value {
            fn cmp(&self, other: &Self) -> ::core::cmp::Ordering {
                (self.cb as *const () as usize).cmp(&(other.cb as usize))
            }
        }

        impl Eq for $callback_value {}

        /// Allow creating callback from a raw function pointer
        /// Sets callable to None (for native Rust/C usage)
        impl From<$callback_ty> for $callback_value {
            fn from(cb: $callback_ty) -> $callback_value {
                $callback_value {
                    cb,
                    ctx: azul_core::refany::OptionRefAny::None,
                }
            }
        }

        /// Allow creating widget callback from a generic Callback
        /// This enables Python/FFI code to pass generic callbacks to widget methods
        impl From<$crate::callbacks::Callback> for $callback_value {
            // transmute target ($callback_value's cb fn-ptr type) varies per macro
            // instantiation, so an explicit annotation can't be written generically here.
            #[allow(clippy::missing_transmute_annotations, clippy::useless_transmute)]
            fn from(cb: $crate::callbacks::Callback) -> $callback_value {
                $callback_value {
                    cb: unsafe { core::mem::transmute(cb.cb) },
                    ctx: cb.ctx,
                }
            }
        }
    };
}

/// Button widget
pub mod button;
/// Checkbox widget
pub mod check_box;
/// Box displaying a color with a callback for value changes
pub mod color_input;
/// File input widget
pub mod file_input;
/// Label widget (centered text)
pub mod label;
/// Drop-down select widget
pub mod drop_down;
/// Frame container widget
pub mod frame;
/// List view widget
pub mod list_view;
/// Shared core for the video-ish widgets (camera/screencap/video): the
/// `VideoFrame` type + the GL-texture `present_frame` writeback.
///
/// See
/// `capture_common.rs`.
pub mod capture_common;
/// Camera-preview widget (P6) — a "dumb widget" owning a background capture
/// thread + a GL-texture ImageRef; no camera logic in core.
///
/// Same RefAny-
/// dataset + merge-callback design as the map widget. See `camera.rs`.
pub mod camera;
/// Screen-capture widget (P6) — identical "dumb widget" architecture to the
/// camera widget, capturing a display/window instead.
///
/// See `screencap.rs`.
pub mod screencap;
/// Video-playback widget (P6) — same "dumb widget" architecture, decoding a
/// video source (vk-video) into a GL texture.
///
/// See `video.rs`.
pub mod video;
/// Microphone-capture widget (P7) — same "dumb widget" architecture as the
/// capture widgets, audio instead of video (no GL): a background thread feeds
/// each `AudioFrame` to the user's `on_frame` hook.
///
/// See `microphone.rs`.
pub mod microphone;
/// Map widget — MVT tile + MapCSS → SVG → DOM (AzulMaps goal app, P3).
///
/// Cache lives in a dataset RefAny owned by a merge callback so it
/// survives relayout. See `layout/src/widgets/map.rs` for the design.
pub mod map;
/// Software menu-bar widget (Linux fallback when there is no native global menu).
///
/// Renders a window's `Menu` as a horizontal bar; items open dropdowns via the
/// unified `WindowPosition::RelativeToParentWindow` popup path.
pub mod menubar;
/// Node graph widget
pub mod node_graph;
/// Same as text input, but only allows numeric input
pub mod number_input;
/// Progress bar widget
pub mod progressbar;
/// Ribbon widget
pub mod ribbon;
/// Office-style backstage view (the full-window "FILE" screen): accent nav
/// column + back ring + app-provided pane content. the Office-2013-era look look by default;
/// pairs with the ribbon's `RibbonAppButton`. See `backstage.rs`.
pub mod backstage;
/// Office-style status bar: left text segments, view switcher, zoom cluster
/// (embeds the `slider` widget). the Office-2013-era look look by default. See `statusbar.rs`.
pub mod statusbar;
/// Office-style title band with a Quick Access Toolbar (save/undo/redo),
/// centered title and window buttons, drawn as DOM. the Office-2013-era look look by
/// default; use `titlebar` instead for native-caption windows. See
/// `quick_access.rs`.
pub mod quick_access;
/// Tab container widgets
pub mod tabs;
/// Single line text input widget
pub mod text_input;
/// Titlebar widget for custom window chrome
pub mod titlebar;
/// Tree view widget
pub mod tree_view;
/// Switch / toggle widget.
///
/// Boolean on/off with a sliding knob; see `switch.rs`.
pub mod switch;
/// Divider / separator rule widget (horizontal or vertical).
///
/// See `divider.rs`.
pub mod divider;
/// Card container widget.
///
/// Elevated/bordered content box (no title); see `card.rs`.
pub mod card;
/// Badge widget.
///
/// A small rounded count/status pill (stateless); see `badge.rs`.
pub mod badge;
/// Slider / range widget.
///
/// Draggable thumb on a track → numeric value; see `slider.rs`.
pub mod slider;
/// Segmented control widget.
///
/// Joined row of mutually-exclusive buttons; see `segmented.rs`.
pub mod segmented;
/// Radio-group widget.
///
/// Vertical/horizontal group of mutually-exclusive options (exactly one selected) with a circular indicator; see `radio_group.rs`.
pub mod radio_group;
/// Tooltip widget.
///
/// Shows a small text popup near an anchor on hover; see `tooltip.rs`.
pub mod tooltip;
/// Multi-line text input (text area) widget.
///
/// See `text_area.rs`.
pub mod text_area;
/// Alert / banner widget.
///
/// A coloured inline message box with an optional dismissible close button; see `alert.rs`.
pub mod alert;
/// Accordion / expander widget.
///
/// One or more collapsible titled sections; see `accordion.rs`.
pub mod accordion;
/// Avatar widget.
///
/// A circular image/initials badge (stateless); see `avatar.rs`.
pub mod avatar;
/// Chip / tag widget.
///
/// A compact rounded pill with a label + optional removable "x" (stateful when removable, mirrors alert's dismiss); see `chip.rs`.
pub mod chip;
/// Spinner / activity widget.
///
/// A static indeterminate busy ring (stateless; no animation — see the file's PARTIAL/TODO2 note); see `spinner.rs`.
pub mod spinner;
/// Popover widget.
///
/// A click-triggered floating panel holding arbitrary content, anchored to a `Dom` (the click-toggled sibling of tooltip); see `popover.rs`.
pub mod popover;
/// Combobox widget.
///
/// An editable text field with a click-toggled drop-down list of options (drop_down's select + text_input's editable field); see `combobox.rs`.
pub mod combobox;
/// Modal / dialog widget.
///
/// An in-app overlay dialog (backdrop + centred panel + arbitrary content), shown/hidden via state toggle; see `modal.rs`.
pub mod modal;
/// Toast / snackbar widget.
///
/// A transient floating notification banner pinned to a corner, manually dismissed via "x" (auto-timeout needs a host timer — see the file's TODO2); a near-clone of `alert.rs` positioned as an overlay; see `toast.rs`.
pub mod toast;
/// Breadcrumb widget.
///
/// A horizontal trail of clickable crumb links separated by "/", ending in the current (non-clickable) page; see `breadcrumb.rs`.
pub mod breadcrumb;
/// Pagination widget.
///
/// A `Prev` / page-numbers / `Next` page navigator with an active-page restyle (segmented-style); see `pagination.rs`.
pub mod pagination;
/// Stepper / wizard widget.
///
/// A horizontal numbered-step progress indicator with connector lines and an accent/muted restyle on step change (segmented-style + progressbar-style filled connector); see `stepper.rs`.
pub mod stepper;
/// Split-pane / splitter widget.
///
/// A two-pane (horizontal/vertical) container with a draggable divider that live-resizes the panes via `set_css_property` (the frame two-box layout + the map/slider pointer-drag state machine); see `split_pane.rs`.
pub mod split_pane;
/// Time picker widget.
///
/// Two clamped numeric up/down spinners (hour + minute) side by side with an optional AM/PM toggle for 12-hour mode (the number_input clamp/retext path + segmented's clickable-cell navigation); see `time_picker.rs`.
pub mod time_picker;
/// Calendar date picker widget.
///
/// A month header (‹ / `Month YYYY` / ›) above a weekday-labelled 7-column day grid computed from real calendar math; clicking a day selects + restyles it (segmented-style), and the per-cell day number is carried drop_down-style. Month nav fires on_change but cannot rebuild the grid in-widget (prominent module TODO2); see `date_picker.rs`.
pub mod date_picker;
// /// Spreadsheet (virtualized view) widget
// pub mod spreadsheet;

/// Every shipped widget's `dom()` with reasonable defaults, for lints that
/// must hold across the whole widget set (the label-convention test below and
/// `dom_lint`'s runtime-warning twin). Test-only.
#[cfg(test)]
pub(crate) fn all_widget_doms_for_lint() -> Vec<(&'static str, azul_core::dom::Dom)> {
    label_convention::every_widget_dom()
}

// ---------------------------------------------------------------------------
// Widget-owned text carriers
// ---------------------------------------------------------------------------
//
// The label convention (see `label_convention` below) puts every widget's own
// text — a label, a value, a placeholder, a `×` glyph — inside a `<p>`, because
// a bare text node is box-less. But the UA stylesheet gives every `<p>`
// `margin: 1em 0`, which is right for a paragraph of prose and wrong for a
// control's text: it is what made a NumberInput holding "42" 39 px tall
// against a 26 px empty TextInput, pushed the TextArea placeholder 13 px down,
// and grew hello-world's counter by two font-sizes (demo test, 2026-08-21).
//
// The reset cannot live in the per-site inline props: `with_css_props`
// REPLACES a node's inline style, and half the call sites apply their style
// after constructing the `<p>`. It lives on the `<p>`'s own component sheet
// instead (`Dom::css`), which `with_css_props` never touches, at AUTHOR
// priority — above the UA default, below an explicit inline margin, so a
// widget that deliberately sets one still wins. It is scoped to the `<p>`
// itself (`*` over a `<p>` whose only child is text), NOT to the widget's
// subtree: a container widget's user content must keep its paragraph margins.
//
// `widget_text_carriers_do_not_inherit_the_ua_paragraph_margin` below fails
// for any widget `<p>` built without these helpers.

/// The component sheet [`widget_p`] attaches: `margin-top: 0; margin-bottom: 0`
/// at AUTHOR priority on a `*` path.
pub(crate) fn widget_p_margin_reset() -> azul_css::css::Css {
    use azul_css::{
        css::{Css, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock, rule_priority},
        props::{
            layout::{LayoutMarginBottom, LayoutMarginTop},
            property::CssProperty,
        },
    };
    Css {
        rules: vec![CssRuleBlock {
            path: CssPath {
                selectors: vec![CssPathSelector::Global].into(),
            },
            declarations: vec![
                CssDeclaration::Static(CssProperty::const_margin_top(LayoutMarginTop::const_px(0))),
                CssDeclaration::Static(CssProperty::const_margin_bottom(LayoutMarginBottom::const_px(0))),
            ]
            .into(),
            conditions: Vec::new().into(),
            priority: rule_priority::AUTHOR,
        }]
        .into(),
        ..Css::default()
    }
}

/// A `<p>` that carries a widget's OWN text (not a paragraph of the app's
/// prose): `Dom::create_p()` with the UA paragraph margin reset attached as
/// the node's component sheet. Set the rest of the style as usual —
/// `with_css_props` replaces the inline style and leaves the sheet alone.
#[must_use]
pub(crate) fn widget_p() -> azul_core::dom::Dom {
    azul_core::dom::Dom::create_p().with_component_css(widget_p_margin_reset())
}

/// [`widget_p`] with a text child — the widget-owned twin of
/// `Dom::create_p_with_text`.
#[must_use]
pub(crate) fn widget_p_with_text<S: Into<azul_css::AzString>>(text: S) -> azul_core::dom::Dom {
    widget_p().with_child(azul_core::dom::Dom::create_text_do_not_use_without_block_level_wrapper(text))
}

/// A widget telling its caller that only THEY can supply the missing piece.
///
/// Two warnings exist for accessibility and they are deliberately different:
///
/// * **This one, from the widget.** A widget knows its own type and its own
///   builder API, so it can name the exact call — "Slider has no accessible
///   name; use `.with_accessibility_name(..)`". It fires at BUILD time, from
///   inside the widget, and it can be specific in a way nothing downstream can.
/// * **`dom_lint::warn_a11y_shape`, from the framework.** That one sees only
///   nodes, long after any widget has finished, and speaks in terms of the DOM:
///   "node 40 has role Slider and no value". It catches hand-built DOMs and
///   third-party widgets the engine has never heard of.
///
/// Neither subsumes the other. The widget's warning is actionable and narrow;
/// the framework's is universal and structural.
///
/// Silent when the widget got a name, and suppressible with
/// `AZ_SUPPRESS=a11y_widget` (or `AZ_SUPPRESS=all`).
#[cfg(feature = "std")]
pub fn warn_widget_needs_a_name(widget_type: &str, has_name: bool) {
    if has_name || crate::dom_lint::lint_suppressed("a11y_widget") {
        return;
    }
    azul_core::diagnostics::emit(alloc::format!(
        "[azul][a11y-widget] {widget_type} was built without an accessible name. \
         It has no text of its own to derive one from, so a screen reader \
         announces its ROLE and nothing else. Only the caller knows what this \
         control is called — add it at the call site with \
         `.with_accessibility_name(\"…\")`, which MERGES and leaves the \
         {widget_type}'s own role, value and state intact, or point at an \
         existing label with `.with_accessibility_labelled_by(node)`. \
         (suppress with AZ_SUPPRESS=a11y_widget)"
    ));
}

#[cfg(not(feature = "std"))]
pub fn warn_widget_needs_a_name(_widget_type: &str, _has_name: bool) {}

#[allow(clippy::too_many_lines)]
#[cfg(test)]
mod ua_paragraph_margin {
    //! Workspace-level guard for the bug class "a widget's text carrier inherits
    //! the UA paragraph margin" (demo test 2026-08-21: NumberInput 26 → 39 px,
    //! TextArea placeholder 13 px low, hello-world's counter two font-sizes
    //! tall). Every `<p>` a widget emits must either go through
    //! `widgets::widget_p` / `widget_p_with_text` (the reset rides on the
    //! node's own component sheet) or set both margins inline itself.
    //!
    //! Scoped to widget-OWNED `<p>`s by construction: `every_widget_dom` builds
    //! the widgets with their default content, so a `<p>` the walk finds is one
    //! the widget created. User prose placed inside a container widget keeps
    //! its margins — that is the reason the reset is per node, not per subtree.

    use azul_core::dom::{Dom, NodeType};
    use azul_css::{
        css::{CssDeclaration, CssPathSelector},
        props::property::CssPropertyType,
    };

    fn inline_sets(node: &Dom, ty: CssPropertyType) -> bool {
        node.root
            .style
            .iter_inline_properties()
            .any(|(p, _)| p.get_type() == ty)
    }

    /// The node's OWN component sheet (attached to this `Dom`, `*` path)
    /// declares `ty`.
    fn own_sheet_sets(node: &Dom, ty: CssPropertyType) -> bool {
        node.css.as_ref().iter().any(|sheet| {
            sheet.rules.as_ref().iter().any(|rule| {
                let global = matches!(
                    rule.path.selectors.as_ref().first(),
                    None | Some(CssPathSelector::Global)
                );
                global
                    && rule.declarations.as_ref().iter().any(|d| match d {
                        CssDeclaration::Static(p) => p.get_type() == ty,
                        CssDeclaration::Dynamic(_) => false,
                    })
            })
        })
    }

    fn walk(node: &Dom, widget: &str, path: &str, bad: &mut Vec<String>) {
        if matches!(node.root.get_node_type(), NodeType::P) {
            let top = inline_sets(node, CssPropertyType::MarginTop)
                || own_sheet_sets(node, CssPropertyType::MarginTop);
            let bottom = inline_sets(node, CssPropertyType::MarginBottom)
                || own_sheet_sets(node, CssPropertyType::MarginBottom);
            if !(top && bottom) {
                let text = node
                    .children
                    .as_ref()
                    .first()
                    .and_then(|c| match c.root.get_node_type() {
                        NodeType::Text(t) => Some(t.as_ref().as_str().to_string()),
                        _ => None,
                    })
                    .unwrap_or_default();
                bad.push(format!(
                    "{widget}: <p> at {path} ({text:?}) inherits the UA `margin: 1em 0` — build it \
                     with widgets::widget_p_with_text / widget_p, or set margin-top AND \
                     margin-bottom inline"
                ));
            }
        }
        for (i, child) in node.children.as_ref().iter().enumerate() {
            walk(child, widget, &format!("{path}/{i}"), bad);
        }
    }

    #[test]
    fn widget_text_carriers_do_not_inherit_the_ua_paragraph_margin() {
        let mut bad = Vec::new();
        for (widget, dom) in super::label_convention::every_widget_dom() {
            walk(&dom, widget, "root", &mut bad);
        }
        assert!(
            bad.is_empty(),
            "{} widget <p> node(s) inherit the UA paragraph margin:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// The helper's own contract: the reset is on the node's sheet (so a later
    /// `with_css_props` cannot wipe it), at AUTHOR priority (so an explicit
    /// inline margin still wins), and the node is still a `<p>` with its text.
    #[test]
    fn widget_p_carries_the_reset_on_its_own_sheet_and_survives_with_css_props() {
        use azul_css::{
            css::rule_priority,
            dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
            props::{basic::StyleFontSize, property::CssProperty},
        };
        let p = super::widget_p_with_text("label").with_css_props(CssPropertyWithConditionsVec::from_vec(
            vec![CssPropertyWithConditions::simple(CssProperty::const_font_size(
                StyleFontSize::const_px(12),
            ))],
        ));
        assert!(matches!(p.root.get_node_type(), NodeType::P));
        assert_eq!(p.children.as_ref().len(), 1);
        assert!(own_sheet_sets(&p, CssPropertyType::MarginTop));
        assert!(own_sheet_sets(&p, CssPropertyType::MarginBottom));
        assert!(
            !inline_sets(&p, CssPropertyType::MarginTop),
            "the reset must not live in the inline style `with_css_props` replaces"
        );
        let sheet = &p.css.as_ref()[0];
        assert_eq!(sheet.rules.as_ref()[0].priority, rule_priority::AUTHOR);
    }

    // An explicit inline margin wins over the reset: checked at compile time.
    const _: () = assert!(
        azul_css::css::rule_priority::AUTHOR < azul_css::css::rule_priority::INLINE
    );
}

#[cfg(test)]
mod label_convention {
    //! Workspace-level enforcement of the widget label convention (USER ruling,
    //! 2026-08-12): a widget must never attach state to a raw text node.
    //!
    //! `NodeType::Text` is unconditionally inline-level
    //! (`solver3::layout_tree`): it is given no rect and no `UnifiedLayout` of
    //! its own — the wrapping block box carries those. Anything attached to a
    //! text node is therefore attached to a box-less node and is silently
    //! INERT: box-model properties never paint, callbacks and `tab_index` have
    //! no hit area, and a dataset has no node to be found on.
    //!
    //! The canonical shape is `Dom::create_p_with_text(label)` (or
    //! `create_p().with_children([create_text_do_not_use_without_block_level_wrapper(label)])`) with every property on
    //! the `<p>`, or — where a dedicated styled `<div>` already is the box — a
    //! bare `create_text` leaf with the properties on that `<div>`.
    //!
    //! This generalises `ribbon`'s per-widget invariant test to every widget in
    //! the crate. Widgets that emit no text at all are still instantiated, so
    //! the list doubles as a smoke test that every `dom()` builds.

    use azul_core::dom::{Dom, NodeType};
    use azul_css::{props::basic::color::ColorU, AzString, OptionString, StringVec};

    /// Everything a node can carry that only a real box can honour, in the
    /// order the failure message lists it.
    fn inert_state_on(node: &Dom) -> Vec<&'static str> {
        let mut found = Vec::new();
        if !node.root.style.rules.as_ref().is_empty() {
            found.push("css props");
        }
        // A subtree stylesheet on a childless text node can only target the text
        // node itself (`with_css("width: …")` parses to `* { … }`), so it is the
        // same violation wearing the other API.
        if !node.css.as_ref().is_empty() {
            found.push("subtree css");
        }
        if !node.root.get_callbacks().as_ref().is_empty() {
            found.push("callbacks");
        }
        if node.root.get_tab_index().is_some() {
            found.push("tab_index");
        }
        if node.root.get_dataset().is_some() {
            found.push("dataset");
        }
        if !node.children.as_ref().is_empty() {
            found.push("children");
        }
        found
    }

    fn walk(node: &Dom, widget: &str, bad: &mut Vec<String>) {
        if let NodeType::Text(text) = node.root.get_node_type() {
            let found = inert_state_on(node);
            if !found.is_empty() {
                bad.push(format!(
                    "{widget}: text node {:?} carries {} — move it onto a wrapping <p> \
                     (or onto the styled <div> that already boxes it)",
                    text.as_ref().as_str(),
                    found.join(" + "),
                ));
            }
        }
        for child in node.children.as_ref() {
            walk(child, widget, bad);
        }
    }

    fn labels(items: &[&str]) -> StringVec {
        StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect::<Vec<_>>())
    }

    /// A user-content placeholder for the widgets that embed an arbitrary
    /// caller-supplied `Dom`. Deliberately property-free: this test governs
    /// what *widgets* emit, not what an application passes in.
    fn user_content() -> Dom {
        Dom::create_div()
    }

    fn node_graph_fixture() -> super::node_graph::NodeGraph {
        use super::node_graph::{
            InputConnectionVec, InputOutputInfo, InputOutputTypeId, InputOutputTypeIdInfoMap,
            InputOutputTypeIdInfoMapVec, InputOutputTypeIdVec, Node, NodeGraph, NodeGraphNodeId,
            NodeGraphNodePosition, NodeIdNodeMap, NodeIdNodeMapVec, NodeTypeField,
            NodeTypeFieldValue, NodeTypeFieldVec, NodeTypeId, NodeTypeIdInfoMap,
            NodeTypeIdInfoMapVec, NodeTypeInfo, OutputConnectionVec,
        };

        const TYPE_A: NodeTypeId = NodeTypeId { inner: 1 };
        const IO_A: InputOutputTypeId = InputOutputTypeId { inner: 1 };

        NodeGraph {
            node_types: NodeTypeIdInfoMapVec::from_vec(vec![NodeTypeIdInfoMap {
                node_type_id: TYPE_A,
                node_type_info: NodeTypeInfo {
                    is_root: true,
                    node_type_name: AzString::from("Add"),
                    inputs: InputOutputTypeIdVec::from_vec(vec![IO_A]),
                    outputs: InputOutputTypeIdVec::from_vec(vec![IO_A]),
                },
            }]),
            input_output_types: InputOutputTypeIdInfoMapVec::from_vec(vec![
                InputOutputTypeIdInfoMap {
                    io_type_id: IO_A,
                    io_info: InputOutputInfo {
                        data_type: AzString::from("number"),
                        color: ColorU { r: 0, g: 0, b: 0, a: 255 },
                    },
                },
            ]),
            nodes: NodeIdNodeMapVec::from_vec(vec![NodeIdNodeMap {
                node_id: NodeGraphNodeId { inner: 1 },
                node: Node {
                    node_type: TYPE_A,
                    position: NodeGraphNodePosition { x: 0.0, y: 0.0 },
                    fields: NodeTypeFieldVec::from_vec(vec![NodeTypeField {
                        key: AzString::from("enabled"),
                        value: NodeTypeFieldValue::CheckBox(false),
                    }]),
                    connect_in: InputConnectionVec::from_const_slice(&[]),
                    connect_out: OutputConnectionVec::from_const_slice(&[]),
                },
            }]),
            add_node_str: AzString::from("Add node"),
            ..NodeGraph::default()
        }
    }

    /// Every widget in the crate, built with defaults that actually exercise
    /// its label paths (a widget with no labels proves nothing).
    ///
    /// NOT in this list, and why:
    /// * `camera` / `microphone` / `screencap` / `video` — each `dom()` emits a
    ///   single replaced `<img>` (or nothing) fed by a background worker and
    ///   needs a device/GL config to construct; they contain no text node at
    ///   all, so there is nothing for this convention to govern.
    /// * `menubar` — a free function over a window `Menu`, not a `dom()` widget;
    ///   its bar items are already `div > bare text`.
    /// * `map`'s tile labels — emitted from the `VirtualView` render callback,
    ///   not from `dom()`, so the walk cannot reach them; they were converted by
    ///   hand and are pinned by the map widget's own tests.
    pub(super) fn every_widget_dom() -> Vec<(&'static str, Dom)> {
        use super::{
            accordion::{Accordion, AccordionSection, AccordionSectionVec},
            alert::Alert,
            avatar::Avatar,
            backstage::{Backstage, BackstageNavItem, BackstageNavItemVec},
            badge::Badge,
            breadcrumb::Breadcrumb,
            button::Button,
            card::Card,
            check_box::CheckBox,
            chip::Chip,
            color_input::ColorInput,
            combobox::ComboBox,
            date_picker::DatePicker,
            divider::Divider,
            drop_down::DropDown,
            file_input::FileInput,
            frame::Frame,
            label::Label,
            list_view::ListView,
            map::{MapTileLayer, MapWidget},
            menubar::build_menubar_dom,
            modal::Modal,
            number_input::NumberInput,
            pagination::Pagination,
            popover::Popover,
            progressbar::ProgressBar,
            quick_access::QuickAccessBar,
            radio_group::RadioGroup,
            ribbon::{Ribbon, RibbonAppButton, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec},
            segmented::Segmented,
            slider::Slider,
            spinner::Spinner,
            split_pane::{SplitDirection, SplitPane},
            statusbar::{StatusBar, StatusBarSegment, StatusBarSegmentVec},
            stepper::Stepper,
            switch::Switch,
            tabs::{TabContent, TabHeader},
            text_area::TextArea,
            text_input::TextInput,
            time_picker::TimePicker,
            titlebar::Titlebar,
            toast::Toast,
            tooltip::Tooltip,
            tree_view::{TreeView, TreeViewNode},
        };

        vec![
            (
                "accordion",
                Accordion::new(AccordionSectionVec::from_vec(vec![
                    AccordionSection::new("Open section", user_content()).with_open(true),
                    AccordionSection::new("Closed section", user_content()),
                ]))
                .dom(),
            ),
            (
                "alert",
                Alert::create(AzString::from("Something happened"))
                    .with_dismissible(true)
                    .dom(),
            ),
            ("avatar", Avatar::create(AzString::from("AB")).dom()),
            (
                "backstage",
                Backstage::new(BackstageNavItemVec::from_vec(vec![
                    BackstageNavItem::new(AzString::from("Info")),
                    BackstageNavItem::new(AzString::from("Save")),
                ]))
                .dom(),
            ),
            ("badge", Badge::create(AzString::from("99+")).dom()),
            (
                "breadcrumb",
                Breadcrumb::create(labels(&["Home", "Docs", "Page"])).dom(),
            ),
            ("button", Button::create(AzString::from("Click me")).dom()),
            ("card", Card::create(user_content()).dom()),
            ("check_box", CheckBox::create(true).dom()),
            (
                "chip",
                Chip::create(AzString::from("tag")).with_removable(true).dom(),
            ),
            (
                "color_input",
                ColorInput::create(ColorU { r: 1, g: 2, b: 3, a: 255 }).dom(),
            ),
            ("combobox", ComboBox::new(labels(&["one", "two"])).dom()),
            ("date_picker", DatePicker::create(2024, 2, 15).dom()),
            ("divider", Divider::create().dom()),
            ("drop_down", DropDown::new(labels(&["one", "two"])).dom()),
            ("file_input", FileInput::create(OptionString::None).dom()),
            (
                "frame",
                Frame::create(AzString::from("Frame title"), user_content()).dom(),
            ),
            ("label", Label::create(AzString::from("A label")).dom()),
            ("list_view", ListView::create(labels(&["Name", "Size"])).dom()),
            ("map", MapWidget::create(MapTileLayer::default()).dom()),
            (
                "menubar",
                build_menubar_dom(&azul_core::menu::Menu::create(
                    azul_core::menu::MenuItemVec::from_vec(vec![
                        azul_core::menu::MenuItem::String(
                            azul_core::menu::StringMenuItem::create("File".into()),
                        ),
                        azul_core::menu::MenuItem::String(
                            azul_core::menu::StringMenuItem::create("Edit".into()),
                        ),
                    ]),
                )),
            ),
            (
                "modal",
                Modal::create(user_content())
                    .with_title(AzString::from("Dialog"))
                    .with_open(true)
                    .dom(),
            ),
            ("node_graph", node_graph_fixture().dom()),
            ("number_input", NumberInput::create(4.0).dom()),
            ("pagination", Pagination::create(2, 5).dom()),
            (
                "popover",
                Popover::new(user_content(), user_content()).with_open(true).dom(),
            ),
            ("progressbar", ProgressBar::create(40.0).dom()),
            (
                "quick_access",
                QuickAccessBar::new(AzString::from("Document1")).dom(),
            ),
            (
                "radio_group",
                RadioGroup::create(labels(&["First", "Second"])).dom(),
            ),
            (
                "ribbon",
                Ribbon::new(RibbonTabVec::from_vec(vec![
                    RibbonTab::new(AzString::from("HOME")).with_group(
                        RibbonGroup::new(AzString::from("Clipboard")).with_item(
                            RibbonItem::LargeButton(RibbonButton::new(
                                AzString::from("content_paste"),
                                AzString::from("Paste"),
                            )),
                        ),
                    ),
                    RibbonTab::new(AzString::from("PAGE LAYOUT")),
                ]))
                .with_app_button(RibbonAppButton::new(AzString::from("FILE")))
                .dom(),
            ),
            (
                "segmented",
                Segmented::create(labels(&["Day", "Week", "Month"])).dom(),
            ),
            ("slider", Slider::create(0.5, 0.0, 1.0).dom()),
            ("spinner", Spinner::create().dom()),
            (
                "split_pane",
                SplitPane::create(SplitDirection::Horizontal, user_content(), user_content()).dom(),
            ),
            (
                "statusbar",
                StatusBar::new(StatusBarSegmentVec::from_vec(vec![
                    StatusBarSegment::new(AzString::from("Page 1 of 3")),
                ]))
                .dom(),
            ),
            (
                "stepper",
                Stepper::create(labels(&["Start", "Details", "Done"])).dom(),
            ),
            ("switch", Switch::create(true).dom()),
            ("tabs (header)", TabHeader::create(labels(&["One", "Two"])).dom()),
            ("tabs (content)", TabContent::new(user_content()).dom()),
            ("text_area", TextArea::create().dom()),
            ("text_input", TextInput::create().dom()),
            (
                "time_picker",
                TimePicker::create(9, 30).with_24h(false).dom(),
            ),
            ("titlebar", Titlebar::create(AzString::from("Window")).dom()),
            ("toast", Toast::create(AzString::from("Saved")).dom()),
            (
                "tooltip",
                Tooltip::new(user_content(), AzString::from("Explains it")).dom(),
            ),
            (
                "tree_view",
                TreeView::new(
                    TreeViewNode::new("root")
                        .with_expanded(true)
                        .with_child(TreeViewNode::new("child")),
                )
                .dom(),
            ),
        ]
    }

    /// THE convention. A widget that trips this has attached box-model CSS, a
    /// callback, a `tab_index`, a dataset or children to a node that owns no
    /// rect — all of which the layout engine silently discards.
    #[test]
    fn no_widget_attaches_state_to_a_rect_less_text_node() {
        let mut bad = Vec::new();
        for (name, dom) in every_widget_dom() {
            walk(&dom, name, &mut bad);
        }
        assert!(
            bad.is_empty(),
            "widget label convention violated ({} site(s)):\n{}",
            bad.len(),
            bad.join("\n"),
        );
    }

    /// A guard on the guard: the walk must be able to SEE a violation, or the
    /// test above would pass vacuously the day someone breaks `inert_state_on`.
    #[test]
    fn the_walk_reports_a_deliberately_broken_text_node() {
        use azul_core::dom::TabIndex;

        let mut leaf = Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from("bare"));
        leaf.root.set_css("width: 10px;");
        let broken = Dom::create_div().with_child(leaf.with_tab_index(TabIndex::Auto));

        let mut bad = Vec::new();
        walk(&broken, "fixture", &mut bad);

        assert_eq!(bad.len(), 1, "the walk missed a hand-broken text node");
        assert!(bad[0].contains("css props"), "{}", bad[0]);
        assert!(bad[0].contains("tab_index"), "{}", bad[0]);
    }
}
