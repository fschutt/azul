//! The seam between the process environment and the settings of the engine.
//!
//! The library crates (azul-css, azul-core, azul-layout, webrender) never call `std::env`. Each
//! defines typed settings with a pure `from_lookup(&dyn EnvSource)` constructor that names its
//! variables in ONE place; the process reader lives in the dll (`azul_dll::env::ProcessEnv`, the
//! only implementation that touches the process environment) and in a tool's `main`.
//!
//! ```
//! use azul_css::env::{EnvSource, MapEnv, NoEnv};
//!
//! struct Settings {
//!     trace: bool,
//!     threads: usize,
//! }
//!
//! impl Settings {
//!     /// An EMPTY environment is the default: the behaviour of a build nobody configured.
//!     fn from_lookup(env: &dyn EnvSource) -> Self {
//!         Self {
//!             trace: env.var("AZ_TRACE").is_some_and(|v| v == "1"),
//!             threads: env.var("AZ_THREADS").and_then(|v| v.parse().ok()).unwrap_or(4),
//!         }
//!     }
//! }
//!
//! let none = Settings::from_lookup(&NoEnv);
//! assert!(!none.trace && none.threads == 4);
//! let some = Settings::from_lookup(&MapEnv::new().with("AZ_THREADS", "8"));
//! assert_eq!(some.threads, 8);
//! // a closure is a source too
//! let by_closure = Settings::from_lookup(&|name: &str| (name == "AZ_TRACE").then(|| "1".to_string()));
//! assert!(by_closure.trace);
//! ```
//!
//! Rules of the seam:
//!
//! - `Default` of a settings struct is the behaviour of an EMPTY environment (`from_lookup(&NoEnv)`),
//!   including the switches that are on by default and the numbers that have a default.
//! - Values that are paths are read with [`EnvSource::var_os`], which keeps a non-UTF-8 value;
//!   flags and numbers use [`EnvSource::var`].
//! - A `no_std` build has no `var_os` (there is no `OsString`); its settings are flags and numbers.
//! - The web and wasm builds have no process environment: their source answers `None` for every
//!   name, which is [`NoEnv`], and they get the defaults.

use alloc::{collections::BTreeMap, string::String};

/// A source of environment variables: the process (in the dll), a map (in a test), nothing
/// ([`NoEnv`]), or any closure `Fn(&str) -> Option<String>`.
///
/// A settings constructor takes `&dyn EnvSource` and asks for the names it knows. The value of
/// `var` is `None` when the variable is unset OR not valid UTF-8 (as `std::env::var(..).ok()`);
/// [`EnvSource::var_os`] is the same lookup that keeps a non-UTF-8 value, for paths.
pub trait EnvSource {
    /// The value of `name` as text, `None` when it is unset or not UTF-8.
    fn var(&self, name: &str) -> Option<String>;

    /// The value of `name` as an OS string (a path keeps its bytes), `None` when it is unset.
    ///
    /// The default answers what [`EnvSource::var`] answers.
    #[cfg(feature = "std")]
    fn var_os(&self, name: &str) -> Option<std::ffi::OsString> {
        self.var(name).map(std::ffi::OsString::from)
    }

    /// Whether `name` is set at all (also to an empty or a non-UTF-8 value), as
    /// `std::env::var_os(name).is_some()`.
    fn is_set(&self, name: &str) -> bool {
        #[cfg(feature = "std")]
        {
            self.var_os(name).is_some()
        }
        #[cfg(not(feature = "std"))]
        {
            self.var(name).is_some()
        }
    }
}

/// A closure is a source: `from_lookup(&|name: &str| ..)`.
impl<F: Fn(&str) -> Option<String>> EnvSource for F {
    fn var(&self, name: &str) -> Option<String> {
        self(name)
    }
}

/// The empty environment: every variable is unset.
///
/// What a settings struct's `Default` is built from, what a library user who never configured
/// anything gets, and what a build without a process environment (web, wasm) answers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NoEnv;

impl EnvSource for NoEnv {
    fn var(&self, _name: &str) -> Option<String> {
        None
    }

    #[cfg(feature = "std")]
    fn var_os(&self, _name: &str) -> Option<std::ffi::OsString> {
        None
    }

    fn is_set(&self, _name: &str) -> bool {
        false
    }
}

/// A source backed by a map, for tests and for hosts that assemble their own configuration.
///
/// Values given with [`MapEnv::with`] answer both `var` and `var_os`; one given with
/// `MapEnv::with_os` (cargo feature `std`) answers `var_os` only, like a non-UTF-8 value of the
/// process.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MapEnv {
    text: BTreeMap<String, String>,
    #[cfg(feature = "std")]
    os_only: BTreeMap<String, std::ffi::OsString>,
}

impl MapEnv {
    /// An empty map (the same as [`NoEnv`] until a value is added).
    #[must_use]
    pub const fn new() -> Self {
        Self {
            text: BTreeMap::new(),
            #[cfg(feature = "std")]
            os_only: BTreeMap::new(),
        }
    }

    /// The map with `name` set to `value` (text, answered by `var` and `var_os`).
    #[must_use]
    pub fn with(mut self, name: &str, value: &str) -> Self {
        self.set(name, value);
        self
    }

    /// Sets `name` to `value` (text, answered by `var` and `var_os`).
    pub fn set(&mut self, name: &str, value: &str) {
        #[cfg(feature = "std")]
        self.os_only.remove(name);
        self.text.insert(String::from(name), String::from(value));
    }

    /// The map with `name` set to an OS string that `var` does not see (a non-UTF-8 path).
    #[cfg(feature = "std")]
    #[must_use]
    pub fn with_os(mut self, name: &str, value: impl Into<std::ffi::OsString>) -> Self {
        self.text.remove(name);
        self.os_only.insert(String::from(name), value.into());
        self
    }

    /// Unsets `name`.
    pub fn remove(&mut self, name: &str) {
        self.text.remove(name);
        #[cfg(feature = "std")]
        self.os_only.remove(name);
    }
}

impl EnvSource for MapEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.text.get(name).cloned()
    }

    #[cfg(feature = "std")]
    fn var_os(&self, name: &str) -> Option<std::ffi::OsString> {
        self.os_only
            .get(name)
            .cloned()
            .or_else(|| self.text.get(name).map(std::ffi::OsString::from))
    }
}

#[cfg(test)]
#[path = "env_test.rs"]
mod tests;
