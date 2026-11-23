//! What the environment asks of the shells and the app start, and the one place of the dll that
//! names those variables (the groups D1 of the env pass).
//!
//! Nothing here reads the process: [`ShellSettings::from_lookup`] takes a `&dyn EnvSource`. The
//! app start builds it once from [`crate::env::ProcessEnv`], inside [`super::app_settings::AppSettings`],
//! and every window reads a plain field of it through its `AppShared`
//! (`window.common.app.settings().shell`). [`ShellSettings::default`] is the behaviour of an
//! EMPTY environment, which is what a test, a tool or an embedding host that never asked for
//! anything gets.

use azul_css::env::EnvSource;

/// `AZ_BACKEND`: the windowing or render backend, as typed.
const BACKEND: &str = "AZ_BACKEND";
/// `AZ_WINDOW`: the windowing system (`x11`, `wayland`, `auto`), as typed.
const WINDOW: &str = "AZ_WINDOW";
/// `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER`: exit 0 after the first rendered frame.
const EXIT_AFTER_FIRST_FRAME: &str = "AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER";
/// `AZ_HIT_DEBUG`: trace the hit tests and the focus-out button clears.
const HIT_DEBUG: &str = "AZ_HIT_DEBUG";
/// `AZ_TELEMETRY_CHANNEL`: the release channel label of the telemetry meta.
const TELEMETRY_CHANNEL: &str = "AZ_TELEMETRY_CHANNEL";
/// `AZ_DUMP_SYSTEM_STYLE`: print what the desktop detection found.
const DUMP_SYSTEM_STYLE: &str = "AZ_DUMP_SYSTEM_STYLE";

/// The channel the telemetry reports when the environment names none.
const DEFAULT_TELEMETRY_CHANNEL: &str = "default";

/// The settings of the app start and of the platform shells.
// Each flag is its own switch of a debugging session: a set of enums would only rename them.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellSettings {
    /// `AZ_BACKEND` as typed: the render backend (`cpu`, `gpu`, `auto`, `headless`, `web://..`)
    /// and, for the legacy spelling, the windowing system (`x11`, `wayland`). `None` when it is
    /// unset or not UTF-8.
    pub backend: Option<String>,
    /// `AZ_WINDOW` as typed: the windowing system (`x11`, `wayland`, `auto`). `None` when it is
    /// unset or not UTF-8.
    pub window: Option<String>,
    /// `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER` is set (to a UTF-8 value): the shell exits with 0
    /// after the first rendered frame.
    pub exit_after_first_frame: bool,
    /// `AZ_HIT_DEBUG` is set (to a UTF-8 value): trace every hit test.
    pub hit_debug: bool,
    /// `AZ_TELEMETRY_CHANNEL`: the release channel label of the telemetry meta; `default` when
    /// the variable is unset.
    pub telemetry_channel: String,
    /// `AZ_DUMP_SYSTEM_STYLE=1`: print what the desktop detection found.
    pub dump_system_style: bool,
}

impl ShellSettings {
    /// What the environment `env` asks for; an empty one asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            backend: env.var(BACKEND),
            window: env.var(WINDOW),
            exit_after_first_frame: env.var(EXIT_AFTER_FIRST_FRAME).is_some(),
            hit_debug: env.var(HIT_DEBUG).is_some(),
            telemetry_channel: env
                .var(TELEMETRY_CHANNEL)
                .unwrap_or_else(|| DEFAULT_TELEMETRY_CHANNEL.to_owned()),
            dump_system_style: env.var(DUMP_SYSTEM_STYLE).as_deref() == Some("1"),
        }
    }
}

impl Default for ShellSettings {
    /// The empty environment's.
    fn default() -> Self {
        Self::from_lookup(&azul_css::env::NoEnv)
    }
}

#[cfg(test)]
#[path = "shell_settings_test.rs"]
mod tests;
