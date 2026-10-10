//! The small DOM pieces the Azlin apps build their screens from (feature `look`, in `azul`):
//! a text run, a styled block, a flex column / row, buttons, a `StringVec` of
//! labels. One copy for every app (AzContacts, AzKeys, the kit's own settings
//! page; AzNews next) instead of one per app.
//!
//! `flex_row` is not [`crate::look::row`]: that one is a settings row (a label
//! column and a control); this one lays any children side by side.

use azul::{
    callbacks::ButtonOnClickCallbackType, prelude::*, str::String as AzString, vec::StringVec,
    widgets::ButtonType,
};

/// The labels as a `StringVec` (a drop-down's choices, a segmented control's): keys the layout
/// pass translates, or plain words ([`crate::l10n::label`]).
#[must_use]
pub fn strs(items: &[&str]) -> StringVec {
    crate::l10n::labels(items)
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
