//! The Dom pass of a window with ONE surface (a phone): what the OS would give a window of its
//! own, the engine gives the `<transient-window>` that is shown inside the page.
//!
//! A popover, a dialog, a modal is a `<transient-window>` node of the app's DOM. On a desktop the
//! shell makes an OS window of its subtree: the title bar, the close box, "stays above the parent"
//! and "the parent is inert" are the OS's. A window with one surface shows the node itself, over the
//! page: `position: fixed`, at the box the transient manager chose (`OverlayBox`), above everything.
//! This pass is what is left of the OS's part:
//!
//! - every popup: the layer style (`position: fixed`, over the page, scrolls what does not fit);
//! - a window with a title bar (`anchor="dialog"`, or the node's `titlebar` attribute): the
//!   chrome of a SHEET. The node becomes the scrim, a modal's dimmed top layer; its content sits on
//!   a card that is a bottom sheet under 600 px of width and a centred card from there up, with the
//!   header the title bar was - the title, and a close box (a press on it is the user dismissing
//!   the window, `Dismissed` and focus back to the invoker, exactly as the OS's close box was).
//!
//! The pass is a pure function of the DOM, the presentation and the live theme, run where the app's
//! DOM is made (before the DOM is fingerprinted, like the resolution of raw form controls), so the
//! retained-layout fast paths see what the window will show. A window that opens real windows
//! ([`WindowPresentation::Windows`]) is untouched by construction: the pass returns at once.
//!
//! It is idempotent: the sheet marks itself, a second run finds it and adds nothing.

use alloc::{string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    callbacks::Update,
    dom::{
        AttributeType, Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, NodeType, TabIndex,
    },
    refany::RefAny,
    transient::{TransientAnchor, TransientDock},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    props::property::CssProperty,
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    transient::WindowPresentation,
    widgets::{
        dialog::{skin_of, DialogSkin},
        themes::OptionUiTheme,
    },
};

/// The layer an overlay is on: over everything the page paints, whatever the stacking of the page.
pub const OVERLAY_Z_INDEX: i32 = 100_000;

/// A window narrower than this (in px) shows a titled window as a bottom SHEET; from it up, as a
/// centred card (Material's compact and medium width classes meet at 600 dp).
pub const SHEET_MAX_WIDTH: f32 = 599.0;

/// The class of a popup this pass presented: its mark (so a second run leaves it as it is), and
/// what a scenario selects an open overlay by.
pub const OVERLAY_CLASS: &str = "__azul-overlay";
/// The class of a sheet this pass made.
pub const SHEET_CLASS: &str = "__azul-overlay-sheet";
/// The class of the header row of a titled sheet.
pub const SHEET_TITLE_CLASS: &str = "__azul-overlay-title";
/// The class of the close box of a titled sheet.
pub const SHEET_CLOSE_CLASS: &str = "__azul-overlay-close";
/// The class of the content of a sheet.
pub const SHEET_CONTENT_CLASS: &str = "__azul-overlay-content";

/// How deep the pass looks for popups in the DOM; below it nothing is presented (and a
/// diagnostic says so). A DOM that deep is not an app's, and the walk must not overrun the stack.
const MAX_DEPTH: usize = 512;

/// The layer style of every overlay: over the page, taken out of the flow, as big as the box the
/// manager gave it (the solver reads both insets of an axis from it) and scrolling what does not fit.
///
/// The cursor is what makes the layer a TARGET of the pointer: a node nobody listens to and that has
/// no cursor is not hit-tested, so a press on the scrim of a modal - or on the padding of a popover -
/// would reach the page under it. The OS window this layer stands in for took every press on it.
const LAYER_STYLE: &str = "position: fixed; box-sizing: border-box; cursor: default;";

