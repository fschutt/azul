//! What a run decides before it talks to anyone, and where every value came
//! from: the shared config file, the endpoints in it (azul-appkit's
//! [`resolve_endpoints`]: built-in profile < file < environment < flag), the
//! transport, a pinned iroh node, and the folders - this device's state
//! folder, the data root of every Azlin app, the `.azlin` folder.
//!
//! [`Settings::resolve`] is pure (the environment, the OS folders and the
//! file reader come in), so the tests check the precedence without touching
//! the machine; [`Settings::from_process`] is the real run. [`Settings::report`]
//! is what `azcloud config` prints: a value that came from a built-in default
//! says so, which is how an address nobody configured is found.

use std::path::{Path, PathBuf};

use azul_appkit::azlin_config::{
    config_path, resolve_endpoints, AzlinConfig, EffectiveEndpoints, Endpoint, EndpointFlags,
    Resolved, Source, CONFIG_DIR, CONFIG_VAR,
};
use serde_json::{json, Value};

use crate::drive::TransportPref;

/// The variable naming this device's state folder.
pub const STATE_VAR: &str = "AZCLOUD_HOME";
/// The flag naming it.
pub const STATE_FLAG: &str = "--state-dir";
/// The state folder's name in the OS config folder.
pub const STATE_DIR_NAME: &str = "azcloud";
/// The variable choosing the transport: `auto`, `iroh`, `https`
/// (AZDRIVE-INTEGRATION.md §3 names it).
pub const TRANSPORT_VAR: &str = "AZCLOUD_TRANSPORT";
/// The flag choosing it.
pub const TRANSPORT_FLAG: &str = "--transport";
/// The variable pinning the iroh node to dial (its endpoint id: the node's
/// Ed25519 key, hex or base32), as `azctl test client --iroh` sets it.
pub const IROH_NODE_VAR: &str = "AZLIN_IROH_NODE";
/// The flag pinning it.
pub const IROH_NODE_FLAG: &str = "--iroh-node";
/// The variable naming that node's UDP address (`ip:port`), so the dial needs
/// no discovery.
pub const IROH_ADDR_VAR: &str = "AZLIN_IROH_ADDR";
/// The flag naming it.
pub const IROH_ADDR_FLAG: &str = "--iroh-addr";
/// The variable naming the `.azlin` folder the Azlin sync takes.
pub const AZLIN_HOME_VAR: &str = "AZLIN_HOME";
/// The flag naming it.
pub const AZLIN_HOME_FLAG: &str = "--azlin-home";
/// The flag naming the shared config file (over `AZLIN_CONFIG`).
pub const CONFIG_FLAG: &str = "--config";
/// The flag naming the data root (azul-appkit's `--data-dir`).
pub const DATA_FLAG: &str = "--data-dir";

/// The command-line layer of everything (`None` = not given).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Flags {
    pub endpoints: EndpointFlags,
    pub config: Option<PathBuf>,
    pub state_dir: Option<PathBuf>,
    pub data_dir: Option<PathBuf>,
    pub azlin_home: Option<PathBuf>,
    pub transport: Option<String>,
    pub iroh_node: Option<String>,
    pub iroh_addr: Option<String>,
}

/// A folder or file a run uses and where it came from; `path: None` = none at
/// all (`AZLIN_CONFIG=off`, a machine without a home folder).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathSetting {
    pub path: Option<PathBuf>,
    pub source: Source,
    /// What the default stands for, for people ("the OS data folder").
    pub note: String,
}

impl PathSetting {
    fn of(path: PathBuf, source: Source, note: &str) -> PathSetting {
        PathSetting {
            path: Some(path),
            source,
            note: note.to_string(),
        }
    }

    fn none(source: Source, note: &str) -> PathSetting {
        PathSetting {
            path: None,
            source,
            note: note.to_string(),
        }
    }
}

/// The OS folders a run starts from: the ones azul's `FilePath::get_home_dir`,
/// `get_config_dir` and `get_data_dir` return (both use `dirs`), so the data
/// root here is the one the apps write.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OsDirs {
    pub home: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub data: Option<PathBuf>,
}

