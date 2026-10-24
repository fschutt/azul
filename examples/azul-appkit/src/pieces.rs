//! The small DOM pieces the Azlin apps build their screens from (feature `look`, in `azul`):
//! a text run, a styled block, a flex column / row, buttons, a `StringVec` of
//! labels. One copy for every app (AzContacts, AzPasswordManager, the kit's own settings
//! page; AzNews next) instead of one per app.
//!
//! `flex_row` is not [`crate::look::row`]: that one is a settings row (a label
//! column and a control); this one lays any children side by side.

use azul::{
    callbacks::{ButtonOnClickCallbackType, CallbackType},
    dom::AccessibilityRole,
    prelude::*,
    str::String as AzString,
    vec::{StringVec, U32Vec},
    widgets::{ButtonType, Toolbar, ToolbarEvent, ToolbarEventKind},
};

/// The labels as a `StringVec` (a drop-down's choices, a segmented control's): keys the layout
/// pass translates, or plain words ([`crate::l10n::label`]).
#[must_use]
pub fn strs(items: &[&str]) -> StringVec {
    crate::l10n::labels(items)
}

/// `dom` as a control the keyboard reaches too: a tab stop called `name` (words as they are,
/// or a key of the resources) that runs `callback` on a click - a press and release on it, or
/// Enter / Space while it has the focus. A row with a bare MouseUp callback answers the mouse
/// only: it is no tab stop, and the keyboard's activation is a click, which MouseUp never
/// hears.
#[must_use]
pub fn clickable(dom: Dom, name: &str, data: RefAny, callback: CallbackType) -> Dom {
    dom.with_tab_index(azul::dom::TabIndex::Auto)
        .with_accessibility_name(crate::l10n::label(name))
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), data, callback)
}

/// [`clickable`] that a screen reader announces as `role`.
#[must_use]
pub fn clickable_as(
    dom: Dom,
    name: &str,
    _role: AccessibilityRole,
    data: RefAny,
    callback: CallbackType,
) -> Dom {
    clickable(dom, name, data, callback)
}

/// [`clickable`] and [`clickable_as`] at the end of a builder chain: `.clickable(name, data,
/// callback)`, `.clickable_as(name, role, data, callback)`.
pub trait Clickable {
    #[must_use]
    fn clickable(self, name: &str, data: RefAny, callback: CallbackType) -> Dom;
    #[must_use]
    fn clickable_as(
        self,
        name: &str,
        role: AccessibilityRole,
        data: RefAny,
        callback: CallbackType,
    ) -> Dom;
}

impl Clickable for Dom {
    fn clickable(self, name: &str, data: RefAny, callback: CallbackType) -> Dom {
        clickable(self, name, data, callback)
    }

    fn clickable_as(
        self,
        name: &str,
        role: AccessibilityRole,
        data: RefAny,
        callback: CallbackType,
    ) -> Dom {
        clickable_as(self, name, role, data, callback)
    }
}

/// A toolbar's measured fit, kept by the app from the toolbar's Resize events: its width and
/// every tool's measured width, so the rebuilt bar puts exactly the tools that fit on it and
/// the rest in its "more" menu (an estimate cuts a tool too early or too late).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolbarFit {
    /// The bar's laid-out width, whole px; 0 until the first Resize.
    width: u32,
    /// Every tool's measured width by its index (0: it was in the "more" menu).
    widths: Vec<u32>,
}

impl ToolbarFit {
    /// Takes `event` if it is a Resize; true when it changed the fit (rebuild the window then;
    /// the same fit again must not, or the bar would rebuild itself forever).
    pub fn take(&mut self, event: &ToolbarEvent) -> bool {
        if event.kind != ToolbarEventKind::Resize {
            return false;
        }
        let widths = event.widths.as_ref().to_vec();
        if event.width == self.width && widths == self.widths {
            return false;
        }
        self.width = event.width;
        self.widths = widths;
        true
    }

    /// `toolbar` built to the fit (as it is before the first Resize).
    #[must_use]
    pub fn apply(&self, toolbar: Toolbar) -> Toolbar {
        if self.width == 0 {
            return toolbar;
        }
        toolbar
            .with_available_width(self.width as f32)
            .with_measured_widths(U32Vec::from_vec(self.widths.clone()))
    }
}

/// A run of text (an inline span).
#[must_use]
pub fn text<S: Into<AzString>>(content: S) -> Dom {
    Dom::create_span_with_text(content)
}

/// A block with `css` around one child.
#[must_use]
pub fn block(css: &str, child: Dom) -> Dom {
    Dom::create_div().with_css(css).with_child(child)
}

/// The children one under the other (a flex column), plus `css`.
#[must_use]
pub fn column(css: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(format!("display: flex; flex-direction: column; {css}"))
        .with_children(DomVec::from_vec(children))
}

/// The children side by side, centred on the cross axis (a flex row), plus `css`.
#[must_use]
pub fn flex_row(css: &str, children: Vec<Dom>) -> Dom {
    Dom::create_div()
        .with_css(format!(
            "display: flex; flex-direction: row; align-items: center; {css}"
        ))
        .with_children(DomVec::from_vec(children))
}

/// A button with the id `id` that calls `cb` with `app` when clicked.
#[must_use]
pub fn button(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(crate::l10n::label(label))
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

/// A [`button`] with an icon (a name of azul's icon set) before its label.
#[must_use]
pub fn icon_button(
    label: &str,
    icon: &str,
    id: AzString,
    app: &RefAny,
    cb: ButtonOnClickCallbackType,
) -> Dom {
    Button::create(crate::l10n::label(label))
        .with_icon(icon)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}

/// The primary (default) [`button`] of a form or a dialog.
#[must_use]
pub fn primary(label: &str, id: AzString, app: &RefAny, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create(crate::l10n::label(label))
        .with_button_type(ButtonType::Primary)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
}
