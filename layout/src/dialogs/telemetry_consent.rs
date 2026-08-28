//! The telemetry CONSENT dialog (`SysDialogType::TelemetryConsent`).
//!
//! Transparency is the whole point: the dialog lists EVERY instrument the
//! app can record — engine metrics with a plain-language sentence each,
//! plus any app-defined ones — and the user checks or unchecks them
//! individually. Above the list sit the four signal switches (crash
//! reports / logs / metrics / app state on crash), and at the bottom a
//! "remember for all azul apps" checkbox that writes the machine-wide
//! shared config (`{config_dir}/azul/config.json`) channel default instead
//! of this app's override.
//!
//! Saving applies everything IMMEDIATELY at runtime (tier atomic, signal
//! gates, disabled-metric set) and persists via `telemetry::sharedconfig`.

use alloc::string::String;
use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType};
use azul_core::dom::Dom;
use azul_core::refany::RefAny;

use crate::callbacks::CallbackInfo;

use super::{cpu_dialog_window, style};
use crate::telemetry::{
    config::{self, TelemetryTier},
    metrics,
    sharedconfig::{self, SharedConfig, SignalSet},
};
use crate::widgets::button::{Button, ButtonOnClickCallbackType};
use crate::widgets::check_box::{CheckBox, CheckBoxOnToggleCallbackType, CheckBoxState};
use azul_css::AzString;

/// One inventory row's dialog state.
#[derive(Debug, Clone)]
struct MetricRow {
    name: String,
    description: String,
    enabled: bool,
}

/// The dialog's state.
// The bools mirror the four consent checkboxes 1:1.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone)]
struct ConsentState {
    crashes: bool,
    logs: bool,
    metrics: bool,
    appdata: bool,
    rows: Vec<MetricRow>,
    remember_for_all: bool,
}

/// Opens the consent dialog, pre-filled from the CURRENT effective config.
pub fn open(info: &mut CallbackInfo) {
    let tier = config::tier();
    let disabled = metrics::disabled_metrics();
    let rows = metrics::instrument_inventory()
        .into_iter()
        .map(|i| MetricRow {
            enabled: !disabled.contains(&i.name),
            name: i.name,
            description: i.description,
        })
        .collect();
    let state = RefAny::new(ConsentState {
        crashes: tier >= TelemetryTier::Crashes,
        logs: tier >= TelemetryTier::Metrics && config::logs_enabled(),
        metrics: tier >= TelemetryTier::Metrics && config::metrics_enabled(),
        appdata: tier >= TelemetryTier::Full,
        rows,
        remember_for_all: false,
    });
    info.create_window(cpu_dialog_window(
        "Data Collection",
        (560.0, 620.0),
        dialog_layout as LayoutCallbackType,
        state,
    ));
}

// --- toggles ---------------------------------------------------------------

macro_rules! signal_toggle {
    ($fn_name:ident, $field:ident) => {
        extern "C" fn $fn_name(
            mut state: RefAny,
            _info: CallbackInfo,
            cb: CheckBoxState,
        ) -> Update {
            if let Some(mut s) = state.downcast_mut::<ConsentState>() {
                s.$field = cb.checked;
            }
            Update::RefreshDom
        }
    };
}
signal_toggle!(on_toggle_crashes, crashes);
signal_toggle!(on_toggle_logs, logs);
signal_toggle!(on_toggle_metrics, metrics);
signal_toggle!(on_toggle_appdata, appdata);
signal_toggle!(on_toggle_remember, remember_for_all);

/// One per-metric checkmark. The row index rides in its own `RefAny` so a
/// single callback serves the whole list.
#[derive(Debug, Clone)]
struct RowToggle {
    dialog: RefAny,
    index: usize,
}

extern "C" fn on_toggle_row(mut state: RefAny, _info: CallbackInfo, cb: CheckBoxState) -> Update {
    let Some(row) = state.downcast_ref::<RowToggle>() else {
        return Update::DoNothing;
    };
    let mut dialog = row.dialog.clone();
    let index = row.index;
    drop(row);
    if let Some(mut s) = dialog.downcast_mut::<ConsentState>() {
        if let Some(r) = s.rows.get_mut(index) {
            r.enabled = cb.checked;
        }
    }
    Update::DoNothing
}

// --- buttons ---------------------------------------------------------------

extern "C" fn on_cancel(mut _state: RefAny, mut info: CallbackInfo) -> Update {
    info.close_window();
    Update::DoNothing
}