impl OsDirs {
    /// This user's.
    #[must_use]
    pub fn of_this_user() -> OsDirs {
        OsDirs {
            home: dirs::home_dir(),
            config: dirs::config_dir(),
            data: dirs::data_dir(),
        }
    }
}

/// Everything a run decided.
#[derive(Clone, Debug)]
pub struct Settings {
    /// The shared config file (`--config`, `AZLIN_CONFIG`, `~/.azlin/config.json`).
    pub config_file: PathSetting,
    /// What it said.
    pub config: AzlinConfig,
    /// What could not be read in it.
    pub config_problem: Option<String>,
    /// The endpoints, after the layers.
    pub endpoints: EffectiveEndpoints,
    /// `auto`, `iroh` or `https`.
    pub transport: Resolved,
    /// A pinned iroh node id.
    pub iroh_node: Resolved,
    /// Its UDP address.
    pub iroh_addr: Resolved,
    /// This device's state folder.
    pub state_dir: PathSetting,
    /// The data root of every Azlin app.
    pub data_root: PathSetting,
    /// The `.azlin` folder the Azlin sync takes.
    pub azlin_home: PathSetting,
}

fn check_transport(raw: &str) -> Result<String, String> {
    TransportPref::parse(raw)
        .map(|t| t.name().to_string())
        .ok_or_else(|| String::from("not a transport (auto, iroh, https)"))
}

fn check_iroh_node(raw: &str) -> Result<String, String> {
    let ok = matches!(raw.len(), 52 | 64) && raw.bytes().all(|b| b.is_ascii_alphanumeric());
    if ok {
        Ok(raw.to_string())
    } else {
        Err(String::from(
            "not an iroh endpoint id (64 hex or 52 base32 characters)",
        ))
    }
}

fn check_socket_addr(raw: &str) -> Result<String, String> {
    raw.parse::<std::net::SocketAddr>()
        .map(|a| a.to_string())
        .map_err(|e| format!("not an ip:port address ({e})"))
}

/// One setting through the layers default < environment < flag.
fn layered(
    default: Option<&str>,
    var: &'static str,
    flag_name: &'static str,
    flag: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
    check: fn(&str) -> Result<String, String>,
) -> Resolved {
    let mut resolved = Resolved::unset();
    if let Some(default) = default {
        resolved.offer(Source::BuiltIn, default, check);
    }
    if let Some(raw) = env(var) {
        resolved.offer(Source::Env(var), &raw, check);
    }
    if let Some(raw) = flag {
        resolved.offer(Source::Flag(flag_name), raw, check);
    }
    resolved
}

fn non_blank(path: Option<&PathBuf>) -> Option<&PathBuf> {
    path.filter(|p| !p.as_os_str().is_empty())
}

