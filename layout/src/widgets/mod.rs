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
                // Defer to Ord below: the two bodies were identical, and one
                // copy per widget callback type is 55 chances to diverge.
                Some(self.cmp(other))
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

/// Accordion / expander widget.
///
/// One or more collapsible titled sections; see `accordion.rs`.
pub mod accordion;
/// Alert / banner widget.
///
/// A coloured inline message box with an optional dismissible close button; see `alert.rs`.
pub mod alert;
/// Avatar widget.
///
/// A circular image/initials badge (stateless); see `avatar.rs`.
pub mod avatar;
/// Office-style backstage view (the full-window "FILE" screen): accent nav
/// column + back ring + app-provided pane content. the Office-2013-era look look by default;
/// pairs with the ribbon's `RibbonAppButton`. See `backstage.rs`.
pub mod backstage;
/// Badge widget.
///
/// A small rounded count/status pill (stateless); see `badge.rs`.
pub mod badge;
/// Breadcrumb widget.
///
/// A horizontal trail of clickable crumb links separated by "/", ending in the current
/// (non-clickable) page; see `breadcrumb.rs`.
pub mod breadcrumb;
/// Button widget
pub mod button;
/// Camera-preview widget (P6) — a "dumb widget" owning a background capture
/// thread + a GL-texture ImageRef; no camera logic in core.
///
/// Same RefAny-
/// dataset + merge-callback design as the map widget. See `camera.rs`.
pub mod camera;
/// Shared core for the video-ish widgets (camera/screencap/video): the
/// `VideoFrame` type + the GL-texture `present_frame` writeback.
///
/// See
/// `capture_common.rs`.
pub mod capture_common;
/// Card container widget.
///
/// Elevated/bordered content box (no title); see `card.rs`.
pub mod card;
/// Checkbox widget
pub mod check_box;
/// Chip / tag widget.
///
/// A compact rounded pill with a label + optional removable "x" (stateful when removable, mirrors
/// alert's dismiss); see `chip.rs`.
pub mod chip;
/// Box displaying a color with a callback for value changes
pub mod color_input;
/// Combobox widget.
///
/// An editable text field with a click-toggled drop-down list of options (drop_down's select +
/// text_input's editable field); see `combobox.rs`.
pub mod combobox;
/// Calendar date picker widget.
///
/// A month header (‹ / `Month YYYY` / ›) above a weekday-labelled 7-column day grid computed from
/// real calendar math; clicking a day selects + restyles it (segmented-style), and the per-cell day
/// number is carried drop_down-style. Month nav fires on_change but cannot rebuild the grid
/// in-widget (prominent module TODO2); see `date_picker.rs`.
pub mod date_picker;
/// Divider / separator rule widget (horizontal or vertical).
///
/// See `divider.rs`.
pub mod divider;
/// Drop-down select widget
pub mod drop_down;
/// File input widget
pub mod file_input;
/// Frame container widget
pub mod frame;
/// Label widget (centered text)
pub mod label;
/// List view widget
pub mod list_view;
/// Map widget — MVT tile + MapCSS → SVG → DOM (AzulMaps goal app, P3).
///
/// Cache lives in a dataset RefAny owned by a merge callback so it
/// survives relayout. See `layout/src/widgets/map.rs` for the design.
pub mod map;
/// Built-in map themes (vendored `MapCSS` palettes, see the file header for licences).
pub mod map_themes;
/// Software menu-bar widget (Linux fallback when there is no native global menu).
///
/// Renders a window's `Menu` as a horizontal bar; items open dropdowns via the
/// unified `WindowPosition::RelativeToParentWindow` popup path.
pub mod menubar;
/// Microphone-capture widget (P7) — same "dumb widget" architecture as the
/// capture widgets, audio instead of video (no GL): a background thread feeds
/// each `AudioFrame` to the user's `on_frame` hook.
///
/// See `microphone.rs`.
pub mod microphone;
/// Modal / dialog widget.
///
/// An in-app overlay dialog (backdrop + centred panel + arbitrary content), shown/hidden via state
/// toggle; see `modal.rs`.
pub mod modal;
/// Node graph widget
pub mod node_graph;
/// Same as text input, but only allows numeric input
pub mod number_input;
/// Pagination widget.
///
/// A `Prev` / page-numbers / `Next` page navigator with an active-page restyle (segmented-style);
/// see `pagination.rs`.
pub mod pagination;
/// Popover widget.
///
/// A click-triggered floating panel holding arbitrary content, anchored to a `Dom` (the
/// click-toggled sibling of tooltip); see `popover.rs`.
pub mod popover;
/// Progress bar widget
pub mod progressbar;
/// Office-style title band with a Quick Access Toolbar (save/undo/redo),
/// centered title and window buttons, drawn as DOM. the Office-2013-era look look by
/// default; use `titlebar` instead for native-caption windows. See
/// `quick_access.rs`.
pub mod quick_access;
/// Radio-group widget.
///
/// Vertical/horizontal group of mutually-exclusive options (exactly one selected) with a circular
/// indicator; see `radio_group.rs`.
pub mod radio_group;
/// Ribbon widget
pub mod ribbon;
/// Screen-capture widget (P6) — identical "dumb widget" architecture to the
/// camera widget, capturing a display/window instead.
///
/// See `screencap.rs`.
pub mod screencap;
/// Segmented control widget.
///
/// Joined row of mutually-exclusive buttons; see `segmented.rs`.
pub mod segmented;
/// Slider / range widget.
///
/// Draggable thumb on a track → numeric value; see `slider.rs`.
pub mod slider;
/// Spinner / activity widget.
///
/// A static indeterminate busy ring (stateless; no animation — see the file's PARTIAL/TODO2 note);
/// see `spinner.rs`.
pub mod spinner;
/// Split-pane / splitter widget.
///
/// A two-pane (horizontal/vertical) container with a draggable divider that live-resizes the panes
/// via `set_css_property` (the frame two-box layout + the map/slider pointer-drag state machine);
/// see `split_pane.rs`.
pub mod split_pane;
/// Office-style status bar: left text segments, view switcher, zoom cluster
/// (embeds the `slider` widget). the Office-2013-era look look by default. See `statusbar.rs`.
pub mod statusbar;
/// Stepper / wizard widget.
///
/// A horizontal numbered-step progress indicator with connector lines and an accent/muted restyle
/// on step change (segmented-style + progressbar-style filled connector); see `stepper.rs`.
pub mod stepper;
/// Switch / toggle widget.
///
/// Boolean on/off with a sliding knob; see `switch.rs`.
pub mod switch;
/// Tab container widgets
pub mod tabs;
/// Multi-line text input (text area) widget.
///
/// See `text_area.rs`.
pub mod text_area;
/// Single line text input widget
pub mod text_input;
/// Time picker widget.
///
/// Two clamped numeric up/down spinners (hour + minute) side by side with an optional AM/PM toggle
/// for 12-hour mode (the number_input clamp/retext path + segmented's clickable-cell navigation);
/// see `time_picker.rs`.
pub mod time_picker;
/// Titlebar widget for custom window chrome
pub mod titlebar;
/// Toast / snackbar widget.
///
/// A transient floating notification banner pinned to a corner, manually dismissed via "x"
/// (auto-timeout needs a host timer — see the file's TODO2); a near-clone of `alert.rs` positioned
/// as an overlay; see `toast.rs`.
pub mod toast;
/// Tooltip widget.
///
/// Shows a small text popup near an anchor on hover; see `tooltip.rs`.
pub mod tooltip;
/// Tree view widget
pub mod tree_view;
/// Video-playback widget (P6) — same "dumb widget" architecture, decoding a
/// video source (vk-video) into a GL texture.
///
/// See `video.rs`.
pub mod video;
// /// Spreadsheet (virtualized view) widget
// pub mod spreadsheet;

