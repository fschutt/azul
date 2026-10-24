//! The SHARED config of the Azlin apps: `~/.azlin/config.json`, one file for
//! every app a user runs, so the look they chose in one app is the look of
//! all of them ("one style config").
//!
//! ```json
//! {
//!   "currentTheme": "flora:green",
//!   "mode": "dark",
//!   "endpoints": {
//!     "profile": "local",
//!     "token": "http://127.0.0.1:8081"
//!   }
//! }
//! ```
//!
//! - `currentTheme`: the app theme - `flat`, `flora` or one of flora's spins
//!   (`flora:green`, `flora:red`, `flora:purple`, `flora:gold`,
//!   `flora:rose`; [`Theme`]). An app that pins its own theme (AzWriter
//!   flora, AzSheets flora:green, AzShow flora:red) neither reads nor writes
//!   it.
//! - `mode`: `system`, `light` or `dark` - shared by every app, pinned or not.
//! - `endpoints`: where the Azlin services are ([`EndpointsSection`]): a
//!   `profile` (`local`, `trial`, `production`; [`Profile`]) whose built-in
//!   addresses are the defaults, and `token`, `s3`, `meet`, `relay` to
//!   override one of them. A run overrides the file with an environment
//!   variable ([`Endpoint::vars`], [`PROFILE_VAR`]) or a flag;
//!   [`resolve_endpoints`] applies the layers (built-in < file < environment
//!   < flag) and says where every value came from. No address is built into
//!   an app outside the profile table, so a value nobody configured shows up
//!   as "profile ..." in `azcloud config`. The section never leaves this
//!   computer ([`MACHINE_LOCAL_KEYS`]): the sync of the `.azlin` folder strips
//!   it.
//! - `updates`: how the Azlin apps stay current ([`UpdatesSection`]), in the
//!   shape of azul's machine-wide `updates` section (`autoupdate`,
//!   `maintenance_window`, `overrides.<app>`) plus what only the Azlin apps
//!   share: the `channel` (stable or beta), the machine's rollout `cohort`,
//!   the `install_dir` AzInstallManager puts the apps in, the `manifest_url`
//!   and the `root_public_key` of the manifests. Machine-local like the
//!   endpoints: the cohort and the install folder belong to this computer.
//! - Any other key is kept as it was: the file is the place for the next
//!   shared preference, and an older app must not drop a newer one's key
//!   (inside `endpoints` too).
//!
//! Every app reads it once, at its start (`ui::create_kit`, over its own
//! `settings.json`), and writes it whenever its theme or mode changes
//! (`ui::save_settings`). `AZLIN_CONFIG` names another file (a test's);
//! `AZLIN_CONFIG=off` (or empty) means no shared config at all, and a
//! `--shot` run (a screenshot regression fixture) never reads one. `AZ_THEME`
//! still outranks it for a run: azul resolves the environment over the app's
//! choice.
//!
//! Reading is forgiving like `settings.json`'s: a missing file is no
//! opinion, an unknown theme or mode is ignored (with a line saying so), a
//! file that is not JSON is left alone and reported. Writing goes through a
//! temporary file and a rename, so a crash never leaves half a config.
//!
//! No azul types here: tested without a window.

use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::args::{ModePref, Theme};

/// The environment variable that names another config file (`off` or empty:
/// none).
pub const CONFIG_VAR: &str = "AZLIN_CONFIG";
/// The folder in the home directory.
pub const CONFIG_DIR: &str = ".azlin";
/// The file in it.
pub const CONFIG_FILE: &str = "config.json";
/// The key of the app theme.
pub const THEME_KEY: &str = "currentTheme";
/// The key of the light / dark mode.
pub const MODE_KEY: &str = "mode";

/// Where the shared config lives: `$AZLIN_CONFIG` if it is set (`None` when
/// it says `off` or nothing), else `<home>/.azlin/config.json`; `None`
/// without a home directory.
#[must_use]
pub fn config_path(var: Option<&str>, home: Option<&Path>) -> Option<PathBuf> {
    match var.map(str::trim) {
        Some(v) if v.is_empty() || v.eq_ignore_ascii_case("off") => None,
        Some(v) => Some(PathBuf::from(v)),
        None => home
            .filter(|h| !h.as_os_str().is_empty())
            .map(|h| h.join(CONFIG_DIR).join(CONFIG_FILE)),
    }
}

/// The shared config.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AzlinConfig {
    /// `currentTheme`; `None` = no opinion.
    pub theme: Option<Theme>,
    /// `mode`; `None` = no opinion.
    pub mode: Option<ModePref>,
    /// `endpoints`; empty = no opinion (every endpoint from the profile).
    pub endpoints: EndpointsSection,
    /// `updates`; empty = every app updates itself on the stable channel.
    pub updates: UpdatesSection,
    /// Every other key, as it was read.
    rest: Map<String, Value>,
}

impl AzlinConfig {
    /// Reads the config. Never fails: what cannot be read is no opinion, and
    /// the second value says what was wrong (for a log line), if anything.
    #[must_use]
    pub fn parse(json: &str) -> (AzlinConfig, Option<String>) {
        if json.trim().is_empty() {
            return (AzlinConfig::default(), None);
        }
        let mut rest = match serde_json::from_str::<Value>(json) {
            Ok(Value::Object(map)) => map,
            Ok(_) => {
                return (
                    AzlinConfig::default(),
                    Some("not a config: the top level is not an object".to_string()),
                )
            }
            Err(e) => return (AzlinConfig::default(), Some(format!("not a config: {e}"))),
        };
        let mut problems = Vec::new();
        let theme = match rest.remove(THEME_KEY) {
            Some(Value::String(name)) => Theme::parse(&name).or_else(|| {
                problems.push(format!("unknown theme {name:?}"));
                None
            }),
            Some(other) => {
                problems.push(format!("{THEME_KEY} is not a name: {other}"));
                None
            }
            None => None,
        };
        let mode = match rest.remove(MODE_KEY) {
            Some(Value::String(name)) => ModePref::parse(&name).or_else(|| {
                problems.push(format!("unknown mode {name:?}"));
                None
            }),
            Some(other) => {
                problems.push(format!("{MODE_KEY} is not a name: {other}"));
                None
            }
            None => None,
        };
        let endpoints = match rest.remove(ENDPOINTS_KEY) {
            Some(Value::Object(map)) => EndpointsSection::parse(map, &mut problems),
            Some(other) => {
                // Kept as it was: writing the file back must not lose it.
                problems.push(format!("{ENDPOINTS_KEY} is not an object: {other}"));
                rest.insert(ENDPOINTS_KEY.to_string(), other);
                EndpointsSection::default()
            }
            None => EndpointsSection::default(),
        };
        let updates = match rest.remove(UPDATES_KEY) {
            Some(Value::Object(map)) => UpdatesSection::parse(map, &mut problems),
            Some(other) => {
                problems.push(format!("{UPDATES_KEY} is not an object: {other}"));
                rest.insert(UPDATES_KEY.to_string(), other);
                UpdatesSection::default()
            }
            None => UpdatesSection::default(),
        };
        let problem = (!problems.is_empty()).then(|| problems.join("; "));
        (
            AzlinConfig {
                theme,
                mode,
                endpoints,
                updates,
                rest,
            },
            problem,
        )
    }

    /// The file's text: pretty-printed JSON with sorted keys and a final
    /// newline (`serde_json`'s map is ordered by key).
    #[must_use]
    pub fn to_json(&self) -> String {
        let mut map = self.rest.clone();
        if let Some(theme) = self.theme {
            map.insert(THEME_KEY.to_string(), Value::String(theme.name().to_string()));
        }
        if let Some(mode) = self.mode {
            map.insert(MODE_KEY.to_string(), Value::String(mode.name().to_string()));
        }
        if let Some(endpoints) = self.endpoints.to_value() {
            map.insert(ENDPOINTS_KEY.to_string(), endpoints);
        }
        if let Some(updates) = self.updates.to_value() {
            map.insert(UPDATES_KEY.to_string(), updates);
        }
        let mut text =
            serde_json::to_string_pretty(&Value::Object(map)).unwrap_or_else(|_| "{}".to_string());
        text.push('\n');
        text
    }

