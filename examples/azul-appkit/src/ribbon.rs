//! The ribbon buttons, columns, rows and groups of the Azlin office apps (feature `azul`):
//! one builder for AzWriter, AzReader, AzShow, AzSheets, AzDrive, AzTasks and AzCalendar
//! instead of one copy per app.
//!
//! The one builder is [`callback_button`]: a button with its click data (a `RefAny`) and its
//! callback. An app whose ribbon buttons all run its command type through one callback
//! implements [`RibbonCommand`] for that type and writes `large(app, icon, label, cmd)`,
//! `small(..)`, `toggle(..)`, `large_toggle(..)`, `icon_button(..)`.

use azul::{
    callbacks::{ButtonOnClickCallbackType, RefAny},
    str::String as AzString,
    widgets::{RibbonButton, RibbonColumn, RibbonGroup, RibbonItem, RibbonRow},
};

/// An app's command a ribbon button runs: the click data of a button that runs it on the
/// app, and the one callback every such button calls.
pub trait RibbonCommand: Sized {
    /// The click data of a button that runs `self` on `app` (the app's `RefAny` and the
    /// command, in the app's own wrapper).
    fn click_data(self, app: &RefAny) -> RefAny;
    /// The callback every ribbon button of the app calls with its click data.
    fn on_click() -> ButtonOnClickCallbackType;
}

/// A ribbon button that calls `cb` with `data` when clicked.
#[must_use]
pub fn callback_button(
    icon: &str,
    label: &str,
    data: RefAny,
    cb: ButtonOnClickCallbackType,
) -> RibbonButton {
    RibbonButton::create(AzString::from(icon), crate::l10n::label(label)).with_on_click(data, cb)
}

/// A ribbon button that runs `cmd` on `app`.
#[must_use]
pub fn button<C: RibbonCommand>(app: &RefAny, icon: &str, label: &str, cmd: C) -> RibbonButton {
    callback_button(icon, label, cmd.click_data(app), C::on_click())
}

/// A large (icon over label) button that runs `cmd`.
#[must_use]
pub fn large<C: RibbonCommand>(app: &RefAny, icon: &str, label: &str, cmd: C) -> RibbonItem {
    RibbonItem::LargeButton(button(app, icon, label, cmd))
}

/// A small (icon beside label) button that runs `cmd`.
#[must_use]
pub fn small<C: RibbonCommand>(app: &RefAny, icon: &str, label: &str, cmd: C) -> RibbonItem {
    RibbonItem::SmallButton(button(app, icon, label, cmd))
}

/// A small button that runs `cmd` and shows a state (pressed in when `on`).
#[must_use]
pub fn toggle<C: RibbonCommand>(
    app: &RefAny,
    icon: &str,
    label: &str,
    cmd: C,
    on: bool,
) -> RibbonItem {
    RibbonItem::SmallButton(button(app, icon, label, cmd).with_toggled(on))
}

/// A large button that runs `cmd` and shows a state (pressed in when `on`).
#[must_use]
pub fn large_toggle<C: RibbonCommand>(
    app: &RefAny,
    icon: &str,
    label: &str,
    cmd: C,
    on: bool,
) -> RibbonItem {
    RibbonItem::LargeButton(button(app, icon, label, cmd).with_toggled(on))
}

/// An icon-only small button (the Font / Paragraph rows of Word, Excel and PowerPoint),
/// named `name` for assistive technology, pressed in when `on`.
#[must_use]
pub fn icon_button<C: RibbonCommand>(
    app: &RefAny,
    icon: &str,
    name: &str,
    cmd: C,
    on: bool,
) -> RibbonItem {
    RibbonItem::SmallButton(
        button(app, icon, "", cmd)
            .with_toggled(on)
            .with_alt(AzString::from(crate::l10n::t_label(name))),
    )
}

/// Items stacked in a column (small buttons three high).
#[must_use]
pub fn column(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Column(RibbonColumn::create().with_items(items))
}

/// Items side by side in a row (inside a column).
#[must_use]
pub fn row(items: Vec<RibbonItem>) -> RibbonItem {
    RibbonItem::Row(RibbonRow::create().with_items(items))
}

/// A labelled group of items.
#[must_use]
pub fn group(label: &str, items: Vec<RibbonItem>) -> RibbonGroup {
    RibbonGroup::create(crate::l10n::label(label)).with_items(items)
}