fn non_blank_var(env: &dyn Fn(&str) -> Option<String>, var: &str) -> Option<String> {
    env(var)
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

impl Settings {
    /// This process's settings: its environment, this user's OS folders, the
    /// config file they name.
    #[must_use]
    pub fn from_process(flags: &Flags) -> Settings {
        let env = |name: &str| std::env::var(name).ok();
        Settings::resolve(flags, &env, &OsDirs::of_this_user(), &AzlinConfig::load)
    }

    /// The settings from `flags`, the environment `env`, the OS folders `os`
    /// and the config file as `load` reads it.
    #[must_use]
    pub fn resolve(
        flags: &Flags,
        env: &dyn Fn(&str) -> Option<String>,
        os: &OsDirs,
        load: &dyn Fn(&Path) -> (AzlinConfig, Option<String>),
    ) -> Settings {
        let config_file = config_file(flags, env, os);
        let (config, config_problem) = match &config_file.path {
            Some(path) => load(path),
            None => (AzlinConfig::default(), None),
        };
        let endpoints = resolve_endpoints(
            config_file
                .path
                .as_deref()
                .map(|path| (path, &config.endpoints)),
            env,
            &flags.endpoints,
        );
        let transport = layered(
            Some(TransportPref::Auto.name()),
            TRANSPORT_VAR,
            TRANSPORT_FLAG,
            flags.transport.as_deref(),
            env,
            check_transport,
        );
        let iroh_node = layered(
            None,
            IROH_NODE_VAR,
            IROH_NODE_FLAG,
            flags.iroh_node.as_deref(),
            env,
            check_iroh_node,
        );
        let iroh_addr = layered(
            None,
            IROH_ADDR_VAR,
            IROH_ADDR_FLAG,
            flags.iroh_addr.as_deref(),
            env,
            check_socket_addr,
        );
        Settings {
            state_dir: state_dir(flags, env, os),
            data_root: data_root(flags, env, os),
            azlin_home: azlin_home(flags, env, os),
            config_file,
            config,
            config_problem,
            endpoints,
            transport,
            iroh_node,
            iroh_addr,
        }
    }

    /// The transport asked for (`auto` unless a layer said otherwise).
    #[must_use]
    pub fn transport_pref(&self) -> TransportPref {
        self.transport
            .value
            .as_deref()
            .and_then(TransportPref::parse)
            .unwrap_or(TransportPref::Auto)
    }

    /// The token server's address.
    ///
    /// # Errors
    ///
    /// When no layer gave one (a profile without a token server).
    pub fn token_url(&self) -> anyhow::Result<&str> {
        self.endpoints.url(Endpoint::Token).ok_or_else(|| {
            anyhow::anyhow!(
                "no token server: set endpoints.token in {}, AZLIN_TOKEN_URL or --token-url",
                self.config_file_label()
            )
        })
    }

    /// The relay setting: `off`, `default` or an address.
    #[must_use]
    pub fn relay(&self) -> Option<&str> {
        self.endpoints.url(Endpoint::Relay)
    }

    /// The state folder.
    ///
    /// # Errors
    ///
    /// When no layer named one (no home folder and no `AZCLOUD_HOME`).
    pub fn state_path(&self) -> anyhow::Result<&Path> {
        self.state_dir
            .path
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("no state folder: set AZCLOUD_HOME or pass --state-dir"))
    }

    fn config_file_label(&self) -> String {
        match &self.config_file.path {
            Some(path) => path.display().to_string(),
            None => String::from("the shared config (AZLIN_CONFIG is off)"),
        }
    }

    /// Every value that came from a built-in default, as `name (source)`: the
    /// addresses and folders nobody configured.
    #[must_use]
    pub fn defaults_in_use(&self) -> Vec<String> {
        let mut out = Vec::new();
        if matches!(self.endpoints.profile_setting.source, Source::BuiltIn) {
            out.push(format!("profile {}", self.endpoints.profile.name()));
        }
        for endpoint in Endpoint::ALL {
            let r = self.endpoints.get(endpoint);
            if matches!(r.source, Source::Profile(_) | Source::BuiltIn) {
                out.push(format!(
                    "{} = {} ({})",
                    endpoint.key(),
                    redact_url(r.value.as_deref().unwrap_or("")),
                    r.source.label()
                ));
            }
        }
        if matches!(self.transport.source, Source::BuiltIn) {
            out.push(format!(
                "transport = {} (built-in default)",
                self.transport_pref().name()
            ));
        }
        for (name, p) in [
            ("config file", &self.config_file),
            ("state folder", &self.state_dir),
            ("data root", &self.data_root),
            (".azlin folder", &self.azlin_home),
        ] {
            if matches!(p.source, Source::BuiltIn) {
                out.push(format!(
                    "{name} = {} (built-in default: {})",
                    p.path
                        .as_deref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_default(),
                    p.note
                ));
            }
        }
        out
    }

    /// What `azcloud config` prints: as JSON and as text. `drive_endpoint` is
    /// the S3 endpoint the current drive's bundle names, if a drive exists.
    #[must_use]
    pub fn report(&self, drive_endpoint: Option<&str>) -> (Value, String) {
        let mut endpoints = serde_json::Map::new();
        let mut text = format!(
            "profile    {:<34} {}\n",
            self.endpoints.profile.name(),
            self.endpoints.profile_setting.source.label()
        );
        for endpoint in Endpoint::ALL {
            let r = self.endpoints.get(endpoint);
            let mut j = resolved_json(r);
            if endpoint == Endpoint::S3 && r.value.is_none() {
                j["from_drive"] = json!(drive_endpoint);
            }
            endpoints.insert(endpoint.key().to_string(), j);
            let shown = match (&r.value, endpoint, drive_endpoint) {
                (Some(v), _, _) => redact_url(v),
                (None, Endpoint::S3, Some(d)) => format!("{} (drive)", redact_url(d)),
                (None, Endpoint::S3, None) => String::from("(the drive's, at signup)"),
                (None, _, _) => String::from("-"),
            };
            text.push_str(&format!(
                "{:<10} {:<34} {}\n",
                endpoint.key(),
                shown,
                r.source.label()
            ));
            text.push_str(&describe_layers(r));
        }
        for (name, r) in [
            ("transport", &self.transport),
            ("iroh node", &self.iroh_node),
            ("iroh addr", &self.iroh_addr),
        ] {
            text.push_str(&format!(
                "{:<10} {:<34} {}\n",
                name,
                r.value.as_deref().unwrap_or("-"),
                r.source.label()
            ));
            text.push_str(&describe_layers(r));
        }
        let mut paths = serde_json::Map::new();
        for (name, p) in [
            ("config_file", &self.config_file),
            ("state_dir", &self.state_dir),
            ("data_root", &self.data_root),
            ("azlin_home", &self.azlin_home),
        ] {
            paths.insert(
                name.to_string(),
                json!({
                    "path": p.path.as_deref().map(|p| p.display().to_string()),
                    "source": p.source.label(),
                    "kind": p.source.kind(),
                    "note": p.note,
                }),
            );
            let shown = p
                .path
                .as_deref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| String::from("(none)"));
            let note = if p.note.is_empty() {
                String::new()
            } else {
                format!(": {}", p.note)
            };
            text.push_str(&format!(
                "{:<10} {} ({}{note})\n",
                name.replace('_', " "),
                shown,
                p.source.label()
            ));
        }
        if let Some(problem) = &self.config_problem {
            text.push_str(&format!("config problem: {problem}\n"));
        }
        let defaults = self.defaults_in_use();
        if !defaults.is_empty() {
            text.push_str("built-in defaults in use (nobody configured them):\n");
            for d in &defaults {
                text.push_str(&format!("  {d}\n"));
            }
        }
        let value = json!({
            "ok": true,
            "profile": resolved_json(&self.endpoints.profile_setting),
            "endpoints": Value::Object(endpoints),
            "transport": resolved_json(&self.transport),
            "iroh_node": resolved_json(&self.iroh_node),
            "iroh_addr": resolved_json(&self.iroh_addr),
            "paths": Value::Object(paths),
            "config_problem": self.config_problem,
            "defaults_in_use": defaults,
        });
        (value, text)
    }
}