    /// Reads the config at `path`: a missing file is no opinion and no
    /// problem.
    #[must_use]
    pub fn load(path: &Path) -> (AzlinConfig, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(text) => AzlinConfig::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (AzlinConfig::default(), None),
            Err(e) => (AzlinConfig::default(), Some(format!("unreadable: {e}"))),
        }
    }

    /// Writes the config to `path`, creating its folder: through a temporary
    /// file beside it and a rename.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped the write.
    pub fn store(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, self.to_json())?;
        std::fs::rename(&tmp, path)
    }

    /// The config at `path` with `theme` (unless `None`: a pinned app leaves
    /// the theme as it is) and `mode` written into it, every other key kept.
    /// What the file says now is read first, so two apps writing in turn
    /// never undo each other's other keys.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped the write.
    pub fn update(path: &Path, theme: Option<Theme>, mode: ModePref) -> std::io::Result<()> {
        let (mut config, _problem) = AzlinConfig::load(path);
        let before = config.clone();
        if theme.is_some() {
            config.theme = theme;
        }
        config.mode = Some(mode);
        if config == before && path.exists() {
            return Ok(());
        }
        config.store(path)
    }

    /// The config at `path` with `change` made to it, every other key kept;
    /// written only when something changed. What the file says now is read
    /// first, so the manager and an app writing in turn never undo each
    /// other. A file that is no JSON object is left alone: writing would
    /// lose what the user typed into it.
    ///
    /// # Errors
    ///
    /// The I/O error that stopped the write, or `InvalidData` for a file
    /// that is not a config.
    pub fn edit(
        path: &Path,
        change: impl FnOnce(&mut AzlinConfig),
    ) -> std::io::Result<AzlinConfig> {
        if let Ok(text) = std::fs::read_to_string(path) {
            let is_object = text.trim().is_empty()
                || matches!(serde_json::from_str::<Value>(&text), Ok(Value::Object(_)));
            if !is_object {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("{} is not a config; left as it is", path.display()),
                ));
            }
        }
        let (mut config, _problem) = AzlinConfig::load(path);
        let before = config.clone();
        change(&mut config);
        if config != before || !path.exists() {
            config.store(path)?;
        }
        Ok(config)
    }
}

// ---------------------------------------------------------------------------
// Endpoints: where the Azlin services are
// ---------------------------------------------------------------------------

/// The key of the endpoints section.
pub const ENDPOINTS_KEY: &str = "endpoints";
/// The key of the profile in the section.
pub const PROFILE_KEY: &str = "profile";
/// The variable naming the profile for a run (over the file's).
pub const PROFILE_VAR: &str = "AZLIN_PROFILE";
/// The flag naming the profile for a run (over the variable).
pub const PROFILE_FLAG: &str = "--profile";
/// The keys of this file that never leave this computer. The sync of the
/// `.azlin` folder uploads the file without them and keeps this computer's
/// own when it writes a downloaded copy: an endpoint read from the bucket
/// would let whoever can write the bucket send this computer's credentials
/// to a server of their choice, and a developer's local cluster must not
/// follow the user to the laptop.
pub const MACHINE_LOCAL_KEYS: &[&str] = &[ENDPOINTS_KEY, UPDATES_KEY];
/// The profile when nothing names one: the local mocked services. A real
/// service is chosen on purpose (`"profile": "production"`), so a test that
/// forgot its config never signs up against it.
pub const DEFAULT_PROFILE: Profile = Profile::Local;
/// The longest address taken (AzMeet's limit for its meeting server).
const MAX_URL_CHARS: usize = 2048;

/// A service the apps talk to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Endpoint {
    /// The token server: signup, credentials and the node list, lockdown,
    /// restore.
    Token,
    /// The S3 endpoint of the drive. Normally the token server hands it out
    /// with the drive; a value here overrides that one (a port forward, a VM
    /// whose block address is not reachable from this computer).
    S3,
    /// The meeting server (AzMeet's `meet` Worker).
    Meet,
    /// The iroh relay (AzMeet's calls, "S3 over iroh"): `off`, `default`
    /// (n0's public relays) or a relay's address.
    Relay,
}

impl Endpoint {
    /// Every endpoint, in the order the config shows them (and the order of
    /// [`EffectiveEndpoints`]' values).
    pub const ALL: [Endpoint; 4] = [
        Endpoint::Token,
        Endpoint::S3,
        Endpoint::Meet,
        Endpoint::Relay,
    ];

    /// Its key in the `endpoints` section.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Endpoint::Token => "token",
            Endpoint::S3 => "s3",
            Endpoint::Meet => "meet",
            Endpoint::Relay => "relay",
        }
    }

    /// The environment variables that set it for a run, the first one
    /// winning over the later ones. They are the names the tools already
    /// use, so one variable sets every program: azlin-client's tests and
    /// `azctl test client` say `AZLIN_TOKEN_URL`, `azctl test gui` says
    /// `AZLIN_TOKEN_SERVER`, AzMeet reads `AZMEET_WORKER` and `AZMEET_RELAY`.
    #[must_use]
    pub fn vars(self) -> &'static [&'static str] {
        match self {
            Endpoint::Token => &["AZLIN_TOKEN_URL", "AZLIN_TOKEN_SERVER"],
            Endpoint::S3 => &["AZLIN_S3_URL"],
            Endpoint::Meet => &["AZMEET_WORKER"],
            Endpoint::Relay => &["AZMEET_RELAY"],
        }
    }

    /// The command-line flag that sets it for a run (`azcloud`).
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Endpoint::Token => "--token-url",
            Endpoint::S3 => "--s3-url",
            Endpoint::Meet => "--meet-url",
            Endpoint::Relay => "--relay",
        }
    }

    /// What it is, for `azcloud config`.
    #[must_use]
    pub fn what(self) -> &'static str {
        match self {
            Endpoint::Token => "token server (signup, credentials, node list)",
            Endpoint::S3 => "S3 endpoint (else the one the token server hands out)",
            Endpoint::Meet => "meeting server (AzMeet's meet Worker)",
            Endpoint::Relay => "iroh relay (off, default = n0's, azlin = Azlin's, or an address)",
        }
    }
}

/// A built-in set of addresses: the bottom layer of every endpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    /// The local mocked services of azul-apps' `iso` (`azctl dev up`).
    Local,
    /// The trial environment.
    Trial,
    /// The live service.
    Production,
}

impl Profile {
    /// Every profile.
    pub const ALL: [Profile; 3] = [Profile::Local, Profile::Trial, Profile::Production];

    /// Its name in the file and the variable.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Profile::Local => "local",
            Profile::Trial => "trial",
            Profile::Production => "production",
        }
    }

    /// The profile of a name (any case; `dev` is `local`, `prod` and `live`
    /// are `production`).
    #[must_use]
    pub fn parse(name: &str) -> Option<Profile> {
        match name.trim().to_ascii_lowercase().as_str() {
            "local" | "dev" => Some(Profile::Local),
            "trial" => Some(Profile::Trial),
            "production" | "prod" | "live" => Some(Profile::Production),
            _ => None,
        }
    }

    /// The names, for a message: `local, trial, production`.
    #[must_use]
    pub fn names() -> String {
        Profile::ALL.map(Profile::name).join(", ")
    }

    /// What the profile says of `endpoint` when the file, the environment
    /// and the flags say nothing; `None` = nothing (the token server's drive
    /// bundle names the S3 endpoint, and no meeting server is deployed yet).
    ///
    /// The local addresses are the ports of the mocked services: `azctl dev
    /// up` binds the token server on 8081 and the S3 load balancer on 9000
    /// (azul-apps `iso/crates/azctl/src/ctl/dev.rs`, which the bundle then
    /// names), the meet Worker runs under `wrangler dev --port 8790`. The trial
    /// and live token servers are the Workers' routes in
    /// `iso/azworker/token/wrangler.toml`.
    ///
    /// The relay is n0's public relays (`default`) for every profile: Azlin's
    /// own ([`AZLIN_RELAY`]) is not hosted yet, so a profile that named it
    /// would leave every relayed connection without a relay. It stays a
    /// choice by its word and its address. The local stack names its own
    /// `iroh-relay --dev` in its profile's file (azul-apps `local/mkprofile.py`).
    #[must_use]
    pub fn default_for(self, endpoint: Endpoint) -> Option<&'static str> {
        match (self, endpoint) {
            (Profile::Local, Endpoint::Token) => Some("http://127.0.0.1:8081"),
            (Profile::Local, Endpoint::Meet) => Some("http://127.0.0.1:8790"),
            (Profile::Trial, Endpoint::Token) => Some("https://token-trial.azlin.io"),
            (Profile::Production, Endpoint::Token) => Some("https://token.azlin.io"),
            (_, Endpoint::Relay) => Some("default"),
            _ => None,
        }
    }
}