/// The scrim of a modal: the dimmed top layer the card sits on, the whole window. A column, the
/// card at the bottom (a phone's sheet) and, from 600 px of width, centred (a card).
const COVER_STYLE: &str = "overflow: hidden; display: flex; flex-direction: column; \
     justify-content: flex-end; align-items: stretch; \
     @media (min-width: 600px) { justify-content: center; align-items: center; }";

/// The card of a modal: full width and rounded on top as a sheet, a centred card from 600 px.
const SHEET_STYLE: &str = "box-sizing: border-box; position: relative; display: flex; \
     flex-direction: column; min-height: 0; flex-shrink: 1; overflow: hidden; padding: 0px; \
     width: 100%; max-height: 92%; margin: 0px; \
     border-top-left-radius: 16px; border-top-right-radius: 16px; \
     border-bottom-left-radius: 0px; border-bottom-right-radius: 0px; \
     padding-bottom: env(safe-area-inset-bottom, 0px); \
     @media (min-width: 600px) { width: auto; min-width: 320px; max-width: 560px; \
     max-height: 85%; padding-bottom: 0px; border-radius: 12px; }";

/// What the sheet's content is: the rest of the card, scrolling what does not fit.
const CONTENT_STYLE: &str = "flex: 1 1 auto; min-height: 0; overflow: auto; display: block;";

/// The close box a finger can hit.
const CLOSE_TOUCH_STYLE: &str =
    "min-width: 44px; min-height: 44px; display: flex; align-items: center; \
     justify-content: center; font-size: 24px;";

/// A panel a widget made for a window of its own (the dialog's panel: its own frame, its minimum
/// and maximum width) becomes plain content of the card: the card is the frame now.
const UNFRAME_STYLE: &str = "background: transparent; border: none; border-radius: 0px; \
     box-shadow: none; min-width: 0px; max-width: 100%; width: 100%; margin: 0px; \
     box-sizing: border-box;";

fn parsed(style: &str) -> CssPropertyWithConditionsVec {
    CssPropertyWithConditionsVec::parse(style)
}

/// Appends `style`'s declarations to the inline style of `node`, after the ones it has: the later
/// declaration wins, so what a widget set does not stand against what the overlay needs.
fn append_style(node: &mut azul_core::dom::NodeData, style: &CssPropertyWithConditionsVec) {
    for property in style.as_ref() {
        node.add_css_property(property.clone());
    }
}

/// The accessible title the `<transient-window>` node names its window by (its `title`, else its
/// `aria-label`): what the OS would show in the title bar of a window of its own.
fn title_of(node: &azul_core::dom::NodeData) -> String {
    node.attributes()
        .as_ref()
        .iter()
        .find_map(|attribute| match attribute {
            AttributeType::Title(t) | AttributeType::AriaLabel(t)
                if !t.as_str().trim().is_empty() =>
            {
                Some(t.as_str().to_owned())
            }
            _ => None,
        })
        .unwrap_or_default()
}

/// Whether the window of the `<transient-window>` node has a title bar: its `titlebar` attribute,
/// else a dialog window (`anchor="dialog"`) has one and a popover none - the rule the shells apply
/// to an OS window ([`crate::transient_reconcile::transient_has_titlebar`], over a styled DOM).
fn has_titlebar(node: &azul_core::dom::NodeData, anchor: TransientAnchor) -> bool {
    node.attributes()
        .as_ref()
        .iter()
        .find_map(|attribute| match attribute {
            AttributeType::Titlebar(on) => Some(*on),
            _ => None,
        })
        .unwrap_or(anchor == TransientAnchor::Dialog)
}

fn has_class(node: &Dom, class: &str) -> bool {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c.as_class(), Some(s) if s == class))
}

/// The backdrop of the theme the DOM is built for: what dims the page under a modal. Taken from the
/// theme's own dialog skin (the colour a window-less modal always had), so a theme owns it.
fn backdrop_of(skin: &DialogSkin) -> Vec<CssPropertyWithConditions> {
    skin.backdrop
        .as_ref()
        .iter()
        .filter(|p| matches!(p.property, CssProperty::BackgroundContent(_)))
        .cloned()
        .collect()
}