fn describe_layers(r: &Resolved) -> String {
    let mut out = String::new();
    for (source, value) in &r.overridden {
        out.push_str(&format!(
            "           overrides {} = {}\n",
            source.label(),
            redact_url(value)
        ));
    }
    for (source, value, why) in &r.rejected {
        out.push_str(&format!(
            "           IGNORED {} = {:?}: {why}\n",
            source.label(),
            redact_url(value)
        ));
    }
    out
}

/// One setting as JSON: its value, where it came from (a label and one
/// word), what it overrode, what was rejected.
#[must_use]
pub fn resolved_json(r: &Resolved) -> Value {
    json!({
        "value": r.value.as_deref().map(redact_url),
        "source": r.source.label(),
        "kind": r.source.kind(),
        "overridden": r.overridden.iter().map(|(s, v)| json!({
            "source": s.label(), "kind": s.kind(), "value": redact_url(v),
        })).collect::<Vec<_>>(),
        "rejected": r.rejected.iter().map(|(s, v, why)| json!({
            "source": s.label(), "kind": s.kind(), "value": redact_url(v), "why": why,
        })).collect::<Vec<_>>(),
    })
}

/// An address with any `user:password@` in it replaced by `***@`: an endpoint
/// is printed, a password in it must not be.
#[must_use]
pub fn redact_url(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    match rest[..authority_end].rfind('@') {
        Some(at) => format!("{scheme}://***@{}", &rest[at + 1..]),
        None => url.to_string(),
    }
}

