//! Built-in SYSTEM DIALOGS (`SysDialogType`), rendered by azul itself.
//!
//! Two rules every dialog here obeys:
//!
//! 1. **Always CPU-rendered.** A dialog that reports a problem — possibly a
//!    GPU problem — must not depend on the GPU working, so every window this
//!    module creates forces `HwAcceleration::Disabled`.
//! 2. **Consent is structural.** Nothing sends, installs or overwrites
//!    without the user pressing the button that says so. Background threads
//!    only CHECK and STAGE.
//!
//! The dialogs read app-level configuration (support mailbox, manifest URL,
//! changelog URL, app name/version) from [`crate::appenv`], which `App::run`
//! fills from `AppConfig`. Entry point: `CallbackInfo::invoke_system_dialog`.

// fn-item -> fn-pointer casts: required for the Into<Callback> generics;
// the annotated-temporary alternative buries the callback wiring.
#![allow(trivial_casts)]
#[cfg(feature = "telemetry")]
pub mod crash_reporter;
pub mod gpu_check;
pub mod markdown;
/// Shared report machinery: screenshot REDACTION + the report bundle both
/// the problem-report dialog and the crash reporter build.
#[cfg(feature = "cpurender")]
pub mod report;
/// The problem-report dialog. Gated with `cpurender` because it is built ON
/// `dialogs::report` (which is) and decodes its screenshot through
/// `crate::cpurender::AzulPixmap` — without the gate a `widgets` build with no
/// CPU renderer referenced two modules that had been configured out.
#[cfg(feature = "cpurender")]
pub mod report_problem;
#[cfg(feature = "telemetry")]
pub mod telemetry_consent;
#[cfg(feature = "updater")]
pub mod update_version;

use azul_core::{
    geom::LogicalSize,
    refany::{OptionRefAny, RefAny},
    window::HwAcceleration,
};
use azul_css::dynamic_selector::{
    CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelectorVec,
};
use azul_css::props::property::CssProperty;

use crate::window_state::WindowCreateOptions;
use azul_core::callbacks::LayoutCallbackType;

/// A dialog window shell: titled, sized, state in `layout_callback.ctx`,
/// and — the invariant of this module — CPU-rendered, whatever the app or
/// the environment asked for.
#[must_use]
pub(crate) fn cpu_dialog_window(
    title: &str,
    size: (f32, f32),
    layout: LayoutCallbackType,
    state: RefAny,
) -> WindowCreateOptions {
    let mut options = WindowCreateOptions::create(layout);
    // The dialog's state rides the layout callback's ctx slot (the same slot
    // FFI hosts use); the layout fn reads it via `info.get_ctx()`. The DATA
    // argument stays the app's RefAny, which these engine dialogs never
    // downcast.
    options.window_state.layout_callback.ctx = OptionRefAny::Some(state);
    options.window_state.title = title.into();
    options.window_state.size.dimensions = LogicalSize::new(size.0, size.1);
    // ALWAYS CPU: never let a diagnostic surface depend on the GPU.
    options.window_state.renderer_options.hw_accel = HwAcceleration::Disabled;
    options
}

/// Unconditional inline properties (no dynamic-selector conditions).
pub(crate) fn style(props: Vec<CssProperty>) -> CssPropertyWithConditionsVec {
    props
        .into_iter()
        .map(|property| CssPropertyWithConditions {
            property,
            apply_if: DynamicSelectorVec::from_const_slice(&[]),
        })
        .collect::<Vec<_>>()
        .into()
}

#[cfg(test)]
mod tests {
    use azul_core::window::HwAcceleration;

    use super::*;
    use azul_core::callbacks::{LayoutCallbackInfo, LayoutCallbackType};

    extern "C" fn dummy_layout(_: RefAny, _: LayoutCallbackInfo) -> azul_core::dom::Dom {
        azul_core::dom::Dom::create_body()
    }

    #[test]
    fn dialog_windows_force_cpu_rendering() {
        // THE module invariant: whatever the environment/app defaults are,
        // a system dialog must come out with hw_accel == Disabled.
        let w = cpu_dialog_window(
            "t",
            (100.0, 100.0),
            dummy_layout as LayoutCallbackType,
            RefAny::new(0u8),
        );
        assert_eq!(
            w.window_state.renderer_options.hw_accel,
            HwAcceleration::Disabled
        );
        assert_eq!(w.window_state.title.as_str(), "t");
        assert!(w.window_state.layout_callback.ctx.is_some());
    }
}
