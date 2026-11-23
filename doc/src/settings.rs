//! The environment of azul-doc: the ONE place that names the variables the tool uses.
//!
//! The tool reads the process environment ONCE, at the start of `main`
//! ([`crate::process_env::read_settings`]), into a [`ToolSettings`] that the commands take as an
//! argument or carry in their configuration. Nothing else in the crate calls `std::env::var`; the
//! gate `examples/azul-srcgate/tests/env_gate.rs` counts the reads per file.
//!
//! [`ToolSettings::from_lookup`] is pure: it asks an [`EnvSource`] for the names below, so a test
//! hands it a `MapEnv` and `ToolSettings::default()` is what an EMPTY environment gives. The names
//! the tool only SETS on a child process (`CLAUDECODE` taken out of an agent's environment,
//! `AZ_WORKSPACE_ROOT` handed to a build script) are constants of this file too.
//!
//! The variables the ENGINE reads (the text anti-aliasing switches of `azul-core`) are not named
//! here: `from_lookup` hands the lookup to the engine's own constructor and keeps the typed value.

use std::{
    ffi::{OsStr, OsString},
    fmt,
    path::PathBuf,
};

use azul_core::resources::TextAntialiasing;
use azul_css::env::{EnvSource, NoEnv};

// ---------------------------------------------------------------------------------------------
// Names the tool sets on a child process (it never reads these)
// ---------------------------------------------------------------------------------------------

/// The azul checkout a build script works in.
pub const WORKSPACE_ROOT: &str = "AZ_WORKSPACE_ROOT";
/// `scripts/build-android.sh`: build the APK, do not install it.
pub const ANDROID_NO_DEPLOY: &str = "AZ_ANDROID_NO_DEPLOY";
/// `scripts/build-android.sh`: more cargo features for the app (the tool also asks whether the
/// caller set it, see [`ToolSettings::android_extra_features`]).
pub const ANDROID_EXTRA_FEATURES: &str = "AZ_ANDROID_EXTRA_FEATURES";
/// `scripts/build-ios.sh`: print the plan, run nothing.
pub const IOS_DRYRUN: &str = "AZ_IOS_DRYRUN";
/// The E2E script the app on a device runs.
pub const E2E: &str = "AZ_E2E";
/// The azul-doc binary an apply agent calls back into.
pub const DOC_BIN: &str = "AZ_DOC_BIN";
/// Set by a Claude Code session; taken out of the environment of an agent the tool starts.
pub const CLAUDECODE: &str = "CLAUDECODE";
/// The paid API route of the `claude` CLI; the tool refuses to run an agent while it is set.
pub const ANTHROPIC_API_KEY: &str = "ANTHROPIC_API_KEY";
/// The executable search path of a child process.
pub const PATH: &str = "PATH";
/// The NDK of an Android build.
pub const ANDROID_NDK_HOME: &str = "ANDROID_NDK_HOME";
/// The JDK of an Android build.
pub const JAVA_HOME: &str = "JAVA_HOME";
/// The hermetic fontconfig file the reftest pins for Chrome and for the engine's font stack
/// (fontconfig and rust-fontconfig read it: the tool sets it, see `reftest::pipeline`).
pub const FONTCONFIG_FILE: &str = "FONTCONFIG_FILE";

// ---------------------------------------------------------------------------------------------
// Names the tool reads
// ---------------------------------------------------------------------------------------------

const AZUL_ROOT: &str = "AZUL_ROOT";
const CARGO_TARGET_DIR: &str = "CARGO_TARGET_DIR";
const HOME: &str = "HOME";
const TMPDIR: &str = "TMPDIR";
const AZUL_DOC_CACHE: &str = "AZUL_DOC_CACHE";
const XDG_CACHE_HOME: &str = "XDG_CACHE_HOME";
const ANDROID_AVD_HOME: &str = "ANDROID_AVD_HOME";
const ANDROID_HOME: &str = "ANDROID_HOME";
const ANDROID_SDK_ROOT: &str = "ANDROID_SDK_ROOT";
const CHROME: &str = "CHROME";
const PROGRAMFILES: &str = "PROGRAMFILES";
const PROGRAMFILES_X86: &str = "PROGRAMFILES(X86)";
const DOC_TG_TOKEN: &str = "AZ_DOC_TG_TOKEN";
const TELEGRAM_BOT_TOKEN: &str = "TELEGRAM_BOT_TOKEN";
const DOC_TG_CHAT_ID: &str = "AZ_DOC_TG_CHAT_ID";
const TELEGRAM_CHAT_ID: &str = "TELEGRAM_CHAT_ID";

