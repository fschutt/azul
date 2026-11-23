//! The settings of azul-core that come from the environment, built from an `EnvSource`.
//!
//! This is the one place of the crate that names their variables (except the test hooks, which
//! [`crate::run_mode`] names itself).
//!
//! Nothing here reads the process: every constructor takes a `&dyn EnvSource`
//! (`azul_css::env`), and `Default` is the behaviour of an EMPTY environment, switches that are on
//! by default included. The dll builds [`CoreSettings`] once at the start from its process reader
//! and installs what the library asks for from places that carry no context
//! ([`CoreSettings::install`]: the log filter, the profiling gate, the diagnostics, the test
//! hooks); the text anti-aliasing travels in `AppConfig::text_antialiasing`, the overlay in the
//! window's `DebugState`.

use alloc::{string::String, vec::Vec};

use azul_css::env::EnvSource;

use crate::{
    log_filter::{Level, LogSettings},
    profile::{ProfileFlags, ProfileSettings},
    resources::{TextAntialiasMode, TextAntialiasing},
    window::DebugState,
};

/// The variables that ask for logs: setting one of them is a request to see more than errors.
///
/// `AZ_LOG` itself, a file or stderr to write to, a debug server, a scripted run, a recording.
pub static LOG_REQUEST_VARS: [&str; 6] = [
    "AZ_LOG",
    "AZ_LOG_FILE",
    "AZ_LOG_STDERR",
    "AZ_DEBUG",
    "AZ_E2E",
    "AZ_RECORD",
];

/// The one-shot diagnostics the environment silenced: the tags of `AZ_SUPPRESS` (and of its
/// misspelt alias `AZ_SUPRESS`), a comma-separated list, matched without case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SuppressSet {
    tags: Vec<String>,
}

impl SuppressSet {
    /// Nothing silenced.
    #[must_use]
    pub const fn new() -> Self {
        Self { tags: Vec::new() }
    }

    /// The set a comma-separated list names; blanks are dropped.
    #[must_use]
    pub fn from_list(list: &str) -> Self {
        Self {
            tags: list
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(str::to_ascii_lowercase)
                .collect(),
        }
    }

    /// Whether the tag is in the list.
    #[must_use]
    pub fn contains(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }

    /// Whether the tag, or `all`, is in the list.
    #[must_use]
    pub fn contains_or_all(&self, tag: &str) -> bool {
        self.contains(tag) || self.contains("all")
    }
}

/// The diagnostic dials: which one-shot findings are silenced, the cascade trace, and the widget
/// build that goes through `DomBuilder`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagSettings {
    /// `AZ_SUPPRESS` (`AZ_SUPRESS`): the findings that are not said.
    pub suppress: SuppressSet,
    /// `AZ_CASCADE_TRACE` is set: trace the cascade passes.
    pub cascade_trace: bool,
    /// `AZ_FLAT_WIDGETS` is set, not empty and not `0`: a widget that has both builds makes its DOM
    /// through `DomBuilder`. Off (the tree of `Dom`s) is the default.
    pub flat_widgets: bool,
}

impl DiagSettings {
    /// The empty environment's: nothing silenced, no trace, the tree build.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            suppress: SuppressSet::new(),
            cascade_trace: false,
            flat_widgets: false,
        }
    }

    /// What the environment `env` asks for.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let list = env
            .var("AZ_SUPPRESS")
            .or_else(|| env.var("AZ_SUPRESS"))
            .unwrap_or_default();
        Self {
            suppress: SuppressSet::from_list(&list),
            cascade_trace: env.var("AZ_CASCADE_TRACE").is_some(),
            flat_widgets: env
                .var("AZ_FLAT_WIDGETS")
                .is_some_and(|value| !value.is_empty() && value != "0"),
        }
    }
}

impl LogSettings {
    /// Whether the environment asks for logs: one of [`LOG_REQUEST_VARS`] is set (to anything, an
    /// empty or a non-UTF-8 value included).
    #[must_use]
    pub fn asked(env: &dyn EnvSource) -> bool {
        LOG_REQUEST_VARS.iter().any(|name| env.is_set(name))
    }

