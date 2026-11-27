//! The ONE reader of the process environment in azul-dll (and, with a tool's `main`, in the
//! whole workspace).
//!
//! Every library crate (azul-css, azul-core, azul-layout, webrender) takes a typed settings value
//! built by a pure `from_lookup(&dyn azul_css::env::EnvSource)`. This module is the source that
//! reads the real process: construct a [`ProcessEnv`], build the settings once at the start
//! (`App::create`), and hand the values down. A change of the environment after that is not seen.
//!
//! Nothing else in the dll calls `std::env::var` or `var_os`; the gate
//! `examples/azul-srcgate/tests/env_gate.rs` counts the reads per file and the count of this file
//! is its two.
//!
//! # Android
//!
//! An Android activity cannot be handed environment variables (`am start` builds an Intent, and
//! the process inherits the zygote's environment, not the shell's), so on Android the reader also
//! answers from the system properties, read ONCE when the reader is made ([`ProcessEnv::new`]):
//!
//! ```text
//! adb shell setprop debug.az.e2e /data/local/tmp/x.json      # the variable AZ_E2E
//! adb shell setprop debug.az.present 1                       # the variable AZ_PRESENT
//! adb shell setprop debug.az.env 'AZ_E2E=/data/local/tmp/x.json;AZ_LOG=debug'
//! ```
//!
//! `debug.az.<name>` is the variable `AZ_<NAME>`; `debug.az.env` carries `KEY=VALUE` pairs
//! separated by `;`, for the variables with no property of their own. The pairs override the real
//! environment (as the `set_var` of the old start-up did), the real environment overrides a
//! `debug.az.<name>` property. An empty value is no value. On every other target there are no
//! properties and the reader is the process environment alone.

use std::collections::BTreeMap;

use azul_css::env::EnvSource;

/// The process environment (and on Android the `debug.az.*` system properties, read once when the
/// reader is made), read at the moment of the call.
///
/// Make ONE with [`ProcessEnv::new`] at the start and pass it as an `&dyn EnvSource` to every
/// `from_lookup`; do not make one per question (on Android making one spawns `getprop`).
#[derive(Debug, Clone)]
pub struct ProcessEnv {
    properties: AndroidProperties,
}

impl ProcessEnv {
    /// The reader of this process. On Android it runs `getprop` once for the `debug.az.*`
    /// properties; everywhere else it reads nothing until it is asked.
    #[must_use]
    pub fn new() -> Self {
        Self {
            properties: AndroidProperties::read(),
        }
    }
}

impl Default for ProcessEnv {
    fn default() -> Self {
        Self::new()
    }
}

impl EnvSource for ProcessEnv {
    fn var(&self, name: &str) -> Option<String> {
        self.properties
            .pair(name)
            .map(str::to_owned)
            .or_else(|| std::env::var(name).ok())
            .or_else(|| self.properties.named(name).map(str::to_owned))
    }

    fn var_os(&self, name: &str) -> Option<std::ffi::OsString> {
        self.properties
            .pair(name)
            .map(std::ffi::OsString::from)
            .or_else(|| std::env::var_os(name))
            .or_else(|| self.properties.named(name).map(std::ffi::OsString::from))
    }
}

/// `AppConfig::create` of the C API and of the `azul` crate (api.json names this function).
///
/// The config of `azul_core::resources::AppConfig::create`, which reads no environment, with the
/// default text anti-aliasing replaced by what `AZ_TEXT_AA` and the `AZ_LCD_*` variables of the
/// process environment ask for. An app that sets it afterwards decides for itself. It runs
/// before `App::create`, so it makes a reader of its own ([`ProcessEnv::new`]).
#[must_use]
pub fn app_config_create() -> azul_core::resources::AppConfig {
    let mut config = azul_core::resources::AppConfig::create();
    config.text_antialiasing =
        azul_core::resources::TextAntialiasing::from_lookup(&ProcessEnv::new());
    config
}

/// The `debug.az.*` system properties of an Android process, as variables.
///
/// Plain data, so the parsing is tested on every target; only [`AndroidProperties::read`], which
/// runs `getprop`, is Android's.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AndroidProperties {
    /// `debug.az.env`: the `KEY=VALUE` pairs. They override the process environment.
    pairs: BTreeMap<String, String>,
    /// `debug.az.<name>` as the variable `AZ_<NAME>`: asked when the environment has no value.
    named: BTreeMap<String, String>,
}

impl AndroidProperties {
    /// The properties of this process: the `debug.az.*` entries of the system's property area, read
    /// through bionic on Android, none elsewhere. No process is spawned: this used to run `getprop`
    /// (which forks, executes a binary and lists every property of the phone), once per reader, and
    /// a start makes several readers (`AppConfig::create`, `App::create`, the services that have no
    /// app): the startup timeline counted them (2026-11-26).
    #[cfg(target_os = "android")]
    #[must_use]
    pub fn read() -> Self {
        let entries = bionic::debug_az_entries();
        Self::from_entries(entries.iter().map(|(name, value)| (name.as_str(), value.as_str())))
    }

    /// No properties: only Android has them.
    #[cfg(not(target_os = "android"))]
    #[must_use]
    pub fn read() -> Self {
        Self::default()
    }

