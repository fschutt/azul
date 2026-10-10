//! Where the token server of this run is, as azul-appkit's shared config resolves it
//! ([`azul_appkit::azlin_config::resolve_endpoints`], lowest layer first): the profile's
//! built-in address (`local` - the local stack - unless something names another profile), the
//! shared config file's `endpoints` section (`$AZLIN_CONFIG`, else `~/.azlin/config.json`;
//! `AZLIN_CONFIG=off` reads none), the environment (`AZLIN_TOKEN_URL`, `AZLIN_PROFILE`), the
//! app's `--token-url` / `--profile` switches. Pure but for reading the config file: the caller
//! hands in its environment and its home folder.

use std::path::Path;

use azul_appkit::azlin_config::{
    config_path, resolve_endpoints, AzlinConfig, Endpoint, EndpointFlags, Profile, CONFIG_VAR,
};

use crate::token::{is_loopback_host, url_host};

/// The token server of this run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenEndpoint {
    /// Its address; `None` when nothing names one.
    pub url: Option<String>,
    /// Where the address came from, for people: `profile local`, `config file <path>`,
    /// `environment AZLIN_TOKEN_URL`, `flag --token-url`.
    pub source: String,
    /// The profile whose addresses are the bottom layer.
    pub profile: Profile,
    /// A development token server: the local profile's, or one on this computer. It signs up
    /// drives without payment (`POST /v1/drives`); a production server sells them through a
    /// checkout only.
    pub development: bool,
    /// What a layer said that could not be used (a file that is not JSON, an address that is
    /// none), for a log line.
    pub problems: Vec<String>,
    /// The iroh relay of this run (`off`, `default` or an address), when a layer names one: the
    /// Azlin drives' iroh lane dials through it.
    pub relay: Option<String>,
}

/// The token server of this run: `flag_token` / `flag_profile` (the app's `--token-url` /
/// `--profile`), then `env` (reads a variable), then the shared config file (found through
/// `AZLIN_CONFIG` or `home`), then the profile's built-in address.
#[must_use]
pub fn token_endpoint(
    flag_token: Option<&str>,
    flag_profile: Option<&str>,
    env: &dyn Fn(&str) -> Option<String>,
    home: Option<&Path>,
) -> TokenEndpoint {
    let mut problems = Vec::new();
    let path = config_path(env(CONFIG_VAR).as_deref(), home);
    let loaded = path.map(|path| {
        let (config, problem) = AzlinConfig::load(&path);
        if let Some(problem) = problem {
            problems.push(format!("{}: {problem}", path.display()));
        }
        (path, config)
    });
    let file = loaded
        .as_ref()
        .map(|(path, config)| (path.as_path(), &config.endpoints));
    let flags = EndpointFlags {
        profile: flag_profile.map(str::to_string),
        token: flag_token.map(str::to_string),
        ..EndpointFlags::default()
    };
    let resolved = resolve_endpoints(file, env, &flags);
    let token = resolved.get(Endpoint::Token);
    for (source, raw, why) in &token.rejected {
        problems.push(format!(
            "the token server {raw:?} from {} passed over: {why}",
            source.label()
        ));
    }
    let url = token.value.clone();
    let development = resolved.profile == Profile::Local
        || url
            .as_deref()
            .and_then(url_host)
            .is_some_and(|(host, _)| is_loopback_host(host));
    TokenEndpoint {
        source: token.source.label(),
        url,
        profile: resolved.profile,
        development,
        problems,
        relay: resolved.url(Endpoint::Relay).map(str::to_string),
    }
}