/// A service address as typed: trimmed, without trailing slashes. The error
/// says why it is none: it must be an http or https address with a host,
/// printable ASCII without spaces, a query or a fragment, at most 2048
/// characters (AzMeet's rule for its meeting server).
///
/// # Errors
///
/// Why the text is no service address.
pub fn normalize_url(input: &str) -> Result<String, String> {
    let s = input.trim().trim_end_matches('/');
    if s.is_empty() {
        return Err(String::from("it is empty"));
    }
    if s.len() > MAX_URL_CHARS {
        return Err(format!("it is longer than {MAX_URL_CHARS} characters"));
    }
    if !s.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(String::from("it has a space or a character outside ASCII"));
    }
    if s.contains(['?', '#']) {
        return Err(String::from("it has a query or a fragment"));
    }
    let lower = s.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"))
        .ok_or_else(|| String::from("it does not start with http:// or https://"))?;
    let host = rest.split('/').next().unwrap_or("");
    if host.is_empty() || host.starts_with(':') {
        return Err(String::from("it has no host"));
    }
    Ok(s.to_string())
}

/// Azlin's own iroh relay: what the relay setting's word `azlin` stands for.
/// No profile names it until it is hosted.
pub const AZLIN_RELAY: &str = "https://connect.azlin.io";

/// A relay setting: `off` (also `0`), `default` (n0's public relays; also
/// `1`), `azlin` (read as [`AZLIN_RELAY`], so every client that takes an
/// address takes it) or a relay's address - AzMeet's values for
/// `AZMEET_RELAY`.
///
/// # Errors
///
/// Why the text is none of them.
pub fn normalize_relay(input: &str) -> Result<String, String> {
    let s = input.trim();
    if s.eq_ignore_ascii_case("off") || s == "0" {
        return Ok(String::from("off"));
    }
    if s.eq_ignore_ascii_case("default") || s == "1" {
        return Ok(String::from("default"));
    }
    if s.eq_ignore_ascii_case("azlin") {
        return Ok(String::from(AZLIN_RELAY));
    }
    normalize_url(s)
}

/// The value of `endpoint` as typed, checked ([`normalize_url`], for the relay
/// [`normalize_relay`]).
///
/// # Errors
///
/// Why the text is no value for it.
pub fn normalize_endpoint(endpoint: Endpoint, input: &str) -> Result<String, String> {
    match endpoint {
        Endpoint::Relay => normalize_relay(input),
        _ => normalize_url(input),
    }
}

/// The `endpoints` section of the file. Every value is optional: what it does
/// not say comes from the profile. A value that is no address is reported
/// when the file is read and kept under its key as it was, so writing the
/// file back never loses what the user typed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EndpointsSection {
    /// `profile`; `None` = no opinion.
    pub profile: Option<Profile>,
    token: Option<String>,
    s3: Option<String>,
    meet: Option<String>,
    relay: Option<String>,
    /// Every other key of the section, and the values that could not be
    /// used, as they were read.
    rest: Map<String, Value>,
}

impl EndpointsSection {
    /// The address the file gives `endpoint`, normalized; `None` = no
    /// opinion.
    #[must_use]
    pub fn get(&self, endpoint: Endpoint) -> Option<&str> {
        match endpoint {
            Endpoint::Token => self.token.as_deref(),
            Endpoint::S3 => self.s3.as_deref(),
            Endpoint::Meet => self.meet.as_deref(),
            Endpoint::Relay => self.relay.as_deref(),
        }
    }

    /// The address of the service `name` (`token`, `tiles`): a known endpoint's checked value
    /// ([`EndpointsSection::get`]), another key's string as written (trimmed); `None` = no
    /// opinion.
    #[must_use]
    pub fn named(&self, name: &str) -> Option<&str> {
        if let Some(endpoint) = Endpoint::ALL.iter().find(|e| e.key() == name) {
            return self.get(*endpoint);
        }
        self.rest
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
    }

    /// Sets (with `None`: clears) the address of `endpoint`; a value the file
    /// held that could not be used goes too.
    ///
    /// # Errors
    ///
    /// Why `value` is no value for it ([`normalize_endpoint`]); nothing
    /// changes then.
    pub fn set(&mut self, endpoint: Endpoint, value: Option<&str>) -> Result<(), String> {
        let value = match value {
            Some(v) => Some(normalize_endpoint(endpoint, v)?),
            None => None,
        };
        self.rest.remove(endpoint.key());
        *self.slot(endpoint) = value;
        Ok(())
    }

    /// Whether the section says nothing at all (it is not written then).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.profile.is_none()
            && Endpoint::ALL.iter().all(|e| self.get(*e).is_none())
            && self.rest.is_empty()
    }

    fn slot(&mut self, endpoint: Endpoint) -> &mut Option<String> {
        match endpoint {
            Endpoint::Token => &mut self.token,
            Endpoint::S3 => &mut self.s3,
            Endpoint::Meet => &mut self.meet,
            Endpoint::Relay => &mut self.relay,
        }
    }

    /// The section of the file's object `map`; what is wrong goes to
    /// `problems`.
    fn parse(mut map: Map<String, Value>, problems: &mut Vec<String>) -> EndpointsSection {
        let mut section = EndpointsSection::default();
        match map.remove(PROFILE_KEY) {
            Some(Value::String(name)) => match Profile::parse(&name) {
                Some(profile) => section.profile = Some(profile),
                None => {
                    problems.push(format!(
                        "unknown profile {name:?} (known: {})",
                        Profile::names()
                    ));
                    section
                        .rest
                        .insert(PROFILE_KEY.to_string(), Value::String(name));
                }
            },
            Some(Value::Null) | None => {}
            Some(other) => {
                problems.push(format!(
                    "{ENDPOINTS_KEY}.{PROFILE_KEY} is not a name: {other}"
                ));
                section.rest.insert(PROFILE_KEY.to_string(), other);
            }
        }
        for endpoint in Endpoint::ALL {
            let key = endpoint.key();
            match map.remove(key) {
                Some(Value::String(raw)) => match normalize_endpoint(endpoint, &raw) {
                    Ok(value) => *section.slot(endpoint) = Some(value),
                    Err(why) => {
                        problems.push(format!("{ENDPOINTS_KEY}.{key} {raw:?}: {why}"));
                        section.rest.insert(key.to_string(), Value::String(raw));
                    }
                },
                Some(Value::Null) | None => {}
                Some(other) => {
                    problems.push(format!("{ENDPOINTS_KEY}.{key} is not an address: {other}"));
                    section.rest.insert(key.to_string(), other);
                }
            }
        }
        section.rest.extend(map);
        section
    }

    /// The section as the file's value; `None` when it says nothing.
    fn to_value(&self) -> Option<Value> {
        if self.is_empty() {
            return None;
        }
        let mut map = self.rest.clone();
        if let Some(profile) = self.profile {
            map.insert(
                PROFILE_KEY.to_string(),
                Value::String(profile.name().to_string()),
            );
        }
        for endpoint in Endpoint::ALL {
            if let Some(value) = self.get(endpoint) {
                map.insert(endpoint.key().to_string(), Value::String(value.to_string()));
            }
        }
        Some(Value::Object(map))
    }
}

// ---------------------------------------------------------------------------
// Updates: how the Azlin apps stay current
// ---------------------------------------------------------------------------

/// The key of the updates section.
pub const UPDATES_KEY: &str = "updates";

/// The release channel an Azlin app follows. There is no nightly: an app is
/// offered as stable releases or as betas, which testers take the moment
/// they are published.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    #[default]
    Stable,
    Beta,
}

impl Channel {
    /// Every channel, stable first (the choice a settings page offers).
    pub const ALL: [Channel; 2] = [Channel::Stable, Channel::Beta];

    /// The name the config and the manifests use.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Beta => "beta",
        }
    }

    /// The channel of `name` (any case); `None` for anything else, nightly
    /// included.
    #[must_use]
    pub fn parse(name: &str) -> Option<Channel> {
        Channel::ALL
            .into_iter()
            .find(|c| c.name().eq_ignore_ascii_case(name.trim()))
    }
}