/// Every shipped widget's `dom()` with reasonable defaults, for lints that
/// must hold across the whole widget set (the label-convention test below and
/// `dom_lint`'s runtime-warning twin). Test-only.
#[cfg(test)]
pub(crate) fn all_widget_doms_for_lint() -> Vec<(&'static str, azul_core::dom::Dom)> {
    label_convention::every_widget_dom()
}

/// Reading a rendered node's inline style apart from the theme's contribution.
///
/// A widget resolves its own style; the theme module then appends the
/// declarations only it can write — the dark twin of a colour, the hover and
/// pressed faces — on top of that. So a test that compares a rendered node
/// against the widget's own style cannot use raw equality: it has to say which
/// half it is looking at. These two helpers are that distinction, shared so
/// each widget's suite does not grow its own copy of the predicate.
#[cfg(test)]
pub(crate) mod theme_probe {
    use azul_core::dom::Dom;
    use azul_css::{
        dynamic_selector::{DynamicSelector, DynamicSelectorVec, ThemeCondition},
        props::property::CssProperty,
    };

    /// True if a declaration is gated on a theme, i.e. it is one half of a
    /// light/dark pair rather than something that applies in every mode.
    fn is_theme_gated(conditions: &DynamicSelectorVec) -> bool {
        conditions
            .as_ref()
            .iter()
            .any(|c| matches!(c, DynamicSelector::Theme(_)))
    }