    /// What the environment `env` asks of the log filter.
    ///
    /// `AZ_LOG` and `AZ_LOG_STDERR`, and which of [`LOG_REQUEST_VARS`] are set at all. An empty
    /// environment asks for errors only ([`crate::log_filter::LEVEL_WHEN_NOT_ASKED`]) and no echo.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let raw = env.var("AZ_LOG").unwrap_or_default();
        // Setting AZ_LOG AT ALL is a request to see logs, so it turns the echo on. Leaving it unset
        // is not, which is what keeps an ordinary app run quiet. (`log_gate` additionally treats an
        // active debug server as such a request; it can see `log_active()` and this crate cannot.)
        let mut stderr_echo = (!raw.trim().is_empty()).then_some(true);
        let (parsed, overrides) = crate::log_filter::parse(&raw);
        let level = crate::log_filter::level_for(Self::asked(env), parsed);
        // Explicit override only. The default is ON (see `LogFilter::stderr_echo`) - this exists so a
        // build that genuinely wants the queue and nothing else can say so, not as the switch that
        // makes logging work.
        if let Some(value) = env.var("AZ_LOG_STDERR") {
            let value = value.trim().to_ascii_lowercase();
            let off = matches!(value.as_str(), "0" | "off" | "false" | "no" | "none");
            stderr_echo = Some(!off);
        }
        // AZ_LOG=off means off, including the echo.
        if level.is_none() {
            stderr_echo = Some(false);
        }
        Self {
            level,
            overrides,
            stderr_echo,
        }
    }
}

impl Default for LogSettings {
    /// The empty environment's: errors only, every category on, the echo left as it is.
    fn default() -> Self {
        Self {
            level: Some(Level::Error),
            overrides: Vec::new(),
            stderr_echo: None,
        }
    }
}

impl ProfileSettings {
    /// What the environment `env` asks for.
    ///
    /// `AZ_PROFILE` (the tokens, see [`crate::profile`]) and `AZ_PROFILE_OUT`. What deserves a
    /// word on stderr (an unknown token, a mode that would emit nothing) is in
    /// [`ProfileSettings::warnings`], for the installer to say once.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let raw = env.var("AZ_PROFILE");
        let flags = raw.as_deref().map(ProfileFlags::parse).unwrap_or_default();
        let out_path = env.var("AZ_PROFILE_OUT");
        let mut warnings = Vec::new();
        // A typo'd token must WARN and keep running, never silently parse to
        // nothing - AZ_PROFILE=phases looked exactly like AZ_PROFILE unset,
        // and "a zero is not a measurement".
        if let Some(raw) = &raw {
            let known = |t: &str| {
                matches!(
                    t.to_ascii_lowercase().as_str(),
                    "memory"
                        | "mem"
                        | "cpu"
                        | "perf"
                        | "cascade"
                        | "css"
                        | "heap"
                        | "jsonl"
                        | "detail"
                        | ""
                )
            };
            let mut unknown: Vec<&str> = raw
                .split(',')
                .map(str::trim)
                .filter(|t| !known(t))
                .collect();
            unknown.truncate(8); // a garbage value must not flood stderr
            if !unknown.is_empty() {
                warnings.push(alloc::format!(
                    "[azul][profile] AZ_PROFILE={raw:?}: unknown token(s) {unknown:?} ignored - \
                     valid values are cpu (perf), memory (mem), cascade (css), heap, jsonl, \
                     detail; combine with commas, e.g. AZ_PROFILE=cpu,memory"
                ));
            }
        }
        // The announce table: a profile mode that silently emits NOTHING
        // reads as "not looking" and has repeatedly burned real debugging
        // time ("a zero is not a measurement"). One line, once, at the
        // single point every mode resolves through.
        if flags.heap && !flags.jsonl {
            warnings.push(String::from(
                "[azul][profile] AZ_PROFILE=heap alone emits nothing: use \
                 AZ_PROFILE=heap,jsonl with AZ_PROFILE_OUT=<file> for the \
                 per-phase heap table (and note builds without the `probe` \
                 feature report heap as 0).",
            ));
        }
        if flags.heap && flags.jsonl && out_path.is_none() {
            warnings.push(String::from(
                "[azul][profile] AZ_PROFILE=heap,jsonl is set but \
                 AZ_PROFILE_OUT is not - no destination, nothing will be \
                 written.",
            ));
        }
        Self {
            flags,
            out_path,
            warnings,
        }
    }
}

