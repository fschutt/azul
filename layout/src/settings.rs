//! The settings of azul-layout: the switches the process environment used to flip from inside
//! the engine, as plain values that the owner of a layout window, a font manager or a cache
//! hands down.
//!
//! The library crates never read `std::env`. Each group of settings is a struct with a pure
//! `from_lookup(&dyn EnvSource)` that names its variables in ONE place (a file of this directory),
//! and a `Default` that is the behaviour of an EMPTY environment (`from_lookup(&NoEnv)`), so a
//! program that never configured anything, or a test, gets the shipped behaviour. The one reader
//! of the process is the dll (`azul_dll::env::ProcessEnv`); it builds a [`LayoutSettings`] once
//! at the start and installs it (`LayoutWindow::set_settings`).
//!
//! One file per group, so two clusters of the env pass never edit the same lines:
//!
//! - [`solver`]: the layout solver (`solver3`): the display-list options, the storage form of
//!   text, the switches back to the old behaviour and the traces of the passes.
//! - [`text`]: text shaping, fonts and glyph rendering (`text3`, `glyph_cache`).
//! - [`window`]: the layout window (`window`, the managers, `scroll_timer`, `headless`): the
//!   alternative paths of a rebuild, the traces and the verifications.
//! - [`render`]: the CPU renderer (`cpu_backend`, `cpurender`): the paint switches, the thread
//!   budget, the kept surfaces, the text halos and the dumps.
//! - [`widgets`]: the widgets: the alternative builds and the traces.
//!
//! A group is added by a `pub mod` line, a `pub` field of [`LayoutSettings`] and a line in
//! [`LayoutSettings::from_lookup`], each in the slot of its cluster below.
#![allow(clippy::module_name_repetitions)]

use azul_css::env::EnvSource;

pub mod solver;
pub mod text;

// Cluster W2 (window, render, widgets): its `pub mod` lines go here.
pub mod render;
pub mod widgets;
pub mod window;

// Cluster W3 (e2e, telemetry, services): its `pub mod` lines go here.
pub mod e2e;
pub mod platform;
pub mod services;
pub mod telemetry;

pub use e2e::E2eSettings;
pub use platform::PlatformEnv;
pub use render::{PaintSwitches, RenderSettings};
pub use services::ServiceSettings;
pub use solver::{DenseTextMode, MovedRunPolicy, SolverSettings};
pub use telemetry::TelemetrySettings;
pub use text::TextSettings;
pub use widgets::WidgetSettings;
pub use window::WindowSettings;

/// Every group of settings of azul-layout.
///
/// A plain struct with one `pub` field per group: a host that wants the process environment
/// builds it with `LayoutSettings::from_lookup(&env)` once; one that wants one switch writes a
/// field (`settings.solver.patching = false`). `Default` is the behaviour of an empty
/// environment.
// Not `Copy`: the groups of the other clusters of the env pass hold paths and strings, and a
// `Copy` that is taken away again would break whoever copies the aggregate.
#[allow(missing_copy_implementations)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayoutSettings {
    /// The layout solver ([`SolverSettings`]).
    pub solver: SolverSettings,
    /// Text shaping, fonts and glyph rendering ([`TextSettings`]).
    pub text: TextSettings,

    // Cluster W2 (window, render, widgets): its fields go here.
    /// The layout window: the alternative paths of a rebuild, the traces and the verifications
    /// ([`WindowSettings`]).
    pub window: WindowSettings,
    /// The CPU renderer: the paint switches, the thread budget, the kept surfaces, the text
    /// halos and the dumps ([`RenderSettings`]).
    pub render: RenderSettings,
    /// The widgets: the alternative builds and the traces ([`WidgetSettings`]).
    pub widgets: WidgetSettings,

    // Cluster W3 (e2e, telemetry, services): its fields go here.
    /// The end-to-end machinery and the debug server ([`E2eSettings`]).
    pub e2e: E2eSettings,
    /// The telemetry client ([`TelemetrySettings`]).
    pub telemetry: TelemetrySettings,
    /// The services: the file picker, the profiler's symbol lookup, the platform folders
    /// ([`ServiceSettings`]).
    pub services: ServiceSettings,
}

impl LayoutSettings {
    /// The settings `env` asks for: each group reads its own variables.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            solver: SolverSettings::from_lookup(env),
            text: TextSettings::from_lookup(env),

            // Cluster W2 (window, render, widgets): its groups go here.
            window: WindowSettings::from_lookup(env),
            render: RenderSettings::from_lookup(env),
            widgets: WidgetSettings::from_lookup(env),

            // Cluster W3 (e2e, telemetry, services): its groups go here.
            e2e: E2eSettings::from_lookup(env),
            telemetry: TelemetrySettings::from_lookup(env),
            services: ServiceSettings::from_lookup(env),
        }
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