    /// The node's inline declarations that apply in EVERY theme, in declaration
    /// order — what the widget itself resolved, with the theme's mode-specific
    /// additions left out. Pair it with [`dark`] so skipping them here cannot
    /// hide a theme that forgot its dark half.
    pub(crate) fn unthemed(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .filter(|(_, c)| !is_theme_gated(c))
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The node's inline declarations that apply in EVERY state and theme, in
    /// declaration order — the widget's resting style.
    ///
    /// Distinct from [`unthemed`], which keeps `:hover`-style rules and drops only
    /// the theme-gated ones. Phase 2 of the widget theme migration moves hover,
    /// pressed and focus rules into the theme modules, so a rendered node now
    /// carries conditional declarations the widget itself never declared; a test
    /// comparing "what landed on this node" against a widget's style wants this.
    pub(crate) fn unconditional(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .filter(|(_, c)| c.as_ref().is_empty())
            .map(|(p, _)| p.clone())
            .collect()
    }

    /// The node's inline declarations that apply only in dark mode.
    pub(crate) fn dark(dom: &Dom) -> Vec<CssProperty> {
        dom.root
            .style
            .iter_inline_properties()
            .filter(|(_, c)| {
                c.as_ref()
                    .iter()
                    .any(|s| matches!(s, DynamicSelector::Theme(ThemeCondition::Dark)))
            })
            .map(|(p, _)| p.clone())
            .collect()
    }
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
/// at AUTHOR priority on a `*` path, plus whatever `extra` the caller adds.
fn widget_p_sheet(extra: Vec<azul_css::css::CssDeclaration>) -> azul_css::css::Css {
    use azul_css::{
        css::{rule_priority, Css, CssDeclaration, CssPath, CssPathSelector, CssRuleBlock},
        props::{
            layout::{LayoutMarginBottom, LayoutMarginTop},
            property::CssProperty,
        },
    };
    let mut declarations = vec![
        CssDeclaration::Static(CssProperty::const_margin_top(LayoutMarginTop::const_px(0))),
        CssDeclaration::Static(CssProperty::const_margin_bottom(
            LayoutMarginBottom::const_px(0),
        )),
    ];
    declarations.extend(extra);
    Css {
        rules: vec![CssRuleBlock {
            path: CssPath {
                selectors: vec![CssPathSelector::Global].into(),
            },
            declarations: declarations.into(),
            conditions: Vec::new().into(),
            priority: rule_priority::AUTHOR,
        }]
        .into(),
        ..Css::default()
    }
}

/// The component sheet [`widget_p`] attaches: `margin-top: 0; margin-bottom: 0`
/// at AUTHOR priority on a `*` path.
pub(crate) fn widget_p_margin_reset() -> azul_css::css::Css {
    widget_p_sheet(Vec::new())
}

/// [`widget_p_margin_reset`] plus `user-select: none` — the sheet a CHROME text
/// carrier gets.
///
/// A widget's OWN text is chrome: a button's label, a tab's caption, a menu
/// item, a dropdown's current value, a stepper's step name. No toolkit lets a
/// drag across those paint a text selection, and azul's default is the
/// opposite — `is_text_selectable` answers "selectable" for anything that does
/// not say otherwise — so every widget label in the tree was draggable text.
/// The rule is stated once, here, because every widget-owned carrier goes
/// through [`widget_p_with_text`] / [`widget_p_chrome`].
///
/// AUTHOR priority, like the margin reset, so a widget that deliberately wants
/// its text selectable can still say so inline. The `*` path scopes to the
/// `<p>`'s subtree — the `<p>` and the text node under it — which is what the
/// pointer path asks about: it tests the HIT node, deepest first.
///
/// NOT on [`widget_p`] itself: that is what TextInput and TextArea build their
/// editable text on, and the user's own content is selectable by definition.
pub(crate) fn widget_p_chrome_sheet() -> azul_css::css::Css {
    use azul_css::{
        css::CssDeclaration,
        props::{property::CssProperty, style::text::StyleUserSelect},
    };
    widget_p_sheet(vec![CssDeclaration::Static(CssProperty::user_select(
        StyleUserSelect::None,
    ))])
}

/// A `<p>` that carries a widget's OWN text (not a paragraph of the app's
/// prose): `Dom::create_p()` with the UA paragraph margin reset attached as
/// the node's component sheet. Set the rest of the style as usual —
/// `with_css_props` replaces the inline style and leaves the sheet alone.
#[must_use]
pub(crate) fn widget_p() -> azul_core::dom::Dom {
    azul_core::dom::Dom::create_p().with_component_css(widget_p_margin_reset())
}

/// [`widget_p`] for text the WIDGET owns rather than text the user typed: the
/// same margin reset plus `user-select: none` (see [`widget_p_chrome_sheet`]).
///
/// Every widget label goes through this or through [`widget_p_with_text`]; the
/// only carriers that deliberately keep plain [`widget_p`] are TextInput's and
/// TextArea's, whose text is the user's content.
#[must_use]
pub(crate) fn widget_p_chrome() -> azul_core::dom::Dom {
    azul_core::dom::Dom::create_p().with_component_css(widget_p_chrome_sheet())
}

/// [`widget_p_chrome`] with a text child — the widget-owned twin of
/// `Dom::create_p_with_text`.
#[must_use]
pub(crate) fn widget_p_with_text<S: Into<azul_css::AzString>>(text: S) -> azul_core::dom::Dom {
    widget_p_chrome()
        .with_child(azul_core::dom::Dom::create_text_do_not_use_without_block_level_wrapper(text))
}

/// A widget telling its caller that only THEY can supply the missing piece.
///
/// Two warnings exist for accessibility and they are deliberately different:
///
/// * **This one, from the widget.** A widget knows its own type and its own builder API, so it can
///   name the exact call — "Slider has no accessible name; use `.with_accessibility_name(..)`". It
///   fires at BUILD time, from inside the widget, and it can be specific in a way nothing
///   downstream can.
/// * **`dom_lint::warn_a11y_shape`, from the framework.** That one sees only nodes, long after any
///   widget has finished, and speaks in terms of the DOM: "node 40 has role Slider and no value".
///   It catches hand-built DOMs and third-party widgets the engine has never heard of.
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
        "[azul][a11y-widget] {widget_type} was built without an accessible name. It has no text \
         of its own to derive one from, so a screen reader announces its ROLE and nothing else. \
         Only the caller knows what this control is called — add it at the call site with \
         `.with_accessibility_name(\"…\")`, which MERGES and leaves the {widget_type}'s own role, \
         value and state intact, or point at an existing label with \
         `.with_accessibility_labelled_by(node)`. (suppress with AZ_SUPPRESS=a11y_widget)"
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
                    "{widget}: <p> at {path} ({text:?}) inherits the UA `margin: 1em 0` — build \
                     it with widgets::widget_p_with_text / widget_p, or set margin-top AND \
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
        let p = super::widget_p_with_text("label").with_css_props(
            CssPropertyWithConditionsVec::from_vec(vec![CssPropertyWithConditions::simple(
                CssProperty::const_font_size(StyleFontSize::const_px(12)),
            )]),
        );
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
    const _: () =
        assert!(azul_css::css::rule_priority::AUTHOR < azul_css::css::rule_priority::INLINE);
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
    //! `create_p().with_children([create_text_do_not_use_without_block_level_wrapper(label)])`)
    //! with every property on the `<p>`, or — where a dedicated styled `<div>` already is the
    //! box — a bare `create_text` leaf with the properties on that `<div>`.
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
                    "{widget}: text node {:?} carries {} — move it onto a wrapping <p> (or onto \
                     the styled <div> that already boxes it)",
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
                        color: ColorU {
                            r: 0,
                            g: 0,
                            b: 0,
                            a: 255,
                        },
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
    /// * `camera` / `microphone` / `screencap` / `video` — each `dom()` emits a single replaced
    ///   `<img>` (or nothing) fed by a background worker and needs a device/GL config to construct;
    ///   they contain no text node at all, so there is nothing for this convention to govern.
    /// * `menubar` — a free function over a window `Menu`, not a `dom()` widget; its bar items are
    ///   already `div > bare text`.
    /// * `map`'s tile labels — emitted from the `VirtualView` render callback, not from `dom()`, so
    ///   the walk cannot reach them; they were converted by hand and are pinned by the map widget's
    ///   own tests.
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
            ribbon::{
                Ribbon, RibbonAppButton, RibbonButton, RibbonGroup, RibbonItem, RibbonTab,
                RibbonTabVec,
            },
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
                Chip::create(AzString::from("tag"))
                    .with_removable(true)
                    .dom(),
            ),
            (
                "color_input",
                ColorInput::create(ColorU {
                    r: 1,
                    g: 2,
                    b: 3,
                    a: 255,
                })
                .dom(),
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
            (
                "list_view",
                ListView::create(labels(&["Name", "Size"])).dom(),
            ),
            ("map", MapWidget::create(MapTileLayer::default()).dom()),
            (
                "menubar",
                build_menubar_dom(&azul_core::menu::Menu::create(
                    azul_core::menu::MenuItemVec::from_vec(vec![
                        azul_core::menu::MenuItem::String(azul_core::menu::StringMenuItem::create(
                            "File".into(),
                        )),
                        azul_core::menu::MenuItem::String(azul_core::menu::StringMenuItem::create(
                            "Edit".into(),
                        )),
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
                Popover::new(user_content(), user_content())
                    .with_open(true)
                    .dom(),
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
                StatusBar::new(StatusBarSegmentVec::from_vec(vec![StatusBarSegment::new(
                    AzString::from("Page 1 of 3"),
                )]))
                .dom(),
            ),
            (
                "stepper",
                Stepper::create(labels(&["Start", "Details", "Done"])).dom(),
            ),
            ("switch", Switch::create(true).dom()),
            (
                "tabs (header)",
                TabHeader::create(labels(&["One", "Two"])).dom(),
            ),
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

        let mut leaf =
            Dom::create_text_do_not_use_without_block_level_wrapper(AzString::from("bare"));
        leaf.root.set_css("width: 10px;");
        let broken = Dom::create_div().with_child(leaf.with_tab_index(TabIndex::Auto));

        let mut bad = Vec::new();
        walk(&broken, "fixture", &mut bad);

        assert_eq!(bad.len(), 1, "the walk missed a hand-broken text node");
        assert!(bad[0].contains("css props"), "{}", bad[0]);
        assert!(bad[0].contains("tab_index"), "{}", bad[0]);
    }
}
#[cfg(test)]
mod theme_pairs {
    //! Workspace-level guard for invariant I8 of the theme-chain analysis
    //! (2026-09-12): a dark twin never ships without its light half, and
    //! never BEFORE it.
    //!
    //! Inline declarations resolve last-match-wins (`prop_cache.rs`,
    //! `get_property_slow`), and the widgets rely on that ordering: the
    //! widget declares its light face, the theme module appends the
    //! `dark_theme(..)` twins after it. Two things can go wrong by hand and
    //! nothing else catches either: a twin whose light counterpart was
    //! never declared (the light window gets the UA default, the dark one a
    //! widget colour — the halves come from different files), and a twin
    //! pushed before its light value (dead under the dark theme: the later
    //! unconditional value wins there too). Build pairs with
    //! `CssPropertyWithConditions::themed*` and neither can happen; this
    //! walk catches whatever is still built by hand.
    //!
    //! Scope, mechanically: every node of every widget `dom()` in the lint
    //! manifest (`label_convention::every_widget_dom`), its inline
    //! declarations only (the theme module writes them there). For each
    //! declaration carrying a `Theme(Dark)` condition, there must be an
    //! EARLIER declaration of the same property type whose conditions are the
    //! same pseudo-states (in the same order) and no `Theme(Dark)` — the
    //! unconditional light value, the `light_theme(..)` value, or the
    //! `on_hover(..)` to a `dark_on_hover(..)`. The "vice versa" (a light
    //! colour with no twin) is NOT a violation: a surface that is its own
    //! colour keeps it in dark mode by design (see `hover_bg_both`).
    use azul_core::dom::Dom;
    use azul_css::dynamic_selector::CssPropertyWithConditions;

    /// Sites the walk flags that are KNOWN and being fixed elsewhere: one
    /// `(widget name, message prefix, reason)` per entry. Empty means every
    /// widget is clean; an entry masks every finding of that widget whose
    /// message contains the prefix (`the_known_list_masks_only_live_findings`
    /// rejects an entry that masks nothing).
    const KNOWN_HALF_PAIRS: &[(&str, &str, &str)] = &[
        // A dark text twin with NO light half is the migration's "no opinion"
        // shape: the light window takes the UA default on purpose (and since
        // the UA text colour is themed and cascaded, the twin is belt and
        // braces). Listed rather than paired, because inventing a light value
        // here would be exactly the "light value moved" the migration forbids.
        (
            "list_view",
            "node root declares a dark twin for color (states [])",
            "light text = the UA default by design; the dark twin predates the themed UA colour",
        ),
        (
            "tree_view",
            "node root/0/1 declares a dark twin for color (states [])",
            "light text = the UA default by design; the dark twin predates the themed UA colour",
        ),
        (
            "tree_view",
            "node root/1/0/1 declares a dark twin for color (states [])",
            "light text = the UA default by design; the dark twin predates the themed UA colour",
        ),
    ];

    /// Every half-pair in one node's inline declarations, as messages. The
    /// ONE message builder: the walk, the self-test and the known-list
    /// staleness check all read these, so a prefix that matched a reported
    /// finding matches here too.
    fn findings(props: &[CssPropertyWithConditions], widget: &str, path: &str) -> Vec<String> {
        let mut out = Vec::new();
        for (i, twin) in props.iter().enumerate() {
            if !twin.is_dark_twin() {
                continue;
            }
            let ty = twin.property.get_type();
            let states = twin.pseudo_state_conditions();
            let counterpart_at = props.iter().position(|p| {
                p.property.get_type() == ty
                    && p.is_light_half()
                    && p.pseudo_state_conditions() == states
            });
            match counterpart_at {
                None => out.push(format!(
                    "{widget}: node {path} declares a dark twin for {ty:?} (states {states:?}) \
                     with NO light counterpart — the light window gets the UA default here"
                )),
                Some(j) if j > i => out.push(format!(
                    "{widget}: node {path} pushes the dark twin for {ty:?} (states {states:?}) \
                     at #{i}, BEFORE its light value at #{j} — dead under the dark theme \
                     (last match wins)"
                )),
                Some(_) => {}
            }
        }
        out
    }

    fn is_known(widget: &str, msg: &str) -> bool {
        KNOWN_HALF_PAIRS
            .iter()
            .any(|(w, prefix, _)| *w == widget && msg.contains(prefix))
    }

    fn inline_props(node: &Dom) -> Vec<CssPropertyWithConditions> {
        node.root
            .style
            .iter_inline_properties()
            .map(|(p, conds)| CssPropertyWithConditions {
                property: p.clone(),
                apply_if: conds.clone(),
            })
            .collect()
    }

    /// Every finding in the widget's tree, known ones included.
    fn walk_raw(node: &Dom, widget: &str, path: &str, out: &mut Vec<String>) {
        out.extend(findings(&inline_props(node), widget, path));
        for (i, child) in node.children.as_ref().iter().enumerate() {
            walk_raw(child, widget, &format!("{path}/{i}"), out);
        }
    }

    /// The findings of every widget in the manifest, known ones included.
    fn all_raw_findings() -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        for (widget, dom) in super::label_convention::every_widget_dom() {
            let mut raw = Vec::new();
            walk_raw(&dom, widget, "root", &mut raw);
            out.extend(raw.into_iter().map(|m| (widget, m)));
        }
        out
    }

    #[test]
    fn every_widget_dark_twin_has_a_light_half_declared_before_it() {
        let bad: Vec<String> = all_raw_findings()
            .into_iter()
            .filter(|(widget, msg)| !is_known(widget, msg))
            .map(|(_, msg)| msg)
            .collect();
        assert!(
            bad.is_empty(),
            "{} half-pair(s) in the widget styles:\n  {}\n\nBuild the pair with \
             CssPropertyWithConditions::themed / themed_on_hover / themed_on_active (light \
             value first), or list the site in KNOWN_HALF_PAIRS with the reason.",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// A guard on the guard: the walk must SEE both failure shapes, and the
    /// builder's shape must be clean.
    #[test]
    fn the_walk_reports_a_missing_half_and_a_reversed_pair() {
        use azul_css::props::{basic::color::ColorU, property::CssProperty, style::StyleTextColor};
        let c = |v: u8| {
            CssProperty::const_text_color(StyleTextColor {
                inner: ColorU::rgb(v, v, v),
            })
        };
        // Missing half.
        let bad = findings(&[CssPropertyWithConditions::dark_theme(c(1))], "fixture", "root");
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("NO light counterpart"), "{}", bad[0]);
        // Reversed pair.
        let bad = findings(
            &[
                CssPropertyWithConditions::dark_theme(c(1)),
                CssPropertyWithConditions::simple(c(2)),
            ],
            "fixture",
            "root",
        );
        assert_eq!(bad.len(), 1, "{bad:?}");
        assert!(bad[0].contains("BEFORE its light value"), "{}", bad[0]);
        // The builder's shape is clean, in every state.
        let mut props: Vec<_> = CssPropertyWithConditions::themed(c(1), c(2)).into();
        props.extend(CssPropertyWithConditions::themed_on_hover(c(3), c(4)));
        props.extend(CssPropertyWithConditions::themed_on_active(c(5), c(6)));
        let bad = findings(&props, "fixture", "root");
        assert!(bad.is_empty(), "{bad:?}");
        // A hover twin is NOT paired by a resting light value.
        let bad = findings(
            &[
                CssPropertyWithConditions::simple(c(1)),
                CssPropertyWithConditions::dark_on_hover(c(2)),
            ],
            "fixture",
            "root",
        );
        assert_eq!(bad.len(), 1, "{bad:?}");
    }

    /// An entry in `KNOWN_HALF_PAIRS` that masks nothing is stale and must
    /// go: the finding it parked was fixed, so the parking is now hiding
    /// whatever appears next under the same prefix.
    #[test]
    fn the_known_list_masks_only_live_findings() {
        let raw = all_raw_findings();
        for (w, prefix, reason) in KNOWN_HALF_PAIRS {
            assert!(
                raw.iter().any(|(widget, msg)| widget == w && msg.contains(prefix)),
                "KNOWN_HALF_PAIRS entry ({w}, {prefix:?}) masks nothing any more — delete it \
                 (reason on file: {reason})"
            );
        }
    }
}

#[cfg(test)]
mod wheel_ownership {
    //! Workspace-level guard for the wheel rule (bug W1, 2026-09-21): a wheel
    //! over a CLOSED control belongs to the page, not to the control.
    //!
    //! Every platform toolkit and every browser agrees: wheeling over a
    //! closed `<select>`, over a slider or over a colour swatch scrolls the
    //! nearest scrollable ancestor and leaves the control's value alone
    //! (Chrome and Firefox both dropped wheel-to-change on `<select>`; a
    //! range input never had it). A control that listens for `Scroll` cannot
    //! honour that rule for free — the listener fires whether or not the
    //! control is focused or open — so the rule is enforced here, over the
    //! widget set as a whole: only a widget whose whole purpose IS the
    //! gesture may register a `Scroll` handler at all.
    //!
    //! Scope: the widget DOMs of the lint manifest
    //! (`all_widget_doms_for_lint`), i.e. what `dom()` emits. The map's
    //! wheel-to-zoom handler is registered inside its `VirtualView` render
    //! callback (`map::map_widget_render`, map.rs ~2802) and is therefore out
    //! of this walk's reach; the rule it obeys is the other half of W1 — a
    //! widget that DOES take the wheel must veto the page scroll — which the
    //! wheel handlers assert for themselves.
    use azul_core::{
        dom::Dom,
        events::{EventFilter, FocusEventFilter, HoverEventFilter, WindowEventFilter},
    };

    /// Names of the manifest widgets whose `dom()` registers a `Scroll`
    /// handler anywhere in its tree, in manifest order, each named once.
    fn wheel_takers() -> Vec<String> {
        fn takes_the_wheel(node: &Dom) -> bool {
            node.root.get_callbacks().as_ref().iter().any(|cb| {
                matches!(
                    cb.event,
                    EventFilter::Hover(HoverEventFilter::Scroll)
                        | EventFilter::Focus(FocusEventFilter::Scroll)
                        | EventFilter::Window(WindowEventFilter::Scroll)
                )
            })
        }
        fn walk(node: &Dom, widget: &str, out: &mut Vec<String>) {
            if takes_the_wheel(node) && !out.iter().any(|w| w == widget) {
                out.push(widget.to_string());
            }
            for child in node.children.as_ref() {
                walk(child, widget, out);
            }
        }

        let mut out = Vec::new();
        for (widget, dom) in super::all_widget_doms_for_lint() {
            walk(&dom, widget, &mut out);
        }
        out
    }

    #[test]
    fn a_closed_control_leaves_the_wheel_to_the_page() {
        // The time picker's two spinner columns are the one sanctioned
        // exception: a stepper column IS a wheel affordance, the way a native
        // time field is. Everything else — the drop-down trigger, the slider,
        // the colour swatch, the number input, the segmented control — stays
        // deaf to the wheel so the gesture reaches the scrollable ancestor.
        assert_eq!(
            wheel_takers(),
            vec!["time_picker".to_string()],
            "a widget started listening for the wheel: a closed control must leave the gesture to \
             the page under it",
        );
    }
}

pub mod themes;

#[cfg(test)]
mod chrome_text_is_not_selectable {
    //! A widget's OWN text - a button's label, a tab's caption, a menu item, a
    //! dropdown's current value - is chrome, not content. No toolkit lets a
    //! drag across it paint a text selection, and azul's default (`user-select`
    //! unset means selectable, `solver3::getters::is_text_selectable`) makes
    //! every one of them selectable.
    //!
    //! The rule this pins: text a WIDGET wrote is not selectable; text the USER
    //! put in is. The widget-owned carriers all go through
    //! `widgets::widget_p_with_text` / `widget_p_chrome`, so the rule rides on
    //! that one sheet — an editable carrier (TextInput, TextArea) deliberately
    //! keeps plain `widget_p`.

    use azul_core::{
        dom::{Dom, NodeId, NodeType},
        styled_dom::StyledDom,
    };
    use azul_css::{
        css::{Css, CssDeclaration, CssPathSelector},
        props::property::CssPropertyType,
    };

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

    /// Is `node_id` selectable, asked exactly the way the pointer path asks it
    /// (`LayoutWindow::process_mouse_click_for_selection` and the shell's
    /// drag-arming check both call this with the HIT node).
    fn selectable(sd: &StyledDom, node_id: NodeId) -> bool {
        let states = sd.styled_nodes.as_container();
        crate::solver3::getters::is_text_selectable(
            sd,
            node_id,
            &states[node_id].styled_node_state,
        )
    }

    /// `body(0) > p(1) > text(2)` out of whatever builder is handed in.
    fn styled(p: Dom) -> StyledDom {
        let mut dom = Dom::create_body().with_child(p);
        StyledDom::create(&mut dom, Css::empty())
    }

    /// Every `<p>` in the subtree, in pre-order.
    fn paragraphs<'a>(node: &'a Dom, out: &mut Vec<&'a Dom>) {
        if matches!(node.root.get_node_type(), NodeType::P) {
            out.push(node);
        }
        for child in node.children.as_ref() {
            paragraphs(child, out);
        }
    }

    /// The builder's contract: the rule rides on the node's own component
    /// sheet, where `with_css_props` (which every call site uses for its own
    /// style) cannot wipe it.
    ///
    /// EXPECTED TO FAIL TODAY: `widget_p_with_text`'s sheet declares only
    /// `margin-top` and `margin-bottom`, so `own_sheet_sets(.., UserSelect)`
    /// is `false`.
    #[test]
    fn a_widget_text_carrier_declares_user_select_on_its_own_sheet() {
        assert!(
            own_sheet_sets(
                &super::widget_p_with_text("Click me"),
                CssPropertyType::UserSelect
            ),
            "a widget's own text carrier must say it is not selectable"
        );
    }

    /// The same law through the REAL predicate, after the cascade — the
    /// question the hit path actually asks, of the `<p>` and of the text node
    /// under it.
    ///
    /// EXPECTED TO FAIL TODAY: both asserts see `true`, because nothing sets
    /// `user-select` and `is_text_selectable` defaults to selectable.
    #[test]
    fn a_widget_text_carrier_is_not_selectable_after_the_cascade() {
        let sd = styled(super::widget_p_with_text("Click me"));
        assert_eq!(sd.node_data.len(), 3, "premise: body > p > text");
        assert!(
            !selectable(&sd, NodeId::new(1)),
            "a widget's label block is chrome, not selectable text"
        );
        assert!(
            !selectable(&sd, NodeId::new(2)),
            "the glyphs under it are the thing a drag would highlight"
        );
    }

    /// The other half: ordinary prose the APP wrote stays selectable, so the
    /// fix cannot be "nothing is selectable any more".
    #[test]
    fn an_app_paragraph_is_still_selectable() {
        let sd = styled(Dom::create_p_with_text("user prose"));
        assert!(selectable(&sd, NodeId::new(1)));
        assert!(selectable(&sd, NodeId::new(2)));
    }

    /// And on a real widget, end to end: every `<p>` a Button emits is its
    /// label.
    ///
    /// EXPECTED TO FAIL TODAY for the same reason — the Button label is built
    /// with bare `widget_p()` (`themes/flat.rs`, `themes/flora.rs`), whose
    /// sheet carries the margin reset and nothing else.
    #[test]
    fn a_buttons_label_is_not_selectable() {
        let (_, dom) = super::label_convention::every_widget_dom()
            .into_iter()
            .find(|(name, _)| *name == "button")
            .expect("every_widget_dom must build a button");
        let mut ps = Vec::new();
        paragraphs(&dom, &mut ps);
        assert!(!ps.is_empty(), "premise: a Button emits a label <p>");
        for p in ps {
            assert!(
                own_sheet_sets(p, CssPropertyType::UserSelect),
                "a button's label must not be selectable"
            );
        }
    }

    /// The exception, stated so it cannot be optimised away: `widget_p` is what
    /// the EDITABLE carriers (TextInput, TextArea) build on, and their text is
    /// the user's content.
    #[test]
    fn the_plain_carrier_editables_use_stays_selectable() {
        let sd = styled(super::widget_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("typed by the user"),
        ));
        assert!(
            selectable(&sd, NodeId::new(1)),
            "widget_p is the editable carriers' base and must not forbid selection"
        );
    }
}

#[cfg(test)]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::cast_precision_loss)]
mod theme_contrast {
    //! Every widget follows the theme it is rendered in.
    //!
    //! The widget demo on a dark desktop painted a dark text field on a white
    //! card: the flat theme's fields had dark twins, most other widgets kept
    //! their light colours, and nothing noticed - every widget test asks for
    //! the widget's DECLARATIONS, none asks what a user sees.
    //!
    //! This asks the user's question. Each widget is styled under the macOS
    //! light and dark presets the way a window does it (the cascade runs
    //! under the window's context from the first pass), and for every visible
    //! text node the text colour is composited over the stack of backgrounds
    //! behind it, down to the window canvas. Two findings, both "this widget
    //! did not follow the theme":
    //!
    //! * CONTRAST below 2:1 - dark text on a dark surface, or light on light;
    //! * in the DARK theme, text on a light NEUTRAL surface (relative luminance
    //!   above 0.45, chroma below 0.25) - a light island: legible, and exactly
    //!   the white card on the dark page. A saturated surface (an accent
    //!   button, a yellow warning badge) is the widget's own colour in both
    //!   themes and does not count.
    //!
    //! The 2:1 floor separates "follows the theme" from "does not"; it is not
    //! a WCAG grade, because light values never move and some light-theme
    //! greys are deliberately quiet.
    use std::sync::Arc;

    use azul_core::{
        dom::{Dom, NodeId, NodeType},
        styled_dom::StyledDom,
    };
    use azul_css::{
        dynamic_selector::DynamicSelectorContext,
        props::{
            basic::{
                color::{ColorOrSystem, ColorU, SystemColorRef},
                PhysicalSize,
            },
            layout::LayoutDisplay,
            style::StyleBackgroundContent,
        },
        system::{defaults, SystemStyle, Theme},
        AzString,
    };

    use crate::solver3::getters;

    /// One theme to render under: its preset and the context a window
    /// builds from it.
    struct Probe {
        theme: Theme,
        style: Arc<SystemStyle>,
        ctx: DynamicSelectorContext,
    }

    fn probe(theme: Theme) -> Probe {
        let style = Arc::new(match theme {
            Theme::Light => defaults::macos_modern_light(),
            Theme::Dark => defaults::macos_modern_dark(),
        });
        let ctx = DynamicSelectorContext::from_system_style(&style).with_viewport(800.0, 600.0);
        Probe { theme, style, ctx }
    }

    type Rgb = [f32; 3];

    fn rgb(c: ColorU) -> Rgb {
        [f32::from(c.r), f32::from(c.g), f32::from(c.b)]
    }

    fn to_color(c: Rgb) -> ColorU {
        ColorU {
            r: c[0].round() as u8,
            g: c[1].round() as u8,
            b: c[2].round() as u8,
            a: 255,
        }
    }

    /// `top` (straight alpha) over an opaque `base`.
    fn over(top: ColorU, base: Rgb) -> Rgb {
        let a = f32::from(top.a) / 255.0;
        let t = rgb(top);
        [
            t[0] * a + base[0] * (1.0 - a),
            t[1] * a + base[1] * (1.0 - a),
            t[2] * a + base[2] * (1.0 - a),
        ]
    }

    /// WCAG 2 relative luminance of an sRGB colour.
    fn luminance(c: Rgb) -> f32 {
        let lin = |v: f32| {
            let v = v / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
    }

    fn contrast(a: Rgb, b: Rgb) -> f32 {
        let (la, lb) = (luminance(a), luminance(b));
        (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
    }

    fn chroma(c: Rgb) -> f32 {
        let max = c[0].max(c[1]).max(c[2]);
        let min = c[0].min(c[1]).min(c[2]);
        (max - min) / 255.0
    }

    /// The colour one background layer contributes. A gradient counts as the
    /// average of its stops; an image is unknowable here and contributes
    /// nothing.
    fn layer_color(layer: &StyleBackgroundContent, p: &Probe) -> Option<ColorU> {
        let stop = |c: &ColorOrSystem| match c {
            ColorOrSystem::Color(c) => *c,
            ColorOrSystem::System(r) => p.ctx.system_color(*r),
        };
        let average = |colors: Vec<ColorU>| -> Option<ColorU> {
            if colors.is_empty() {
                return None;
            }
            let n = colors.len() as f32;
            let sum = colors.iter().fold([0.0_f32; 4], |acc, c| {
                [
                    acc[0] + f32::from(c.r),
                    acc[1] + f32::from(c.g),
                    acc[2] + f32::from(c.b),
                    acc[3] + f32::from(c.a),
                ]
            });
            Some(ColorU {
                r: (sum[0] / n).round() as u8,
                g: (sum[1] / n).round() as u8,
                b: (sum[2] / n).round() as u8,
                a: (sum[3] / n).round() as u8,
            })
        };
        match layer {
            StyleBackgroundContent::Color(c) => Some(*c),
            StyleBackgroundContent::SystemColor(r) => Some(p.ctx.system_color(*r)),
            StyleBackgroundContent::LinearGradient(g) => {
                average(g.stops.as_ref().iter().map(|s| stop(&s.color)).collect())
            }
            StyleBackgroundContent::RadialGradient(g) => {
                average(g.stops.as_ref().iter().map(|s| stop(&s.color)).collect())
            }
            StyleBackgroundContent::ConicGradient(g) => {
                average(g.stops.as_ref().iter().map(|s| stop(&s.color)).collect())
            }
            StyleBackgroundContent::Image(_) => None,
        }
    }

    /// `node` and its ancestors, root first.
    fn root_path(sd: &StyledDom, node: NodeId) -> Vec<NodeId> {
        let hierarchy = sd.node_hierarchy.as_container();
        let mut path = vec![node];
        let mut cur = hierarchy[node].parent_id();
        while let Some(n) = cur {
            path.push(n);
            cur = hierarchy[n].parent_id();
        }
        path.reverse();
        path
    }

    /// Nothing on `path` is `display: none` or fully transparent.
    fn is_visible(sd: &StyledDom, path: &[NodeId]) -> bool {
        let states = sd.styled_nodes.as_container();
        path.iter().all(|&n| {
            !matches!(
                getters::get_display_property(sd, Some(n)),
                getters::MultiValue::Exact(LayoutDisplay::None)
            ) && getters::get_opacity(sd, n, &states[n].styled_node_state) > 0.0
        })
    }

    /// `body > dom`, cascaded under the probe's context from the first pass.
    fn styled(dom: Dom, p: &Probe) -> StyledDom {
        StyledDom::create_from_dom_with_context(
            Dom::create_body().with_child(dom),
            Some(p.ctx.clone()),
        )
    }

    /// What a user sees at a text node: the composited ink and background.
    fn seen(sd: &StyledDom, text: NodeId, p: &Probe) -> (Rgb, Rgb) {
        let states = sd.styled_nodes.as_container();
        let mut bg = rgb(p.ctx.system_color(SystemColorRef::WindowBackground));
        for n in root_path(sd, text) {
            for layer in getters::get_background_contents(sd, n, &states[n].styled_node_state) {
                if let Some(c) = layer_color(&layer, p) {
                    bg = over(c, bg);
                }
            }
        }
        let ink = getters::get_style_properties(
            sd,
            text,
            Some(&p.style),
            PhysicalSize::new(800.0, 600.0),
        )
        .color;
        (over(ink, bg), bg)
    }

    /// Every finding for one widget under one theme, as messages.
    fn findings(name: &str, dom: Dom, p: &Probe) -> Vec<String> {
        let sd = styled(dom, p);
        let nodes = sd.node_data.as_container();
        let mut out = Vec::new();
        for i in 0..nodes.len() {
            let id = NodeId::new(i);
            let NodeType::Text(text) = nodes[id].get_node_type() else {
                continue;
            };
            let label = text.as_str();
            if label.trim().is_empty() || !is_visible(&sd, &root_path(&sd, id)) {
                continue;
            }
            let (fg, bg) = seen(&sd, id, p);
            let ratio = contrast(fg, bg);
            if ratio < 2.0 {
                out.push(format!(
                    "{name} ({:?}): {label:?} reads {ratio:.2}:1 - ink {:?} on {:?}",
                    p.theme,
                    to_color(fg),
                    to_color(bg),
                ));
            } else if p.theme == Theme::Dark && luminance(bg) > 0.45 && chroma(bg) < 0.25 {
                out.push(format!(
                    "{name} (Dark): {label:?} sits on the light surface {:?} - a light island",
                    to_color(bg),
                ));
            }
        }
        out
    }

    fn assert_follow_the_theme(widgets: Vec<(&'static str, Dom)>) {
        let (light, dark) = (probe(Theme::Light), probe(Theme::Dark));
        let mut bad = Vec::new();
        for (name, dom) in widgets {
            bad.extend(findings(name, dom.clone(), &light));
            bad.extend(findings(name, dom, &dark));
        }
        assert!(
            bad.is_empty(),
            "{} widget text(s) do not follow the theme:\n  {}",
            bad.len(),
            bad.join("\n  ")
        );
    }

    /// The manifest widgets named in `names`.
    fn manifest(names: &[&str]) -> Vec<(&'static str, Dom)> {
        super::all_widget_doms_for_lint()
            .into_iter()
            .filter(|(n, _)| names.contains(n))
            .collect()
    }

    /// Feedback and tags: the widgets with a semantic colour per kind.
    const STATUS: &[&str] = &["alert", "badge", "chip", "toast", "spinner"];
    /// Surfaces that hold the application's own content.
    const CONTAINERS: &[&str] = &[
        "accordion",
        "card",
        "divider",
        "frame",
        "modal",
        "popover",
        "split_pane",
        "tabs (content)",
        "tooltip",
    ];
    /// Controls a user types into, picks from or toggles.
    const INPUTS: &[&str] = &[
        "avatar",
        "button",
        "check_box",
        "color_input",
        "combobox",
        "date_picker",
        "drop_down",
        "file_input",
        "label",
        "number_input",
        "progressbar",
        "radio_group",
        "segmented",
        "slider",
        "switch",
        "text_area",
        "text_input",
        "time_picker",
    ];
    /// Navigation and application chrome.
    const CHROME: &[&str] = &[
        "backstage",
        "breadcrumb",
        "list_view",
        "map",
        "menubar",
        "node_graph",
        "pagination",
        "quick_access",
        "ribbon",
        "statusbar",
        "stepper",
        "tabs (header)",
        "titlebar",
        "tree_view",
    ];

    /// A widget added to the manifest must land in a group, or it is simply
    /// not checked.
    #[test]
    fn every_manifest_widget_is_checked_by_exactly_one_group() {
        for (name, _) in super::all_widget_doms_for_lint() {
            let groups = [STATUS, CONTAINERS, INPUTS, CHROME]
                .iter()
                .filter(|g| g.contains(&name))
                .count();
            assert_eq!(groups, 1, "{name} is in {groups} theme-contrast group(s)");
        }
    }

    #[test]
    fn status_widgets_follow_the_theme() {
        use super::{
            alert::{Alert, AlertKind},
            badge::{Badge, BadgeKind},
            chip::{Chip, ChipKind},
            toast::{Toast, ToastKind},
        };

        let mut widgets = manifest(STATUS);
        for (name, kind) in [
            ("alert success", AlertKind::Success),
            ("alert warning", AlertKind::Warning),
            ("alert danger", AlertKind::Danger),
        ] {
            widgets.push((
                name,
                Alert::with_kind(AzString::from("Message"), kind)
                    .with_dismissible(true)
                    .dom(),
            ));
        }
        for (name, kind) in [
            ("badge primary", BadgeKind::Primary),
            ("badge success", BadgeKind::Success),
            ("badge danger", BadgeKind::Danger),
            ("badge warning", BadgeKind::Warning),
            ("badge info", BadgeKind::Info),
        ] {
            widgets.push((name, Badge::with_kind(AzString::from("New"), kind).dom()));
        }
        for (name, kind) in [
            ("chip primary", ChipKind::Primary),
            ("chip success", ChipKind::Success),
            ("chip danger", ChipKind::Danger),
            ("chip warning", ChipKind::Warning),
            ("chip info", ChipKind::Info),
        ] {
            widgets.push((
                name,
                Chip::with_kind(AzString::from("Rust"), kind)
                    .with_removable(true)
                    .dom(),
            ));
        }
        for (name, kind) in [
            ("toast success", ToastKind::Success),
            ("toast warning", ToastKind::Warning),
            ("toast danger", ToastKind::Danger),
        ] {
            widgets.push((
                name,
                Toast::with_kind(AzString::from("Saved"), kind)
                    .with_dismissible(true)
                    .dom(),
            ));
        }
        assert_follow_the_theme(widgets);
    }

    #[test]
    fn container_widgets_follow_the_theme() {
        use super::{
            accordion::{Accordion, AccordionSection, AccordionSectionVec},
            card::Card,
            frame::Frame,
            modal::Modal,
            popover::Popover,
            split_pane::{SplitDirection, SplitPane},
            tabs::TabContent,
        };

        // The manifest's containers hold an empty div; the application's own
        // text is what shows whether the SURFACE followed the theme.
        let body = || Dom::create_p_with_text("Body text");
        let mut widgets = manifest(CONTAINERS);
        widgets.push(("card + text", Card::create(body()).dom()));
        widgets.push((
            "frame + text",
            Frame::create(AzString::from("Frame title"), body()).dom(),
        ));
        widgets.push((
            "modal + text",
            Modal::create(body())
                .with_title(AzString::from("Dialog"))
                .with_open(true)
                .dom(),
        ));
        widgets.push((
            "popover + text",
            Popover::new(Dom::create_p_with_text("Anchor"), body())
                .with_open(true)
                .dom(),
        ));
        widgets.push((
            "split_pane + text",
            SplitPane::create(SplitDirection::Horizontal, body(), body()).dom(),
        ));
        widgets.push(("tabs (content) + text", TabContent::new(body()).dom()));
        widgets.push((
            "accordion + text",
            Accordion::new(AccordionSectionVec::from_vec(vec![
                AccordionSection::new("Open section", body()).with_open(true),
                AccordionSection::new("Closed section", body()),
            ]))
            .dom(),
        ));
        assert_follow_the_theme(widgets);
    }

    #[test]
    fn input_widgets_follow_the_theme() {
        use super::button::{Button, ButtonType};

        let mut widgets = manifest(INPUTS);
        for (name, kind) in [
            ("button primary", ButtonType::Primary),
            ("button secondary", ButtonType::Secondary),
            ("button success", ButtonType::Success),
            ("button danger", ButtonType::Danger),
            ("button warning", ButtonType::Warning),
            ("button info", ButtonType::Info),
            ("button link", ButtonType::Link),
        ] {
            widgets.push((name, Button::with_type(AzString::from("Go"), kind).dom()));
        }
        assert_follow_the_theme(widgets);
    }

    #[test]
    fn chrome_widgets_follow_the_theme() {
        assert_follow_the_theme(manifest(CHROME));
    }

    /// The text fields the demo showed dark-on-white: in the dark theme they
    /// sit on the desktop's FIELD colour and write in its LABEL colour, like
    /// the native fields around them - not on a hand-picked bluish grey that
    /// agrees with nothing else on the page.
    #[test]
    fn the_flat_fields_take_the_system_palette_in_the_dark_theme() {
        use super::{number_input::NumberInput, text_area::TextArea, text_input::TextInput};

        let p = probe(Theme::Dark);
        let field = p.ctx.system_color(SystemColorRef::ControlBackground);
        let label = p.ctx.system_color(SystemColorRef::Text);
        for (name, dom) in [
            (
                "text_input",
                TextInput::create().with_text(AzString::from("abc")).dom(),
            ),
            ("number_input", NumberInput::create(4.0).dom()),
            (
                "text_area",
                TextArea::create().with_text(AzString::from("abc")).dom(),
            ),
        ] {
            let sd = styled(dom, &p);
            let states = sd.styled_nodes.as_container();
            let nodes = sd.node_data.as_container();
            // The field is the first node under the body that paints a
            // surface; the ink is its first text.
            let surface = (1..nodes.len())
                .map(NodeId::new)
                .map(|n| getters::get_background_color(&sd, n, &states[n].styled_node_state))
                .find(|c| c.a > 0);
            assert_eq!(surface, Some(field), "{name}: the field surface in the dark theme");
            let ink = (1..nodes.len())
                .map(NodeId::new)
                .find(|n| matches!(nodes[*n].get_node_type(), NodeType::Text(_)))
                .map(|n| {
                    getters::get_style_properties(
                        &sd,
                        n,
                        Some(&p.style),
                        PhysicalSize::new(800.0, 600.0),
                    )
                    .color
                });
            assert_eq!(ink, Some(label), "{name}: the field's text in the dark theme");
        }
    }
}