extern "C" fn on_save(mut state: RefAny, mut info: CallbackInfo) -> Update {
    let Some(s) = state.downcast_ref::<ConsentState>() else {
        return Update::DoNothing;
    };
    let signals = SignalSet {
        crashes: s.crashes,
        logs: s.logs,
        metrics: s.metrics,
        appdata: s.appdata,
    };
    let disabled: Vec<String> = s
        .rows
        .iter()
        .filter(|r| !r.enabled)
        .map(|r| r.name.clone())
        .collect();
    let remember_for_all = s.remember_for_all;
    drop(s);

    // Apply at RUNTIME immediately: the choice takes effect in this process
    // whether or not the persist below succeeds.
    let _ = config::set_tier(signals.tier());
    config::set_signal_gates(signals);
    metrics::set_disabled_metrics(disabled.iter().cloned());

    // Persist to the machine-wide shared config: the channel DEFAULT when
    // "remember for all azul apps" is checked, this app's override
    // otherwise.
    let channel = metrics::labels().channel;
    let app_key = sharedconfig::app_key();
    let mut shared = SharedConfig::load();
    let scope = if remember_for_all {
        None
    } else {
        app_key.as_deref()
    };
    shared.set_telemetry(&channel, scope, signals, Some(&disabled));
    if let Err(e) = shared.save() {
        crate::telemetry::log(
            crate::telemetry::Severity::Warn,
            alloc::format!("consent dialog: could not persist shared config: {e}"),
        );
    }

    info.close_window();
    Update::DoNothing
}

// --- layout ----------------------------------------------------------------

extern "C" fn dialog_layout(_data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let Some(mut ctx) = info.get_ctx().into_option() else {
        return Dom::create_body();
    };
    let snapshot = match ctx.downcast_ref::<ConsentState>() {
        Some(s) => s.clone(),
        None => return Dom::create_body(),
    };
    drop(ctx);
    let state = info
        .get_ctx()
        .into_option()
        .unwrap_or_else(|| RefAny::new(()));

    use azul_css::props::{
        basic::pixel::PixelValue,
        layout::{LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop},
    };
    let pad = style(vec![
        azul_css::props::property::CssProperty::padding_left(LayoutPaddingLeft {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_right(LayoutPaddingRight {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_top(LayoutPaddingTop {
            inner: PixelValue::px(16.0),
        }),
        azul_css::props::property::CssProperty::padding_bottom(LayoutPaddingBottom {
            inner: PixelValue::px(16.0),
        }),
    ]);

    let mut children: Vec<Dom> = vec![Dom::create_h2_with_text("Data collection")];
    children.push(Dom::create_p_with_text(
        "Choose what this app may send to its developer. Nothing is sent \
         without your consent; the rows below are the COMPLETE list.",
    ));

    children.push(check_row(
        snapshot.crashes,
        "Crash reports (what went wrong, never your documents)",
        on_toggle_crashes,
        state.clone(),
    ));
    children.push(check_row(
        snapshot.logs,
        "Diagnostic logs",
        on_toggle_logs,
        state.clone(),
    ));
    children.push(check_row(
        snapshot.metrics,
        "Anonymous usage metrics (the checkmarks below)",
        on_toggle_metrics,
        state.clone(),
    ));
    children.push(check_row(
        snapshot.appdata,
        "App state on crash (helps reproduce, may include document data)",
        on_toggle_appdata,
        state.clone(),
    ));

    children.push(Dom::create_p_with_text(
        "Collected metrics - uncheck any you do not want recorded:",
    ));
    let mut list_children: Vec<Dom> = Vec::new();
    for (index, row) in snapshot.rows.iter().enumerate() {
        let toggle = RefAny::new(RowToggle {
            dialog: state.clone(),
            index,
        });
        let label = alloc::format!("{} - {}", row.name, row.description);
        list_children.push(check_row(row.enabled, &label, on_toggle_row, toggle));
    }
    children.push(Dom::create_div().with_children(list_children.into()));

    children.push(check_row(
        snapshot.remember_for_all,
        "Remember this setting for all azul apps on this machine",
        on_toggle_remember,
        state.clone(),
    ));

    children.push(button_row(vec![
        ("Save", on_save as ButtonOnClickCallbackType, state.clone()),
        ("Cancel", on_cancel as ButtonOnClickCallbackType, state),
    ]));

    Dom::create_body()
        .with_css_props(pad)
        .with_children(children.into())
}

/// Checkbox + label on one line (same shape as the report dialog's rows).
fn check_row(checked: bool, label: &str, cb: CheckBoxOnToggleCallbackType, state: RefAny) -> Dom {
    Dom::create_div().with_children(
        vec![
            CheckBox::create(checked).with_on_toggle(state, cb).dom(),
            Dom::create_p_with_text(label),
        ]
        .into(),
    )
}

/// Buttons on one line (same shape as the other dialogs' rows).
fn button_row(buttons: Vec<(&str, ButtonOnClickCallbackType, RefAny)>) -> Dom {
    let children: Vec<Dom> = buttons
        .into_iter()
        .map(|(label, cb, state)| {
            Button::create(AzString::from(label))
                .with_on_click(state, cb)
                .dom()
        })
        .collect();
    Dom::create_div().with_children(children.into())
}