impl TextAntialiasing {
    /// The settings the environment `env` asks for, or the default where it says nothing.
    ///
    /// * `AZ_TEXT_AA` = `lcd` | `legacy` | `grayscale` | `none`
    /// * `AZ_LCD_BLEND=legacy`: the older spelling of `AZ_TEXT_AA=legacy`, honoured where
    ///   `AZ_TEXT_AA` is unset or `lcd`
    /// * `AZ_LCD_CONTRAST`, `AZ_LCD_CHROMA_LIMIT`, `AZ_LCD_COVERAGE_GAMMA`: 0..=255
    /// * `AZ_LCD_PRETILE=0` or `AZ_NO_LCD_PRETILE`: no pre-blended tiles
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        let default = Self::default();
        let asked = env.var("AZ_TEXT_AA");
        let mut mode = match asked.as_deref() {
            Some(s) if s.eq_ignore_ascii_case("none") || s == "0" => TextAntialiasMode::Aliased,
            Some(s)
                if s.eq_ignore_ascii_case("grayscale") || s.eq_ignore_ascii_case("greyscale") =>
            {
                TextAntialiasMode::Grayscale
            }
            Some(s) if s.eq_ignore_ascii_case("legacy") => TextAntialiasMode::LcdLegacy,
            Some(s) if s.eq_ignore_ascii_case("lcd") => TextAntialiasMode::Lcd,
            _ => TextAntialiasMode::PlatformDefault,
        };
        if matches!(
            mode,
            TextAntialiasMode::PlatformDefault | TextAntialiasMode::Lcd
        ) && env
            .var("AZ_LCD_BLEND")
            .is_some_and(|v| v.eq_ignore_ascii_case("legacy"))
        {
            mode = TextAntialiasMode::LcdLegacy;
        }
        let level = |name: &str, default: u8| {
            env.var(name)
                .and_then(|v| v.parse::<u8>().ok())
                .unwrap_or(default)
        };
        Self {
            mode,
            lcd_contrast: level("AZ_LCD_CONTRAST", default.lcd_contrast),
            lcd_chroma_limit: level("AZ_LCD_CHROMA_LIMIT", default.lcd_chroma_limit),
            lcd_coverage_gamma: level("AZ_LCD_COVERAGE_GAMMA", default.lcd_coverage_gamma),
            lcd_pretile: env.var("AZ_LCD_PRETILE").is_none_or(|v| v.trim() != "0")
                && env.var("AZ_NO_LCD_PRETILE").is_none(),
        }
    }
}

impl DebugState {
    /// The overlays `AZ_OVERLAY` names in `env` ([`DebugState::from_overlay_spec`] parses the
    /// verbs: `hit-test`, `overdraw`, `profiler`, ...); none when it is unset.
    ///
    /// Available in RELEASE builds too, deliberately: the moment you need to see hit-test regions
    /// or overdraw is usually on the build a user is running.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        env.var("AZ_OVERLAY")
            .map_or_else(Self::default, |v| Self::from_overlay_spec(v.as_str()))
    }
}

/// Everything azul-core asks of the environment.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoreSettings {
    /// The log filter.
    pub log: LogSettings,
    /// The profiling gate.
    pub profile: ProfileSettings,
    /// The diagnostic dials.
    pub diag: DiagSettings,
    /// The default of `AppConfig::text_antialiasing`.
    pub text_antialiasing: TextAntialiasing,
    /// The test stand-ins. [`crate::run_mode::TestHooks::NONE`] in a build without the cargo
    /// feature `test-hooks`: those variables are not even looked at.
    #[cfg(feature = "std")]
    pub test_hooks: crate::run_mode::TestHooks,
}

impl CoreSettings {
    /// What the environment `env` asks for. An empty environment asks for nothing.
    #[must_use]
    pub fn from_lookup(env: &dyn EnvSource) -> Self {
        Self {
            log: LogSettings::from_lookup(env),
            profile: ProfileSettings::from_lookup(env),
            diag: DiagSettings::from_lookup(env),
            text_antialiasing: TextAntialiasing::from_lookup(env),
            // A build without the cargo feature `test-hooks` does not even look at the switches.
            #[cfg(feature = "std")]
            test_hooks: if crate::run_mode::AVAILABLE {
                crate::run_mode::TestHooks::from_lookup(env)
            } else {
                crate::run_mode::TestHooks::NONE
            },
        }
    }

    /// Puts the settings where azul-core's context-free code finds them.
    ///
    /// That is the log filter
    /// ([`crate::log_filter::install`]), the profiling gate ([`crate::profile::install`]), the
    /// diagnostics ([`crate::diagnostics::install_settings`]) and the test hooks
    /// ([`crate::run_mode::install`]). Call it once, at the start, before the first log record;
    /// what an earlier install put there stays (the diagnostics, the profile gate and the hooks
    /// are install-once). The text anti-aliasing is not installed: it is the default of
    /// `AppConfig::text_antialiasing` and travels with the config.
    pub fn install(&self) {
        crate::log_filter::install(&self.log);
        crate::profile::install(self.profile.clone());
        crate::diagnostics::install_settings(self.diag.clone());
        #[cfg(feature = "std")]
        crate::run_mode::install(self.test_hooks.clone());
    }
}

#[cfg(test)]
#[path = "settings_test.rs"]
mod tests;