fn config_file(flags: &Flags, env: &dyn Fn(&str) -> Option<String>, os: &OsDirs) -> PathSetting {
    if let Some(path) = non_blank(flags.config.as_ref()) {
        return PathSetting::of(path.clone(), Source::Flag(CONFIG_FLAG), "");
    }
    let var = env(CONFIG_VAR);
    let path = config_path(var.as_deref(), os.home.as_deref());
    match (var, path) {
        (Some(_), Some(path)) => PathSetting::of(path, Source::Env(CONFIG_VAR), ""),
        (Some(_), None) => PathSetting::none(Source::Env(CONFIG_VAR), "off: no shared config"),
        (None, Some(path)) => PathSetting::of(path, Source::BuiltIn, "~/.azlin/config.json"),
        (None, None) => PathSetting::none(Source::Unset, "no home folder"),
    }
}

fn state_dir(flags: &Flags, env: &dyn Fn(&str) -> Option<String>, os: &OsDirs) -> PathSetting {
    if let Some(path) = non_blank(flags.state_dir.as_ref()) {
        return PathSetting::of(path.clone(), Source::Flag(STATE_FLAG), "");
    }
    if let Some(var) = non_blank_var(env, STATE_VAR) {
        return PathSetting::of(PathBuf::from(var), Source::Env(STATE_VAR), "");
    }
    if let Some(config) = &os.config {
        return PathSetting::of(
            config.join(STATE_DIR_NAME),
            Source::BuiltIn,
            "the OS config folder",
        );
    }
    if let Some(home) = &os.home {
        return PathSetting::of(
            home.join(".azcloud"),
            Source::BuiltIn,
            "the home folder (no OS config folder)",
        );
    }
    PathSetting::none(Source::Unset, "no home folder")
}

fn data_root(flags: &Flags, env: &dyn Fn(&str) -> Option<String>, os: &OsDirs) -> PathSetting {
    let flag = non_blank(flags.data_dir.as_ref());
    let var = non_blank_var(env, azul_appkit::data::DATA_VAR);
    let path =
        azul_appkit::data::data_root(flag.map(PathBuf::as_path), var.as_deref(), os.data.clone());
    let (source, note) = if flag.is_some() {
        (Source::Flag(DATA_FLAG), "")
    } else if var.is_some() {
        (Source::Env(azul_appkit::data::DATA_VAR), "")
    } else if os.data.is_some() {
        (Source::BuiltIn, "the OS data folder (every Azlin app's)")
    } else {
        (Source::BuiltIn, "the working folder (no OS data folder)")
    };
    PathSetting::of(path, source, note)
}

fn azlin_home(flags: &Flags, env: &dyn Fn(&str) -> Option<String>, os: &OsDirs) -> PathSetting {
    if let Some(path) = non_blank(flags.azlin_home.as_ref()) {
        return PathSetting::of(path.clone(), Source::Flag(AZLIN_HOME_FLAG), "");
    }
    if let Some(var) = non_blank_var(env, AZLIN_HOME_VAR) {
        return PathSetting::of(PathBuf::from(var), Source::Env(AZLIN_HOME_VAR), "");
    }
    match &os.home {
        Some(home) => PathSetting::of(
            home.join(CONFIG_DIR),
            Source::BuiltIn,
            "the shared config's folder in the home folder",
        ),
        None => PathSetting::none(Source::Unset, "no home folder"),
    }
}

