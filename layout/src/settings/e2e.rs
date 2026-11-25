//! The settings of the end-to-end machinery and the debug server that come from the environment,
//! and the one place of this crate that names their variables.
//!
//! Nothing here reads the process: [`E2eSettings::from_lookup`] takes a `&dyn EnvSource`
//! (`azul_css::env`), the one process reader is `azul_dll::env::ProcessEnv`, and `Default` is the
//! behaviour of an EMPTY environment (nothing recorded, no port, no trace, no stand-in device, no
//! URL an e2e run may fetch for real).
//!
//! Two kinds of consumers:
//!
//! * the process-level services that have no object to travel with (the debug server's log
//!   recorder, the request mock store behind the context-free `FileDialog` / `HttpRequest`
//!   functions) get their values once, from [`E2eSettings::install`], called by the app start;
//! * what a scenario op or the in-process runner asks (baseline recording, the divergent-pixel
//!   dump, the screenshot folder, the traces) is read from the window's copy of the settings
//!   (`LayoutWindow::settings`).

use std::path::PathBuf;

use azul_css::env::EnvSource;

use super::platform::os_var;

/// What the environment asks of the e2e machinery and the debug server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
// A bag of independent switches, each its own variable: a state machine of enums would only
// rename them.
#[allow(clippy::struct_excessive_bools)]
pub struct E2eSettings {
    /// `AZ_RECORD=<file>`: the file the debug server records its log into (it also turns the debug
    /// log on).
    pub record: Option<PathBuf>,
    /// `AZ_DEBUG=<port>`: the port of the debug server; `None` when it is unset or no port.
    pub debug_port: Option<u16>,
    /// `AZ_E2E_RECORD=1` (or `true`): `assert_screenshot` writes the baseline it misses instead of
    /// failing. Off by default, on purpose: a missing reference is RED.
    pub record_baselines: bool,
    /// `AZ_E2E_DUMP` is set: a failed pixel-identity check prints the channel values around the
    /// first divergent pixel.
    pub dump_divergent_pixels: bool,
    /// `AZ_E2E_SHOT_DIR=<dir>`: every screenshot of a scenario is also written there, numbered in
    /// capture order.
    pub shot_dir: Option<PathBuf>,
    /// `AZ_ACT_DEBUG` is set: the in-process runner traces the default keyboard actions.
    pub act_debug: bool,
    /// `AZ_ANIM_DEBUG` is set: the in-process runner names each scenario it starts.
    pub anim_debug: bool,
    /// `AZ_SYNTHETIC_DEVICES=<list>`: the devices (`microphone`, `camera`, `screen`, `audio_sink`,
    /// `all`) an e2e run opens as synthetic stand-ins, as the text the run was started with.
    pub synthetic_devices: Option<String>,
    /// `AZ_E2E_ALLOW_HTTP=<patterns>`: the URL patterns a scripted run fetches for real, as the text
    /// the run was started with (comma-separated, `http://127.0.0.1:*`).
    pub allow_http: Option<String>,
}

impl E2eSettings {
    /// What the environment `env` asks for; an empty environment asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            record: env.var("AZ_RECORD").map(PathBuf::from),
            debug_port: env.var("AZ_DEBUG").and_then(|port| port.parse().ok()),
            record_baselines: env
                .var("AZ_E2E_RECORD")
                .is_some_and(|v| v == "1" || v.eq_ignore_ascii_case("true")),
            dump_divergent_pixels: env.is_set("AZ_E2E_DUMP"),
            shot_dir: os_var(env, "AZ_E2E_SHOT_DIR").map(PathBuf::from),
            act_debug: env.is_set("AZ_ACT_DEBUG"),
            anim_debug: env.is_set("AZ_ANIM_DEBUG"),
            synthetic_devices: env.var("AZ_SYNTHETIC_DEVICES"),
            allow_http: env.var("AZ_E2E_ALLOW_HTTP"),
        }
    }

    /// Puts what the context-free code of the e2e machinery needs where it finds it: the request
    /// mock store learns the stand-in devices and the URLs an e2e run may fetch
    /// (`request::mock::configure`, with the `text_layout` feature), and the debug server starts
    /// recording its log into `record` (`e2e::init_recording`, with the `e2e-server` feature).
    ///
    /// Call it once, at the start, before the first log record.
    pub fn install(&self) {
        #[cfg(feature = "text_layout")]
        crate::request::mock::configure(
            self.synthetic_devices.as_deref(),
            self.allow_http.as_deref(),
        );
        #[cfg(feature = "e2e-server")]
        crate::e2e::init_recording(self.record.as_deref());
    }
}

#[cfg(test)]
#[path = "e2e_test.rs"]
mod tests;