/// A press on the close box of a sheet: the user dismisses the window, as the OS's close box did
/// (the `Dismissed` event, focus back to what opened it).
extern "C" fn close_box_pressed(_data: RefAny, mut info: CallbackInfo) -> Update {
    close_window_around(&mut info);
    Update::DoNothing
}

/// Closes the window the control this callback runs on is in: the nearest `<transient-window>`
/// above it that is open (a sheet can sit inside the sheet of another window), as the user
/// dismissing it. What the close box of a sheet does, and what a menu does when one of its items
/// is picked.
pub(crate) fn close_window_around(info: &mut CallbackInfo) {
    let mut node = Some(info.get_hit_node());
    for _ in 0..MAX_DEPTH {
        let Some(n) = node else { break };
        if info.is_transient_window_open(n) {
            info.set_transient_window_open(n, false);
            break;
        }
        node = info.get_parent(n);
    }
}

/// Presents every `<transient-window>` of `dom` (with `dock="popup"`, the default) as an overlay of
/// the page, for a window with the given `presentation`. Returns how many were presented. See the
/// module docs.
///
/// A DOM its builder handed over FLAT stays flat when no node of it is a `<transient-window>`; any
/// other becomes its tree here, which the fingerprint of the shell reads.
pub fn present_transient_windows(dom: &mut Dom, presentation: WindowPresentation) -> usize {
    if !presentation.is_one_surface() {
        return 0;
    }
    if dom.is_flat()
        && !dom.flat_holds_node_type(|node_type| matches!(node_type, NodeType::TransientWindow(_)))
    {
        return 0;
    }
    dom.expand_flat();
    let skin = skin_of(OptionUiTheme::None);
    let mut too_deep = false;
    let presented = present_in(dom, &skin, 0, &mut too_deep);
    if too_deep {
        azul_core::diagnostics::emit(alloc::format!(
            "[azul][warn] a DOM nests deeper than {MAX_DEPTH} levels: the popups below that depth \
             are not presented over the page"
        ));
    }
    presented
}

fn present_in(dom: &mut Dom, skin: &DialogSkin, depth: usize, too_deep: &mut bool) -> usize {
    if depth > MAX_DEPTH {
        *too_deep = true;
        return 0;
    }
    let mut presented = 0;
    let popup_anchor = match dom.root.get_node_type() {
        NodeType::TransientWindow(config) if config.dock == TransientDock::Popup => {
            Some(config.anchor)
        }
        _ => None,
    };
    if let Some(anchor) = popup_anchor {
        present_one(dom, anchor, skin);
        presented += 1;
    }
    for child in dom.children.as_mut() {
        presented += present_in(child, skin, depth + 1, too_deep);
    }
    presented
}