    /// The `debug.az.*` entries of what `getprop` prints, one `[name]: [value]` per line. Every
    /// other line, an entry with an empty value and a name that is only the prefix are skipped.
    #[must_use]
    pub fn from_listing(listing: &str) -> Self {
        Self::from_entries(listing.lines().filter_map(split_property))
    }

    /// The `debug.az.*` entries of `(name, value)` pairs, as the system's property area gives them
    /// ([`Self::from_listing`] is the same for the text of `getprop`): every other name, an entry
    /// with an empty value and a name that is only the prefix are skipped.
    #[must_use]
    pub fn from_entries<'a>(entries: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut properties = Self::default();
        for (name, value) in entries {
            let Some(suffix) = name.strip_prefix("debug.az.") else {
                continue;
            };
            if value.is_empty() || suffix.is_empty() {
                continue;
            }
            if suffix == "env" {
                properties.pairs = parse_pairs(value);
            } else {
                properties
                    .named
                    .insert(format!("AZ_{}", suffix.to_ascii_uppercase()), value.to_owned());
            }
        }
        properties
    }

    /// The value `debug.az.env` gives `name`, which outranks the environment.
    fn pair(&self, name: &str) -> Option<&str> {
        self.pairs.get(name).map(String::as_str)
    }

    /// The value of the property `debug.az.<name>` for the variable `name`, which the environment
    /// outranks.
    fn named(&self, name: &str) -> Option<&str> {
        self.named.get(name).map(String::as_str)
    }
}

/// `[debug.az.e2e]: [/data/local/tmp/x.json]` as the name and the value.
fn split_property(line: &str) -> Option<(&str, &str)> {
    let rest = line.trim().strip_prefix('[')?;
    let (name, value) = rest.split_once("]: [")?;
    Some((name, value.strip_suffix(']')?))
}

/// `KEY=VALUE;KEY=VALUE`: a pair with no `=` or an empty key is skipped, not turned into a
/// nonsense variable; blanks around either side are dropped.
fn parse_pairs(raw: &str) -> BTreeMap<String, String> {
    raw.split(';')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.trim(), value.trim()))
        .filter(|(key, _)| !key.is_empty())
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

/// The system's property area on Android, read through bionic (`<sys/system_properties.h>`):
/// `__system_property_foreach` passes every property to a callback and `__system_property_read`
/// copies its name and value out. Both exist at every API level (the NDK header marks only the
/// callback-based reader as API 26); the name buffer is `PROP_NAME_MAX` (32) bytes, which holds
/// `debug.az.<name>`, and the value buffer `PROP_VALUE_MAX` (92): the limit of `setprop` itself.
#[cfg(target_os = "android")]
mod bionic {
    use core::ffi::{c_char, c_int, c_void, CStr};

    /// `struct prop_info`: opaque, only ever passed back to bionic.
    #[repr(C)]
    pub struct PropInfo {
        _opaque: [u8; 0],
    }

    const PROP_NAME_MAX: usize = 32;
    const PROP_VALUE_MAX: usize = 92;
    /// The prefix of the properties the engine reads.
    const PREFIX: &str = "debug.az.";

    extern "C" {
        fn __system_property_foreach(
            callback: unsafe extern "C" fn(*const PropInfo, *mut c_void),
            cookie: *mut c_void,
        ) -> c_int;
        fn __system_property_read(
            info: *const PropInfo,
            name: *mut c_char,
            value: *mut c_char,
        ) -> c_int;
    }

    /// Called by bionic once per property: keeps the `debug.az.*` ones.
    unsafe extern "C" fn collect(info: *const PropInfo, cookie: *mut c_void) {
        let mut name = [0 as c_char; PROP_NAME_MAX];
        let mut value = [0 as c_char; PROP_VALUE_MAX];
        // SAFETY: `info` comes from bionic, the buffers have the sizes it documents.
        let length = unsafe { __system_property_read(info, name.as_mut_ptr(), value.as_mut_ptr()) };
        if length < 0 {
            return;
        }
        // SAFETY: bionic NUL-terminates both buffers.
        let (name, value) = unsafe {
            (
                CStr::from_ptr(name.as_ptr()).to_string_lossy().into_owned(),
                CStr::from_ptr(value.as_ptr()).to_string_lossy().into_owned(),
            )
        };
        if name.starts_with(PREFIX) {
            // SAFETY: `cookie` is the `&mut Vec` of `debug_az_entries`, alive for the whole walk.
            let entries = unsafe { &mut *cookie.cast::<Vec<(String, String)>>() };
            entries.push((name, value));
        }
    }

    /// The `debug.az.*` properties as `(name, value)`; empty when the walk fails.
    pub fn debug_az_entries() -> Vec<(String, String)> {
        let mut entries: Vec<(String, String)> = Vec::new();
        // SAFETY: the callback only reads what bionic hands it and pushes into `entries`, which
        // outlives the call.
        unsafe {
            __system_property_foreach(collect, core::ptr::addr_of_mut!(entries).cast::<c_void>());
        }
        entries
    }
}

#[cfg(test)]
#[path = "env_test.rs"]
mod tests;