/// What one app does about updates: the shared choice with the app's own
/// laid over it ([`UpdatesSection::policy_for`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdatePolicy {
    /// Updates are fetched and verified in the background and the app shows
    /// its banner; `false`: only when the user asks in the manager.
    pub autoupdate: bool,
    /// The channel the app's manifest is read for.
    pub channel: Channel,
    /// The version the user does not want (the banner's "Skip this version",
    /// a revert away from it): neither offered nor staged; a newer one is.
    pub skip_version: Option<String>,
}

/// The Azlin apps that were renamed: `(the app's key now, the key it had)`. What was kept under
/// the old key - the update choices here, an installed copy's registration, an "Update now" of the
/// old program - belongs to the app under its new one.
pub const RENAMED_APPS: [(&str, &str); 1] = [("azpasswordmanager", "azkeys")];

/// The key the app `key` has now: its new one if it was renamed, else `key` itself.
#[must_use]
pub fn current_app_key(key: &str) -> &str {
    RENAMED_APPS
        .iter()
        .find(|(_, former)| *former == key)
        .map_or(key, |(now, _)| *now)
}

/// The key the app `key` had before it was renamed, if it was.
#[must_use]
pub fn former_app_key(key: &str) -> Option<&'static str> {
    RENAMED_APPS
        .iter()
        .find(|(now, _)| *now == key)
        .map(|(_, former)| *former)
}

/// Moves the choices kept under a renamed app's former key to its key now (unless the file has
/// choices under the new key too: those were made later).
fn adopt_renamed_overrides(overrides: &mut Map<String, Value>) {
    for (now, former) in RENAMED_APPS {
        if overrides.contains_key(now) {
            continue;
        }
        if let Some(choices) = overrides.remove(former) {
            overrides.insert(now.to_string(), choices);
        }
    }
}

/// The `updates` section. Every field is `None` when the file does not say.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UpdatesSection {
    /// `autoupdate`, for every app without its own choice (`None`: on).
    pub autoupdate: Option<bool>,
    /// `channel`, for every app without its own (`None`: stable).
    pub channel: Option<Channel>,
    /// `cohort`: the machine's rollout bucket (0-99). AzInstallManager draws
    /// it once and passes it into every check, so this computer is in the
    /// same cohort for every app and every release.
    pub cohort: Option<u8>,
    /// `install_dir`: where AzInstallManager puts the apps (`None`:
    /// `~/.azlin/apps`).
    pub install_dir: Option<String>,
    /// `manifest_url`: where an app's update manifest is, with `{app}` (the
    /// app's key, `azwriter`) and `{channel}` filled in. `None`: no source
    /// is configured and nothing can be checked.
    pub manifest_url: Option<String>,
    /// `root_public_key`: the minisign root key the manifests' signature
    /// chain must end in; `None` or empty: the digest pin only.
    pub root_public_key: Option<String>,
    /// `maintenance_window`: when unattended update work may run, azul's
    /// RRULE subset (`None`: any time).
    pub maintenance_window: Option<String>,
    /// `overrides`: each app's own choices by the app's key, as read (the
    /// keys this module does not know stay with them).
    overrides: Map<String, Value>,
    /// Every other key of the section, as it was read.
    rest: Map<String, Value>,
}

/// An override's key of an app's auto-update choice.
const AUTOUPDATE_KEY: &str = "autoupdate";
/// An override's key of an app's channel.
const CHANNEL_KEY: &str = "channel";
/// An override's key of the version an app skips.
const SKIP_KEY: &str = "skip_version";

impl UpdatesSection {
    /// The section of the file's object `map`; what is wrong goes to
    /// `problems` and stays in the section as it was.
    fn parse(mut map: Map<String, Value>, problems: &mut Vec<String>) -> UpdatesSection {
        let mut section = UpdatesSection::default();
        match map.remove(AUTOUPDATE_KEY) {
            Some(Value::Bool(on)) => section.autoupdate = Some(on),
            Some(Value::Null) | None => {}
            Some(other) => {
                problems.push(format!("{UPDATES_KEY}.{AUTOUPDATE_KEY} is not true or false: {other}"));
                section.rest.insert(AUTOUPDATE_KEY.to_string(), other);
            }
        }
        match map.remove(CHANNEL_KEY) {
            Some(Value::String(name)) => match Channel::parse(&name) {
                Some(channel) => section.channel = Some(channel),
                None => {
                    problems.push(format!(
                        "unknown channel {name:?} (known: stable, beta)"
                    ));
                    section.rest.insert(CHANNEL_KEY.to_string(), Value::String(name));
                }
            },
            Some(Value::Null) | None => {}
            Some(other) => {
                problems.push(format!("{UPDATES_KEY}.{CHANNEL_KEY} is not a name: {other}"));
                section.rest.insert(CHANNEL_KEY.to_string(), other);
            }
        }
        match map.remove("cohort") {
            Some(Value::Number(n)) if n.as_u64().is_some_and(|b| b < 100) => {
                section.cohort = n.as_u64().and_then(|b| u8::try_from(b).ok());
            }
            Some(Value::Null) | None => {}
            Some(other) => {
                problems.push(format!("{UPDATES_KEY}.cohort is not 0 to 99: {other}"));
                section.rest.insert("cohort".to_string(), other);
            }
        }
        for (key, slot) in [
            ("install_dir", &mut section.install_dir),
            ("manifest_url", &mut section.manifest_url),
            ("root_public_key", &mut section.root_public_key),
            ("maintenance_window", &mut section.maintenance_window),
        ] {
            match map.remove(key) {
                Some(Value::String(text)) => *slot = Some(text),
                Some(Value::Null) | None => {}
                Some(other) => {
                    problems.push(format!("{UPDATES_KEY}.{key} is not text: {other}"));
                    section.rest.insert(key.to_string(), other);
                }
            }
        }
        match map.remove("overrides") {
            Some(Value::Object(mut overrides)) => {
                adopt_renamed_overrides(&mut overrides);
                section.overrides = overrides;
            }
            Some(Value::Null) | None => {}
            Some(other) => {
                problems.push(format!("{UPDATES_KEY}.overrides is not an object: {other}"));
                section.rest.insert("overrides".to_string(), other);
            }
        }
        section.rest.extend(map);
        section
    }

    /// The section as the file's value; `None` when it says nothing.
    fn to_value(&self) -> Option<Value> {
        if self.is_empty() {
            return None;
        }
        let mut map = self.rest.clone();
        if let Some(on) = self.autoupdate {
            map.insert(AUTOUPDATE_KEY.to_string(), Value::Bool(on));
        }
        if let Some(channel) = self.channel {
            map.insert(CHANNEL_KEY.to_string(), Value::String(channel.name().to_string()));
        }
        if let Some(cohort) = self.cohort {
            map.insert("cohort".to_string(), Value::from(cohort));
        }
        for (key, value) in [
            ("install_dir", &self.install_dir),
            ("manifest_url", &self.manifest_url),
            ("root_public_key", &self.root_public_key),
            ("maintenance_window", &self.maintenance_window),
        ] {
            if let Some(text) = value {
                map.insert(key.to_string(), Value::String(text.clone()));
            }
        }
        if !self.overrides.is_empty() {
            map.insert("overrides".to_string(), Value::Object(self.overrides.clone()));
        }
        Some(Value::Object(map))
    }