#[cfg(test)]
mod tests {
    use azul_appkit::azlin_config::{EndpointsSection, Profile};

    use super::*;

    fn os() -> OsDirs {
        OsDirs {
            home: Some(PathBuf::from("/home/ann")),
            config: Some(PathBuf::from("/home/ann/.config")),
            data: Some(PathBuf::from("/home/ann/.local/share")),
        }
    }

    fn no_file(_: &Path) -> (AzlinConfig, Option<String>) {
        (AzlinConfig::default(), None)
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn with_nothing_configured_every_folder_and_endpoint_is_a_built_in_default_and_says_so() {
        let s = Settings::resolve(&Flags::default(), &no_env, &os(), &no_file);
        assert_eq!(
            s.config_file.path,
            Some(PathBuf::from("/home/ann/.azlin/config.json"))
        );
        assert_eq!(
            s.state_dir.path,
            Some(PathBuf::from("/home/ann/.config/azcloud"))
        );
        assert_eq!(
            s.data_root.path,
            Some(PathBuf::from("/home/ann/.local/share/Azlin"))
        );
        assert_eq!(s.azlin_home.path, Some(PathBuf::from("/home/ann/.azlin")));
        assert_eq!(s.transport_pref(), TransportPref::Auto);
        let defaults = s.defaults_in_use().join("\n");
        for expected in [
            "profile local",
            "token = http://127.0.0.1:8081 (profile local)",
            "transport = auto",
            "state folder",
            "data root",
        ] {
            assert!(defaults.contains(expected), "{expected:?} in {defaults}");
        }
        assert!(
            !defaults.contains("s3 ="),
            "the S3 endpoint is the drive's, not a default"
        );
    }

    #[test]
    fn the_config_file_the_environment_and_the_flags_are_layered_in_that_order() {
        let load = |_: &Path| -> (AzlinConfig, Option<String>) {
            let mut c = AzlinConfig::default();
            c.endpoints.profile = Some(Profile::Local);
            c.endpoints
                .set(Endpoint::Token, Some("http://127.0.0.1:18081"))
                .unwrap();
            (c, None)
        };
        let env = |name: &str| -> Option<String> {
            match name {
                "AZLIN_CONFIG" => Some(String::from("/tmp/e2e/config.json")),
                "AZCLOUD_TRANSPORT" => Some(String::from("https")),
                "AZCLOUD_HOME" => Some(String::from("/tmp/e2e/state")),
                "AZLIN_DATA" => Some(String::from(" /tmp/e2e/data ")),
                "AZLIN_IROH_NODE" => Some("ab".repeat(32)),
                "AZLIN_IROH_ADDR" => Some(String::from("127.0.0.1:41000")),
                _ => None,
            }
        };
        let s = Settings::resolve(&Flags::default(), &env, &os(), &load);
        assert_eq!(
            s.config_file.path,
            Some(PathBuf::from("/tmp/e2e/config.json"))
        );
        assert_eq!(s.config_file.source, Source::Env("AZLIN_CONFIG"));
        assert_eq!(
            s.endpoints.url(Endpoint::Token),
            Some("http://127.0.0.1:18081")
        );
        assert_eq!(
            s.endpoints.get(Endpoint::Token).source,
            Source::File(PathBuf::from("/tmp/e2e/config.json"))
        );
        assert_eq!(s.transport_pref(), TransportPref::Https);
        assert_eq!(s.state_dir.path, Some(PathBuf::from("/tmp/e2e/state")));
        assert_eq!(s.data_root.path, Some(PathBuf::from("/tmp/e2e/data")));
        assert_eq!(s.data_root.source, Source::Env("AZLIN_DATA"));
        assert_eq!(s.iroh_addr.value.as_deref(), Some("127.0.0.1:41000"));

        let flags = Flags {
            transport: Some(String::from("iroh")),
            state_dir: Some(PathBuf::from("/srv/state")),
            config: Some(PathBuf::from("/srv/config.json")),
            endpoints: EndpointFlags {
                token: Some(String::from("https://token.test")),
                ..EndpointFlags::default()
            },
            ..Flags::default()
        };
        let s = Settings::resolve(&flags, &env, &os(), &load);
        assert_eq!(s.config_file.source, Source::Flag("--config"));
        assert_eq!(s.transport_pref(), TransportPref::Iroh);
        assert_eq!(s.state_dir.path, Some(PathBuf::from("/srv/state")));
        assert_eq!(s.endpoints.url(Endpoint::Token), Some("https://token.test"));
    }

    #[test]
    fn a_shared_config_switched_off_leaves_the_profile_and_a_bad_value_is_ignored_with_a_reason() {
        let env = |name: &str| -> Option<String> {
            match name {
                "AZLIN_CONFIG" => Some(String::from("off")),
                "AZCLOUD_TRANSPORT" => Some(String::from("carrier-pigeon")),
                "AZLIN_IROH_ADDR" => Some(String::from("localhost")),
                _ => None,
            }
        };
        let s = Settings::resolve(&Flags::default(), &env, &os(), &no_file);
        assert_eq!(s.config_file.path, None);
        assert_eq!(
            s.endpoints.get(Endpoint::Token).source,
            Source::Profile(Profile::Local)
        );
        assert_eq!(s.transport_pref(), TransportPref::Auto);
        assert_eq!(s.transport.rejected.len(), 1);
        assert_eq!(s.iroh_addr.value, None);
        assert_eq!(s.iroh_addr.rejected.len(), 1);
        let (json, text) = s.report(None);
        assert_eq!(json["transport"]["rejected"][0]["value"], "carrier-pigeon");
        assert!(text.contains("IGNORED"), "{text}");
    }

    #[test]
    fn the_report_names_the_source_of_every_endpoint_and_hides_passwords_in_addresses() {
        let env = |name: &str| -> Option<String> {
            (name == "AZLIN_TOKEN_URL").then(|| String::from("http://ann:secret@127.0.0.1:8081"))
        };
        let s = Settings::resolve(&Flags::default(), &env, &os(), &no_file);
        let (json, text) = s.report(Some("http://127.0.0.1:9000"));
        assert_eq!(json["endpoints"]["token"]["kind"], "env");
        assert_eq!(
            json["endpoints"]["token"]["source"],
            "environment AZLIN_TOKEN_URL"
        );
        assert_eq!(json["endpoints"]["s3"]["kind"], "unset");
        assert_eq!(
            json["endpoints"]["s3"]["from_drive"],
            "http://127.0.0.1:9000"
        );
        assert_eq!(json["endpoints"]["meet"]["kind"], "profile");
        assert!(!text.contains("secret"), "{text}");
        assert!(!json.to_string().contains("secret"), "{json}");
        assert_eq!(
            redact_url("https://u:p@host.test:1/x"),
            "https://***@host.test:1/x"
        );
        assert_eq!(redact_url("http://host.test/a@b"), "http://host.test/a@b");
    }

    #[test]
    fn a_file_section_is_read_through_the_loader_it_is_given() {
        let load = |path: &Path| -> (AzlinConfig, Option<String>) {
            assert_eq!(path, Path::new("/home/ann/.azlin/config.json"));
            let (c, problem) = AzlinConfig::parse(
                r#"{"endpoints": {"profile": "trial", "meet": "http://127.0.0.1:8787"}}"#,
            );
            (c, problem)
        };
        let s = Settings::resolve(&Flags::default(), &no_env, &os(), &load);
        assert_eq!(s.endpoints.profile, Profile::Trial);
        assert_eq!(
            s.endpoints.url(Endpoint::Token),
            Some("https://token-trial.azlin.io")
        );
        assert_eq!(
            s.endpoints.url(Endpoint::Meet),
            Some("http://127.0.0.1:8787")
        );
        assert!(EndpointsSection::default().is_empty());
    }
}