/// What the tool takes from the environment, read once.
///
/// A path is kept as the OS gave it (`var_os`), a flag that the old code asked with
/// `var(..).is_ok()` is a `bool` that is true for a UTF-8 value only, and a flag it asked with
/// `var_os(..).is_some()` is true for any value. A value that is a secret is never printed: see
/// the `Debug` impl.
#[derive(Clone, PartialEq)]
pub struct ToolSettings {
    /// `AZUL_ROOT`: the azul checkout, over the search from the working directory.
    pub azul_root: Option<PathBuf>,
    /// `CARGO_TARGET_DIR` (relative to the workspace, or absolute).
    pub cargo_target_dir: Option<PathBuf>,
    /// `HOME`.
    pub home: Option<OsString>,
    /// `PATH`.
    pub path: Option<OsString>,
    /// The directory scratch files go to: the platform's own answer in the process
    /// ([`crate::process_env::read_settings`]), `TMPDIR` or `/tmp` from a lookup.
    pub temp_dir: PathBuf,
    /// `AZUL_DOC_CACHE`: where the build assets of `mobile` are written.
    pub doc_cache: Option<PathBuf>,
    /// `XDG_CACHE_HOME`.
    pub xdg_cache_home: Option<PathBuf>,
    /// `AZ_ANDROID_EXTRA_FEATURES` is set (to any value): the caller chose the features.
    pub android_extra_features: bool,
    /// `ANDROID_NDK_HOME`.
    pub android_ndk_home: Option<PathBuf>,
    /// `ANDROID_AVD_HOME`.
    pub android_avd_home: Option<PathBuf>,
    /// `ANDROID_HOME`.
    pub android_home: Option<PathBuf>,
    /// `ANDROID_SDK_ROOT`.
    pub android_sdk_root: Option<PathBuf>,
    /// `JAVA_HOME`.
    pub java_home: Option<PathBuf>,
    /// `CHROME`: the browser the reftest compares with (an empty value is as good as none).
    pub chrome: Option<String>,
    /// `PROGRAMFILES` (Windows).
    pub program_files: Option<String>,
    /// `PROGRAMFILES(X86)` (Windows).
    pub program_files_x86: Option<String>,
    /// The engine's text anti-aliasing as its own switches ask for it. The reftest overrides the
    /// mode and keeps the rest (`reftest::pin_comparison_text_blend`).
    pub text_aa: TextAntialiasing,
    /// `CLAUDECODE` is set to a UTF-8 value: the tool runs inside a Claude Code session.
    pub inside_claude_code: bool,
    /// `ANTHROPIC_API_KEY` is set to a UTF-8 value (the key itself is not kept).
    pub anthropic_api_key_set: bool,
    /// `AZ_DOC_TG_TOKEN`, else `TELEGRAM_BOT_TOKEN`: the Telegram bot token (a secret).
    pub telegram_token: Option<String>,
    /// `AZ_DOC_TG_CHAT_ID`, else `TELEGRAM_CHAT_ID`, as written.
    pub telegram_chat_id: Option<String>,
}

impl ToolSettings {
    /// The settings `env` asks for; an unset variable is the default (`from_lookup(&NoEnv)` is
    /// [`ToolSettings::default`]).
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let path = |name: &str| env.var_os(name).map(PathBuf::from);
        Self {
            azul_root: path(AZUL_ROOT),
            cargo_target_dir: path(CARGO_TARGET_DIR),
            home: env.var_os(HOME),
            path: env.var_os(PATH),
            temp_dir: path(TMPDIR).unwrap_or_else(|| PathBuf::from("/tmp")),
            doc_cache: path(AZUL_DOC_CACHE),
            xdg_cache_home: path(XDG_CACHE_HOME),
            android_extra_features: env.is_set(ANDROID_EXTRA_FEATURES),
            android_ndk_home: path(ANDROID_NDK_HOME),
            android_avd_home: path(ANDROID_AVD_HOME),
            android_home: path(ANDROID_HOME),
            android_sdk_root: path(ANDROID_SDK_ROOT),
            java_home: path(JAVA_HOME),
            chrome: env.var(CHROME),
            program_files: env.var(PROGRAMFILES),
            program_files_x86: env.var(PROGRAMFILES_X86),
            text_aa: TextAntialiasing::from_vars(|name| env.var(name)),
            inside_claude_code: env.var(CLAUDECODE).is_some(),
            anthropic_api_key_set: env.var(ANTHROPIC_API_KEY).is_some(),
            telegram_token: env.var(DOC_TG_TOKEN).or_else(|| env.var(TELEGRAM_BOT_TOKEN)),
            telegram_chat_id: env.var(DOC_TG_CHAT_ID).or_else(|| env.var(TELEGRAM_CHAT_ID)),
        }
    }

    /// `HOME` as a path.
    #[must_use]
    pub fn home_dir(&self) -> Option<PathBuf> {
        self.home.as_ref().map(PathBuf::from)
    }

    /// `HOME` as text: `None` when it is unset or not UTF-8, as `std::env::var("HOME").ok()`.
    #[must_use]
    pub fn home_utf8(&self) -> Option<&str> {
        self.home.as_deref().and_then(OsStr::to_str)
    }

    /// `PATH` as text: empty when it is unset or not UTF-8, as
    /// `std::env::var("PATH").unwrap_or_default()`.
    #[must_use]
    pub fn path_utf8(&self) -> &str {
        self.path.as_deref().and_then(OsStr::to_str).unwrap_or("")
    }
}

impl Default for ToolSettings {
    /// The behaviour of an EMPTY environment.
    fn default() -> Self {
        Self::from_lookup(&NoEnv)
    }
}

/// Secrets are never printed: the Telegram token shows as `<set>`.
impl fmt::Debug for ToolSettings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ToolSettings")
            .field("azul_root", &self.azul_root)
            .field("cargo_target_dir", &self.cargo_target_dir)
            .field("home", &self.home)
            .field("path", &self.path)
            .field("temp_dir", &self.temp_dir)
            .field("doc_cache", &self.doc_cache)
            .field("xdg_cache_home", &self.xdg_cache_home)
            .field("android_extra_features", &self.android_extra_features)
            .field("android_ndk_home", &self.android_ndk_home)
            .field("android_avd_home", &self.android_avd_home)
            .field("android_home", &self.android_home)
            .field("android_sdk_root", &self.android_sdk_root)
            .field("java_home", &self.java_home)
            .field("chrome", &self.chrome)
            .field("program_files", &self.program_files)
            .field("program_files_x86", &self.program_files_x86)
            .field("text_aa", &self.text_aa)
            .field("inside_claude_code", &self.inside_claude_code)
            .field("anthropic_api_key_set", &self.anthropic_api_key_set)
            .field(
                "telegram_token",
                &self.telegram_token.as_ref().map(|_| "<set>"),
            )
            .field("telegram_chat_id", &self.telegram_chat_id)
            .finish()
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