    /// Whether the section says nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == UpdatesSection::default()
    }

    /// What the app `app` (its key, `azwriter`) does: its own choices where
    /// it has them, else the shared ones, else auto-update on the stable
    /// channel.
    #[must_use]
    pub fn policy_for(&self, app: &str) -> UpdatePolicy {
        let own = self.overrides.get(app).and_then(Value::as_object);
        let own_value = |key: &str| own.and_then(|o| o.get(key));
        UpdatePolicy {
            autoupdate: own_value(AUTOUPDATE_KEY)
                .and_then(Value::as_bool)
                .or(self.autoupdate)
                .unwrap_or(true),
            channel: own_value(CHANNEL_KEY)
                .and_then(Value::as_str)
                .and_then(Channel::parse)
                .or(self.channel)
                .unwrap_or_default(),
            skip_version: own_value(SKIP_KEY)
                .and_then(Value::as_str)
                .filter(|v| !v.trim().is_empty())
                .map(str::to_string),
        }
    }

    /// Sets auto-update for every app (`None`) or for the app `app`.
    pub fn set_autoupdate(&mut self, app: Option<&str>, on: bool) {
        match app {
            None => self.autoupdate = Some(on),
            Some(app) => {
                self.own_mut(app)
                    .insert(AUTOUPDATE_KEY.to_string(), Value::Bool(on));
            }
        }
    }

    /// Sets the channel of every app (`None`) or of the app `app`.
    pub fn set_channel(&mut self, app: Option<&str>, channel: Channel) {
        match app {
            None => self.channel = Some(channel),
            Some(app) => {
                self.own_mut(app).insert(
                    CHANNEL_KEY.to_string(),
                    Value::String(channel.name().to_string()),
                );
            }
        }
    }

    /// Sets the version the app `app` skips (`None`: none).
    pub fn set_skip_version(&mut self, app: &str, version: Option<&str>) {
        match version {
            Some(v) => {
                self.own_mut(app)
                    .insert(SKIP_KEY.to_string(), Value::String(v.to_string()));
            }
            None => {
                if let Some(Value::Object(own)) = self.overrides.get_mut(app) {
                    own.remove(SKIP_KEY);
                    if own.is_empty() {
                        self.overrides.remove(app);
                    }
                }
            }
        }
    }

    /// The app's own choices, made an object when they were none (or not one).
    fn own_mut(&mut self, app: &str) -> &mut Map<String, Value> {
        let entry = self
            .overrides
            .entry(app.to_string())
            .or_insert_with(|| Value::Object(Map::new()));
        if !entry.is_object() {
            *entry = Value::Object(Map::new());
        }
        entry
            .as_object_mut()
            .expect("an app's choices were made an object above")
    }
}

/// Where a setting's value came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// Nothing gave it a value.
    Unset,
    /// Built in: the default profile, when nothing names one.
    BuiltIn,
    /// Built into this profile.
    Profile(Profile),
    /// The shared config file at this path.
    File(PathBuf),
    /// This environment variable.
    Env(&'static str),
    /// This command-line flag.
    Flag(&'static str),
}

impl Source {
    /// One word for scripts: `unset`, `built-in`, `profile`, `file`, `env`,
    /// `flag`.
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Source::Unset => "unset",
            Source::BuiltIn => "built-in",
            Source::Profile(_) => "profile",
            Source::File(_) => "file",
            Source::Env(_) => "env",
            Source::Flag(_) => "flag",
        }
    }

    /// For people: `environment AZLIN_TOKEN_URL`, `config file /x/config.json`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Source::Unset => String::from("unset"),
            Source::BuiltIn => String::from("built-in default"),
            Source::Profile(profile) => format!("profile {}", profile.name()),
            Source::File(path) => format!("config file {}", path.display()),
            Source::Env(var) => format!("environment {var}"),
            Source::Flag(flag) => format!("flag {flag}"),
        }
    }
}

/// One setting after every layer: its value, where it came from, what it
/// overrode and what a layer offered that could not be used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The value; `None` = nothing set it.
    pub value: Option<String>,
    /// Where `value` came from ([`Source::Unset`] without one).
    pub source: Source,
    /// The values of the lower layers it overrode, the closest first.
    pub overridden: Vec<(Source, String)>,
    /// What a layer said that is no value for it, and why; the layers below
    /// stayed in force.
    pub rejected: Vec<(Source, String, String)>,
}

impl Resolved {
    /// Nothing yet.
    #[must_use]
    pub fn unset() -> Resolved {
        Resolved {
            value: None,
            source: Source::Unset,
            overridden: Vec::new(),
            rejected: Vec::new(),
        }
    }

    /// Applies the next layer up: `raw` from `source`, checked by `check`. A
    /// usable value replaces the one so far (which moves to `overridden`); a
    /// blank one counts as unset; any other is rejected.
    pub fn offer(
        &mut self,
        source: Source,
        raw: &str,
        check: impl Fn(&str) -> Result<String, String>,
    ) {
        let raw = raw.trim();
        if raw.is_empty() {
            return;
        }
        match check(raw) {
            Ok(value) => {
                if let Some(old) = self.value.take() {
                    let old_source = std::mem::replace(&mut self.source, Source::Unset);
                    self.overridden.insert(0, (old_source, old));
                }
                self.value = Some(value);
                self.source = source;
            }
            Err(why) => self.rejected.push((source, raw.to_string(), why)),
        }
    }
}

/// What the command line said (`None` = the flag was not given): the top
/// layer.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EndpointFlags {
    /// `--profile`.
    pub profile: Option<String>,
    /// `--token-url`.
    pub token: Option<String>,
    /// `--s3-url`.
    pub s3: Option<String>,
    /// `--meet-url`.
    pub meet: Option<String>,
    /// `--relay`.
    pub relay: Option<String>,
}

impl EndpointFlags {
    /// The flag's value for `endpoint`.
    #[must_use]
    pub fn get(&self, endpoint: Endpoint) -> Option<&str> {
        match endpoint {
            Endpoint::Token => self.token.as_deref(),
            Endpoint::S3 => self.s3.as_deref(),
            Endpoint::Meet => self.meet.as_deref(),
            Endpoint::Relay => self.relay.as_deref(),
        }
    }
}

/// Every endpoint after the layers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EffectiveEndpoints {
    /// The profile whose addresses are the bottom layer.
    pub profile: Profile,
    /// Where the profile came from (its value is the profile's name).
    pub profile_setting: Resolved,
    /// One per endpoint, in [`Endpoint::ALL`]'s order.
    endpoints: [Resolved; 4],
}

impl EffectiveEndpoints {
    /// The setting of `endpoint`.
    #[must_use]
    pub fn get(&self, endpoint: Endpoint) -> &Resolved {
        &self.endpoints[endpoint as usize]
    }

    /// The value of `endpoint`; `None` when nothing set it.
    #[must_use]
    pub fn url(&self, endpoint: Endpoint) -> Option<&str> {
        self.get(endpoint).value.as_deref()
    }
}