/// One popup: the layer, and for a window the OS would have framed, the sheet.
fn present_one(dom: &mut Dom, anchor: TransientAnchor, skin: &DialogSkin) {
    // Presented already (the shell runs the pass where the DOM is made, and the styling runs it
    // again): left as it is.
    if has_class(dom, OVERLAY_CLASS) {
        return;
    }
    let cover = matches!(anchor, TransientAnchor::Dialog | TransientAnchor::Viewport);
    let node = &mut dom.root;
    node.add_class(AzString::from_const_str(OVERLAY_CLASS));
    node.upsert_inline_css_property(CssProperty::const_position(
        azul_css::props::layout::LayoutPosition::Fixed,
    ));
    node.upsert_inline_css_property(CssProperty::const_z_index(
        azul_css::props::layout::LayoutZIndex::Integer(OVERLAY_Z_INDEX),
    ));
    append_style(node, &parsed(LAYER_STYLE));

    if !cover {
        // An anchored panel: scrolls inside the box it was given when it is taller than the room.
        node.upsert_inline_css_property(CssProperty::const_overflow_x(
            azul_css::props::layout::LayoutOverflow::Hidden,
        ));
        node.upsert_inline_css_property(CssProperty::const_overflow_y(
            azul_css::props::layout::LayoutOverflow::Auto,
        ));
        return;
    }

    // The scrim: the theme's dim, and the layout that puts the card at the bottom or in the middle.
    append_style(node, &CssPropertyWithConditionsVec::from_vec(backdrop_of(skin)));
    append_style(node, &parsed(COVER_STYLE));

    let titled = has_titlebar(&dom.root, anchor);
    let title = title_of(&dom.root);
    let mut content = core::mem::take(&mut dom.children);
    // A panel a widget made for a window of its own (the dialog's) is plain content of the card now.
    let mut has_own_dialog = false;
    for child in content.as_mut() {
        let is_dialog = child
            .root
            .get_accessibility_info()
            .is_some_and(|a| matches!(a.role, AccessibilityRole::Dialog));
        if is_dialog {
            has_own_dialog = true;
            append_style(&mut child.root, &parsed(UNFRAME_STYLE));
        }
    }

    let mut card_children: Vec<Dom> = Vec::new();
    if titled {
        if title.is_empty() {
            // No title: the close box must not sit on the content.
            card_children.push(Dom::create_div().with_css_props(skin.close_row.clone()));
        } else {
            card_children.push(
                crate::widgets::widget_p_with_text(AzString::from(title.clone()))
                    .with_ids_and_classes(
                        alloc::vec![IdOrClass::Class(AzString::from_const_str(SHEET_TITLE_CLASS))]
                            .into(),
                    )
                    .with_css_props(skin.title.clone()),
            );
        }
    }
    card_children.push(
        Dom::create_div()
            .with_ids_and_classes(
                alloc::vec![IdOrClass::Class(AzString::from_const_str(SHEET_CONTENT_CLASS))]
                    .into(),
            )
            .with_css_props(parsed(CONTENT_STYLE))
            .with_children(content),
    );
    if titled {
        // LAST in tree order, as the dialog's own: the first control of the content gets the focus.
        let mut close_style = skin.close.clone().into_library_owned_vec();
        close_style.extend(parsed(CLOSE_TOUCH_STYLE).into_library_owned_vec());
        card_children.push(
            crate::widgets::widget_p_with_text(AzString::from_const_str("\u{00D7}"))
                .with_ids_and_classes(
                    alloc::vec![IdOrClass::Class(AzString::from_const_str(SHEET_CLOSE_CLASS))]
                        .into(),
                )
                .with_css_props(CssPropertyWithConditionsVec::from_vec(close_style))
                .with_tab_index(TabIndex::Auto)
                .with_accessibility_info(AccessibilityInfo {
                    role: AccessibilityRole::PushButton,
                    accessibility_name: Some(AzString::from_const_str("Close")).into(),
                    ..Default::default()
                })
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(()),
                    Callback::from_ptr(close_box_pressed).to_core(),
                ),
        );
    }

    let mut card = Dom::create_div()
        .with_ids_and_classes(
            alloc::vec![IdOrClass::Class(AzString::from_const_str(SHEET_CLASS))].into(),
        )
        .with_css_props(skin.panel.clone());
    append_style(&mut card.root, &parsed(SHEET_STYLE));
    // A card whose content brings no dialog of its own is the dialog, named by its title.
    if !has_own_dialog {
        card = card.with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::Dialog,
            accessibility_name: Some(AzString::from(if title.is_empty() {
                String::from("Dialog")
            } else {
                title
            }))
            .into(),
            ..Default::default()
        });
    }
    let card = card.with_children(DomVec::from_vec(card_children));
    dom.children = DomVec::from_vec(alloc::vec![card]);
    dom.fixup_children_estimated();
}

#[cfg(test)]
#[path = "transient_overlay_dom_test.rs"]
mod transient_overlay_dom_test;