/// Resolves every endpoint, the lowest layer first: the profile's built-in
/// address, the config file's section (`file`: its path and section), the
/// environment (`env` reads a variable; of an endpoint's [`Endpoint::vars`]
/// the first wins over the later ones), the command line (`flags`). The
/// profile goes through the same layers before (built-in
/// [`DEFAULT_PROFILE`], the file, [`PROFILE_VAR`], [`PROFILE_FLAG`]): it
/// decides the bottom layer of the rest. Pure, so the tests bring their own
/// environment.
#[must_use]
pub fn resolve_endpoints(
    file: Option<(&Path, &EndpointsSection)>,
    env: &dyn Fn(&str) -> Option<String>,
    flags: &EndpointFlags,
) -> EffectiveEndpoints {
    let check_profile = |raw: &str| {
        Profile::parse(raw)
            .map(|p| p.name().to_string())
            .ok_or_else(|| format!("unknown profile (known: {})", Profile::names()))
    };
    let mut profile_setting = Resolved::unset();
    profile_setting.offer(Source::BuiltIn, DEFAULT_PROFILE.name(), check_profile);
    if let Some((path, section)) = file {
        if let Some(profile) = section.profile {
            profile_setting.offer(
                Source::File(path.to_path_buf()),
                profile.name(),
                check_profile,
            );
        }
    }
    if let Some(raw) = env(PROFILE_VAR) {
        profile_setting.offer(Source::Env(PROFILE_VAR), &raw, check_profile);
    }
    if let Some(raw) = flags.profile.as_deref() {
        profile_setting.offer(Source::Flag(PROFILE_FLAG), raw, check_profile);
    }
    let profile = profile_setting
        .value
        .as_deref()
        .and_then(Profile::parse)
        .unwrap_or(DEFAULT_PROFILE);
    let endpoints = Endpoint::ALL.map(|endpoint| {
        let check = move |raw: &str| normalize_endpoint(endpoint, raw);
        let mut resolved = Resolved::unset();
        if let Some(default) = profile.default_for(endpoint) {
            resolved.offer(Source::Profile(profile), default, check);
        }
        if let Some((path, section)) = file {
            if let Some(value) = section.get(endpoint) {
                resolved.offer(Source::File(path.to_path_buf()), value, check);
            }
        }
        // The later names first, so the first name of the list wins.
        for &var in endpoint.vars().iter().rev() {
            if let Some(raw) = env(var) {
                resolved.offer(Source::Env(var), &raw, check);
            }
        }
        if let Some(raw) = flags.get(endpoint) {
            resolved.offer(Source::Flag(endpoint.flag()), raw, check);
        }
        resolved
    });
    EffectiveEndpoints {
        profile,
        profile_setting,
        endpoints,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_is_read_by_its_key_known_ones_checked_others_as_written() {
        let (config, _) = AzlinConfig::parse(
            r#"{"endpoints": {"token": "https://t.example/", "tiles": " http://127.0.0.1:8099/tiles/{z}/{x}/{y}.pbf ", "s3": "not an address"}}"#,
        );
        assert_eq!(config.endpoints.named("token"), Some("https://t.example"));
        assert_eq!(
            config.endpoints.named("tiles"),
            Some("http://127.0.0.1:8099/tiles/{z}/{x}/{y}.pbf")
        );
        assert_eq!(config.endpoints.named("s3"), None, "no address: reported, never used");
        assert_eq!(config.endpoints.named("meet"), None, "a service the section does not name");
    }

    #[test]
    fn the_config_lives_in_the_home_folder_unless_the_environment_says_otherwise() {
        let home = Path::new("/home/ann");
        assert_eq!(
            config_path(None, Some(home)),
            Some(PathBuf::from("/home/ann/.azlin/config.json"))
        );
        assert_eq!(
            config_path(Some("/tmp/c.json"), Some(home)),
            Some(PathBuf::from("/tmp/c.json"))
        );
        assert_eq!(config_path(Some("off"), Some(home)), None);
        assert_eq!(config_path(Some(" "), Some(home)), None);
        assert_eq!(config_path(None, None), None);
    }

    /// The relay may name Azlin's own with a word, as AzMeet's settings do: it reads as Azlin's
    /// relay's address, so every client that takes an address takes it. The meeting server stays
    /// an http(s) address.
    #[test]
    fn the_relay_may_be_azlins_by_name_and_the_meeting_server_is_an_http_address() {
        assert_eq!(AZLIN_RELAY, "https://connect.azlin.io");
        assert_eq!(normalize_endpoint(Endpoint::Relay, " Azlin ").as_deref(), Ok(AZLIN_RELAY));
        assert_eq!(normalize_endpoint(Endpoint::Relay, "default").as_deref(), Ok("default"));
        assert!(normalize_endpoint(Endpoint::Meet, "iroh://connect.azlin.io").is_err());
        assert!(normalize_endpoint(Endpoint::Meet, "azlin").is_err());
    }

    /// Every profile relays through n0's public relays until Azlin's own relay is hosted; Azlin's
    /// stays a choice by name (`azlin`) and by address.
    #[test]
    fn every_profile_relays_through_n0s_relays_until_azlins_is_hosted() {
        assert_eq!(Profile::Production.default_for(Endpoint::Relay), Some("default"));
        assert_eq!(Profile::Trial.default_for(Endpoint::Relay), Some("default"));
        assert_eq!(Profile::Local.default_for(Endpoint::Relay), Some("default"));
    }

    #[test]
    fn a_config_reads_the_theme_and_the_mode_and_keeps_every_other_key() {
        let (c, problem) = AzlinConfig::parse(
            r#"{"currentTheme": "flora:green", "mode": "dark", "locale": "de"}"#,
        );
        assert_eq!(problem, None);
        assert_eq!(c.theme, Some(Theme::FloraGreen));
        assert_eq!(c.mode, Some(ModePref::Dark));
        let text = c.to_json();
        let (again, _) = AzlinConfig::parse(&text);
        assert_eq!(again, c, "it round-trips");
        assert!(text.contains("\"locale\": \"de\""), "{text}");
    }

    #[test]
    fn an_unknown_theme_is_no_opinion_and_is_reported() {
        let (c, problem) = AzlinConfig::parse(r#"{"currentTheme": "monokai"}"#);
        assert_eq!(c.theme, None);
        assert!(problem.unwrap().contains("monokai"));
        let (c, problem) = AzlinConfig::parse("not json");
        assert_eq!(c, AzlinConfig::default());
        assert!(problem.is_some());
    }

    #[test]
    fn an_update_writes_the_look_and_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("azlin-config-test-{}", std::process::id()));
        let path = dir.join(".azlin").join("config.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"locale": "de"}"#).unwrap();
        AzlinConfig::update(&path, Some(Theme::FloraRed), ModePref::Light).unwrap();
        let (c, _) = AzlinConfig::load(&path);
        assert_eq!(c.theme, Some(Theme::FloraRed));
        assert_eq!(c.mode, Some(ModePref::Light));
        assert!(c.to_json().contains("locale"));
        // A pinned app writes the mode only.
        AzlinConfig::update(&path, None, ModePref::Dark).unwrap();
        let (c, _) = AzlinConfig::load(&path);
        assert_eq!(c.theme, Some(Theme::FloraRed), "the pinned app left the theme");
        assert_eq!(c.mode, Some(ModePref::Dark));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    #[test]
    fn without_any_config_every_endpoint_comes_from_the_local_profile() {
        let e = resolve_endpoints(None, &no_env, &EndpointFlags::default());
        assert_eq!(e.profile, Profile::Local);
        assert_eq!(e.profile_setting.source, Source::BuiltIn);
        assert_eq!(e.url(Endpoint::Token), Some("http://127.0.0.1:8081"));
        assert_eq!(
            e.get(Endpoint::Token).source,
            Source::Profile(Profile::Local)
        );
        assert_eq!(e.url(Endpoint::Meet), Some("http://127.0.0.1:8790"));
        assert_eq!(
            e.url(Endpoint::Relay),
            Some("default"),
            "the local profile relays through n0's public relays unless a relay is named"
        );
        assert_eq!(
            e.url(Endpoint::S3),
            None,
            "the token server's drive bundle names the S3 endpoint"
        );
        assert_eq!(e.get(Endpoint::S3).source, Source::Unset);
    }

    #[test]
    fn a_flag_beats_the_environment_which_beats_the_file_which_beats_the_profile() {
        let path = Path::new("/home/ann/.azlin/config.json");
        let mut section = EndpointsSection::default();
        section
            .set(Endpoint::Token, Some(" http://file.test:1/ "))
            .unwrap();
        let env = |name: &str| -> Option<String> {
            (name == "AZLIN_TOKEN_URL").then(|| String::from("http://env.test:2"))
        };

        let e = resolve_endpoints(Some((path, &section)), &no_env, &EndpointFlags::default());
        let token = e.get(Endpoint::Token);
        assert_eq!(token.value.as_deref(), Some("http://file.test:1"));
        assert_eq!(token.source, Source::File(path.to_path_buf()));
        assert_eq!(
            token.overridden,
            vec![(
                Source::Profile(Profile::Local),
                String::from("http://127.0.0.1:8081")
            )]
        );

        let e = resolve_endpoints(Some((path, &section)), &env, &EndpointFlags::default());
        assert_eq!(e.url(Endpoint::Token), Some("http://env.test:2"));
        assert_eq!(
            e.get(Endpoint::Token).source,
            Source::Env("AZLIN_TOKEN_URL")
        );

        let flags = EndpointFlags {
            token: Some(String::from("https://flag.test:3")),
            ..EndpointFlags::default()
        };
        let e = resolve_endpoints(Some((path, &section)), &env, &flags);
        let token = e.get(Endpoint::Token);
        assert_eq!(token.value.as_deref(), Some("https://flag.test:3"));
        assert_eq!(token.source, Source::Flag("--token-url"));
        let below: Vec<&str> = token.overridden.iter().map(|(s, _)| s.kind()).collect();
        assert_eq!(below, vec!["env", "file", "profile"], "the closest first");
    }

    #[test]
    fn the_token_servers_two_variable_names_both_count_and_the_first_one_wins() {
        let alias = |name: &str| -> Option<String> {
            (name == "AZLIN_TOKEN_SERVER").then(|| String::from("http://alias.test:1"))
        };
        let e = resolve_endpoints(None, &alias, &EndpointFlags::default());
        assert_eq!(e.url(Endpoint::Token), Some("http://alias.test:1"));
        assert_eq!(
            e.get(Endpoint::Token).source,
            Source::Env("AZLIN_TOKEN_SERVER")
        );

        let both = |name: &str| -> Option<String> {
            match name {
                "AZLIN_TOKEN_SERVER" => Some(String::from("http://alias.test:1")),
                "AZLIN_TOKEN_URL" => Some(String::from("http://canonical.test:2")),
                _ => None,
            }
        };
        let e = resolve_endpoints(None, &both, &EndpointFlags::default());
        let token = e.get(Endpoint::Token);
        assert_eq!(token.value.as_deref(), Some("http://canonical.test:2"));
        assert_eq!(
            token.overridden.first(),
            Some(&(
                Source::Env("AZLIN_TOKEN_SERVER"),
                String::from("http://alias.test:1")
            ))
        );
    }

    #[test]
    fn the_profile_goes_through_the_same_layers_and_decides_the_defaults() {
        let production = |name: &str| -> Option<String> {
            (name == PROFILE_VAR).then(|| String::from("production"))
        };
        let e = resolve_endpoints(None, &production, &EndpointFlags::default());
        assert_eq!(e.profile, Profile::Production);
        assert_eq!(e.profile_setting.source, Source::Env(PROFILE_VAR));
        assert_eq!(e.url(Endpoint::Token), Some("https://token.azlin.io"));
        assert_eq!(e.url(Endpoint::Relay), Some("default"), "n0's relays until Azlin's is hosted");
        assert_eq!(e.url(Endpoint::Meet), None, "no meeting server is deployed");

        // The file's own address stays over any profile's.
        let path = Path::new("/c.json");
        let mut section = EndpointsSection::default();
        section.profile = Some(Profile::Trial);
        section
            .set(Endpoint::Token, Some("http://127.0.0.1:18081"))
            .unwrap();
        let flags = EndpointFlags {
            profile: Some(String::from("prod")),
            ..EndpointFlags::default()
        };
        let e = resolve_endpoints(Some((path, &section)), &no_env, &flags);
        assert_eq!(e.profile, Profile::Production);
        assert_eq!(e.profile_setting.source, Source::Flag(PROFILE_FLAG));
        assert_eq!(e.url(Endpoint::Token), Some("http://127.0.0.1:18081"));

        // A misspelt profile is rejected and the layer below stays.
        let typo = |name: &str| -> Option<String> {
            (name == PROFILE_VAR).then(|| String::from("prodution"))
        };
        let e = resolve_endpoints(Some((path, &section)), &typo, &EndpointFlags::default());
        assert_eq!(e.profile, Profile::Trial);
        assert_eq!(e.profile_setting.rejected.len(), 1);
        assert_eq!(e.profile_setting.rejected[0].0, Source::Env(PROFILE_VAR));
    }

    #[test]
    fn a_blank_variable_is_unset_and_a_value_that_is_no_address_is_rejected() {
        let env = |name: &str| -> Option<String> {
            match name {
                "AZLIN_TOKEN_URL" => Some(String::from("   ")),
                "AZMEET_WORKER" => Some(String::from("127.0.0.1:8790")),
                _ => None,
            }
        };
        let e = resolve_endpoints(None, &env, &EndpointFlags::default());
        let token = e.get(Endpoint::Token);
        assert_eq!(token.source, Source::Profile(Profile::Local));
        assert!(
            token.rejected.is_empty(),
            "blank is unset: {:?}",
            token.rejected
        );
        let meet = e.get(Endpoint::Meet);
        assert_eq!(meet.value.as_deref(), Some("http://127.0.0.1:8790"));
        assert_eq!(meet.source, Source::Profile(Profile::Local));
        assert_eq!(meet.rejected.len(), 1, "no scheme: {:?}", meet.rejected);
        assert_eq!(meet.rejected[0].0, Source::Env("AZMEET_WORKER"));
    }

    #[test]
    fn an_address_needs_a_scheme_and_a_host_and_the_relay_also_takes_off_and_default() {
        assert_eq!(
            normalize_url(" https://token.azlin.io/ "),
            Ok(String::from("https://token.azlin.io"))
        );
        assert_eq!(
            normalize_url("http://127.0.0.1:9000/base"),
            Ok(String::from("http://127.0.0.1:9000/base"))
        );
        for bad in [
            "",
            "127.0.0.1:9000",
            "ftp://a.test",
            "https://",
            "https://:8787",
            "https://a.test/?x=1",
            "https://a.test/#top",
            "https://a b.test",
        ] {
            assert!(normalize_url(bad).is_err(), "{bad:?}");
        }
        assert_eq!(normalize_relay(" OFF "), Ok(String::from("off")));
        assert_eq!(normalize_relay("0"), Ok(String::from("off")));
        assert_eq!(normalize_relay("Default"), Ok(String::from("default")));
        assert_eq!(normalize_relay("1"), Ok(String::from("default")));
        assert_eq!(
            normalize_relay("http://127.0.0.1:3340/"),
            Ok(String::from("http://127.0.0.1:3340"))
        );
        assert!(normalize_relay("relay.example.com").is_err());
        assert!(
            normalize_endpoint(Endpoint::Token, "off").is_err(),
            "only the relay is ever off"
        );
    }

    #[test]
    fn the_endpoints_section_round_trips_and_keeps_what_it_could_not_use() {
        let (c, problem) = AzlinConfig::parse(
            r#"{"mode": "dark", "endpoints": {"profile": "local",
                "token": "http://127.0.0.1:18081/", "meet": "not an address",
                "turso": "http://127.0.0.1:8080"}}"#,
        );
        let problem = problem.expect("the meeting server is reported");
        assert!(problem.contains("endpoints.meet"), "{problem}");
        assert_eq!(c.endpoints.profile, Some(Profile::Local));
        assert_eq!(
            c.endpoints.get(Endpoint::Token),
            Some("http://127.0.0.1:18081")
        );
        assert_eq!(c.endpoints.get(Endpoint::Meet), None);
        let text = c.to_json();
        assert!(text.contains("\"turso\""), "a newer key stays: {text}");
        assert!(
            text.contains("not an address"),
            "what the user typed stays: {text}"
        );
        let (again, _) = AzlinConfig::parse(&text);
        assert_eq!(again, c, "it round-trips");

        let (plain, problem) = AzlinConfig::parse(r#"{"mode": "dark"}"#);
        assert_eq!(problem, None);
        assert!(plain.endpoints.is_empty());
        assert!(
            !plain.to_json().contains(ENDPOINTS_KEY),
            "no section is written when there is none"
        );
        let (odd, problem) = AzlinConfig::parse(r#"{"endpoints": "local"}"#);
        assert!(problem.is_some());
        assert!(odd.to_json().contains("\"endpoints\": \"local\""));
    }

    #[test]
    fn a_new_look_written_by_an_app_keeps_the_endpoints() {
        let dir = azul_storage::testing::TempDir::new("azlin-config-endpoints");
        let path = dir.path().join(CONFIG_DIR).join(CONFIG_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            r#"{"endpoints": {"profile": "trial", "s3": "http://127.0.0.1:9000"}}"#,
        )
        .unwrap();
        AzlinConfig::update(&path, Some(Theme::Flat), ModePref::Dark).unwrap();
        let (c, problem) = AzlinConfig::load(&path);
        assert_eq!(problem, None);
        assert_eq!(c.theme, Some(Theme::Flat));
        assert_eq!(c.endpoints.profile, Some(Profile::Trial));
        assert_eq!(c.endpoints.get(Endpoint::S3), Some("http://127.0.0.1:9000"));
    }

    #[test]
    fn only_the_endpoints_the_updates_and_the_trust_never_leave_this_computer() {
        assert_eq!(MACHINE_LOCAL_KEYS, &[ENDPOINTS_KEY, UPDATES_KEY, TRUST_KEY]);
        assert!(!MACHINE_LOCAL_KEYS.contains(&THEME_KEY));
        assert!(!MACHINE_LOCAL_KEYS.contains(&MODE_KEY));
    }

    #[test]
    fn the_trust_section_reads_its_roots_from_files_and_text_and_keeps_the_rest() {
        let dir = azul_storage::testing::TempDir::new("azlin-config-trust");
        let pem = "-----BEGIN CERTIFICATE-----\nAAAA\n-----END CERTIFICATE-----\n";
        std::fs::create_dir_all(dir.path().join("roots")).unwrap();
        std::fs::write(dir.path().join("roots/company.pem"), pem).unwrap();
        std::fs::write(dir.path().join("roots/emsign.pem"), pem.replace("AAAA", "BBBB")).unwrap();
        let json = format!(
            r#"{{"trust": {{"pinned_roots": ["roots/company.pem", {inline:?}],
                "distrusted_roots": ["~/emsign.pem"], "future": 1}}}}"#,
            inline = pem.replace("AAAA", "CCCC")
        );
        let (c, problem) = AzlinConfig::parse(&json);
        assert_eq!(problem, None);
        assert_eq!(c.trust.pinned_roots.len(), 2);
        let home = dir.path().join("roots");
        let pems = c.trust.pems(Some(dir.path()), Some(&home)).expect("readable");
        assert_eq!(
            pems.pinned,
            Some(vec![pem.to_string(), pem.replace("AAAA", "CCCC")])
        );
        assert_eq!(pems.distrusted, vec![pem.replace("AAAA", "BBBB")]);
        let (again, _) = AzlinConfig::parse(&c.to_json());
        assert_eq!(again, c, "written back as read, the unknown key kept");
    }

    #[cfg(feature = "trust")]
    #[test]
    fn the_trust_sections_roots_are_the_pinned_ones_without_the_distrusted_ones() {
        use azul_core::tls_testing::TestCa;
        let a = TestCa::new("azul test root A");
        let b = TestCa::new("azul test root B");
        let pems = TrustPems {
            pinned: Some(vec![a.cert_pem.clone(), b.cert_pem.clone()]),
            distrusted: vec![b.cert_pem.clone()],
        };
        assert_eq!(pems.certificates_pem(), Ok(vec![a.cert_pem.clone()]));
        let public = TrustPems::default().certificates_pem().expect("the public roots");
        assert!(public.len() > 100, "no pins: Mozilla's roots");
    }

    #[test]
    fn a_trust_section_that_names_no_pins_trusts_the_public_roots_and_a_missing_file_is_an_error() {
        let (c, _) = AzlinConfig::parse(r#"{"trust": {"distrusted_roots": ["/nowhere/x.pem"]}}"#);
        let why = c.trust.pems(None, None).unwrap_err();
        assert!(why.contains("/nowhere/x.pem"), "{why}");
        let (c, _) = AzlinConfig::parse("{}");
        assert_eq!(c.trust.pems(None, None), Ok(TrustPems::default()));
        assert_eq!(TrustPems::default().pinned, None, "no pins: the public roots");
    }

    #[test]
    fn the_updates_section_reads_the_channel_the_cohort_and_each_apps_policy_and_keeps_the_rest() {
        let json = r#"{"updates": {
            "autoupdate": false, "channel": "beta", "cohort": 42,
            "install_dir": "/home/ann/apps", "manifest_url": "https://example.org/{app}/{channel}.json",
            "root_public_key": "RWQroot", "maintenance_window": "FREQ=DAILY;BYHOUR=2",
            "overrides": {"azwriter": {"autoupdate": true, "channel": "stable", "skip_version": "2.0.0", "pinned": 1}},
            "future": [1, 2]
        }}"#;
        let (c, problem) = AzlinConfig::parse(json);
        assert_eq!(problem, None);
        let u = &c.updates;
        assert_eq!(u.autoupdate, Some(false));
        assert_eq!(u.channel, Some(Channel::Beta));
        assert_eq!(u.cohort, Some(42));
        assert_eq!(u.install_dir.as_deref(), Some("/home/ann/apps"));
        assert_eq!(
            u.manifest_url.as_deref(),
            Some("https://example.org/{app}/{channel}.json")
        );
        assert_eq!(u.root_public_key.as_deref(), Some("RWQroot"));
        assert_eq!(u.maintenance_window.as_deref(), Some("FREQ=DAILY;BYHOUR=2"));
        let writer = u.policy_for("azwriter");
        assert!(writer.autoupdate, "the app's own choice wins");
        assert_eq!(writer.channel, Channel::Stable);
        assert_eq!(writer.skip_version.as_deref(), Some("2.0.0"));
        let sheets = u.policy_for("azsheets");
        assert!(!sheets.autoupdate, "the shared choice");
        assert_eq!(sheets.channel, Channel::Beta);
        assert_eq!(sheets.skip_version, None);
        let text = c.to_json();
        let (again, _) = AzlinConfig::parse(&text);
        assert_eq!(again, c, "it round-trips");
        assert!(text.contains("\"future\"") && text.contains("\"pinned\""), "{text}");
    }

    #[test]
    fn without_an_updates_section_every_app_updates_itself_on_the_stable_channel() {
        let (c, _) = AzlinConfig::parse("{}");
        let policy = c.updates.policy_for("azmail");
        assert!(policy.autoupdate);
        assert_eq!(policy.channel, Channel::Stable);
        assert!(c.updates.is_empty());
        assert!(!c.to_json().contains("updates"), "an empty section is not written");
    }

    #[test]
    fn nightly_is_no_channel_and_a_cohort_past_99_is_no_cohort() {
        assert_eq!(Channel::parse("stable"), Some(Channel::Stable));
        assert_eq!(Channel::parse("Beta"), Some(Channel::Beta));
        assert_eq!(Channel::parse("nightly"), None);
        let (c, problem) =
            AzlinConfig::parse(r#"{"updates": {"channel": "nightly", "cohort": 150}}"#);
        assert_eq!(c.updates.channel, None);
        assert_eq!(c.updates.cohort, None);
        let problem = problem.expect("both are reported");
        assert!(problem.contains("nightly") && problem.contains("150"), "{problem}");
        let text = c.to_json();
        assert!(text.contains("nightly"), "what was not understood is kept: {text}");
    }

    #[test]
    fn the_updates_setters_write_the_shared_choice_or_one_apps_and_edit_keeps_the_rest() {
        let dir = std::env::temp_dir().join(format!("azlin-updates-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&path, r#"{"mode": "dark", "updates": {"overrides": {"azpasswordmanager": {"x": 1}}}}"#)
            .unwrap();
        AzlinConfig::edit(&path, |c| {
            c.updates.set_autoupdate(None, false);
            c.updates.set_autoupdate(Some("azpasswordmanager"), true);
            c.updates.set_channel(Some("azpasswordmanager"), Channel::Beta);
            c.updates.set_skip_version("azpasswordmanager", Some("3.1.0"));
            c.updates.cohort = Some(7);
        })
        .unwrap();
        let (c, problem) = AzlinConfig::load(&path);
        assert_eq!(problem, None);
        assert_eq!(c.mode, Some(ModePref::Dark), "the look stays");
        assert_eq!(c.updates.cohort, Some(7));
        let keys = c.updates.policy_for("azpasswordmanager");
        assert!(keys.autoupdate);
        assert_eq!(keys.channel, Channel::Beta);
        assert_eq!(keys.skip_version.as_deref(), Some("3.1.0"));
        assert!(!c.updates.policy_for("azmail").autoupdate);
        assert!(c.to_json().contains("\"x\": 1"), "an override's other keys stay");
        AzlinConfig::edit(&path, |c| c.updates.set_skip_version("azpasswordmanager", None)).unwrap();
        let (c, _) = AzlinConfig::load(&path);
        assert_eq!(c.updates.policy_for("azpasswordmanager").skip_version, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// AzKeys became AzPasswordManager: the update choices a user made for it under its former
    /// key are its choices now, and the next write keeps them under the new key alone.
    #[test]
    fn a_renamed_apps_update_choices_are_kept_under_its_new_key() {
        let dir = std::env::temp_dir().join(format!("azlin-renamed-test-{}", std::process::id()));
        let path = dir.join("config.json");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            &path,
            r#"{"updates": {"overrides": {"azkeys": {"channel": "beta", "autoupdate": false}}}}"#,
        )
        .unwrap();
        let (c, problem) = AzlinConfig::load(&path);
        assert_eq!(problem, None);
        let policy = c.updates.policy_for("azpasswordmanager");
        assert_eq!(policy.channel, Channel::Beta);
        assert!(!policy.autoupdate);
        AzlinConfig::edit(&path, |c| c.updates.cohort = Some(3)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("\"azpasswordmanager\""), "{text}");
        assert!(!text.contains("\"azkeys\""), "the former key is gone from the file: {text}");
        assert_eq!(current_app_key("azkeys"), "azpasswordmanager");
        assert_eq!(current_app_key("azmail"), "azmail");
        assert_eq!(former_app_key("azpasswordmanager"), Some("azkeys"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
