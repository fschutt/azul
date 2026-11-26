//! From the settings of a task to the backend that answers it.
//!
//! The settings are text the user typed (or a file somebody edited), so everything is checked
//! again here, where a backend is made: the program is looked for (a missing one says where it was
//! looked), a key reference is resolved (a reference that is set and cannot be resolved is a
//! failure, never a quiet fall back to the program's own sign-in: the user asked for that key), and
//! an extra argument that would widen what the agent may do is refused even when it came from a
//! hand-edited file. It runs on the worker (the keychain and the search of the PATH never wait on
//! the window): [`crate::agent_pool::JobSpec`]'s factory calls [`build_backend`].
//!
//! What the app knows once - the environment it started with, the keychain, the folders programs
//! live in, where a scratch folder may be - arrives as [`Wiring`]; nothing here reads the
//! environment.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use crate::{
    agent_api::{AnthropicApi, AnthropicConfig, GeminiApi, GeminiApiConfig, Transport},
    agent_backend::{AgentError, Backend, BackendKind, Task},
    agent_claude::{ClaudeConfig, ClaudePrint},
    agent_gemini::{GeminiCli, GeminiConfig},
    agent_limits::Limits,
    agent_mock::MockBackend,
    agent_process::{find_program, is_program, Host},
    agent_settings::{forbidden_arg, split_args, KeyRef, Keys, Secret, TaskSettings},
};

/// What the app wires into every backend it makes.
#[derive(Clone)]
pub struct Wiring {
    pub keys: Keys,
    /// Where programs are looked for.
    pub host: Host,
    pub limits: Limits,
    /// An empty folder for the calls of a CLI that cannot be told to use no tools (it is made when
    /// the backend is).
    pub scratch: Option<PathBuf>,
    /// Whether a path is a program (the real check is [`is_program`]).
    pub exists: fn(&Path) -> bool,
    /// How long the mock takes to answer, ms.
    pub mock_delay_ms: u64,
    /// What sends the requests of the APIs (none: this build has no HTTP client for them).
    pub transport: Option<Arc<dyn Transport>>,
}

impl Wiring {
    /// A wiring with the real check for programs and a mock that answers after 800 ms.
    #[must_use]
    pub fn new(keys: Keys, host: Host, limits: Limits) -> Self {
        Self {
            keys,
            host,
            limits,
            scratch: None,
            exists: is_program,
            mock_delay_ms: 800,
            transport: None,
        }
    }
}

/// The key the settings name: none when none is named.
///
/// # Errors
/// [`AgentError::Settings`] for a reference that cannot be read, the failures of [`Keys::resolve`]
/// for one that cannot be resolved.
pub fn resolve_key(settings: &TaskSettings, keys: &Keys) -> Result<Option<Secret>, AgentError> {
    let reference: Option<KeyRef> = settings.key_ref().map_err(|kind| AgentError::Settings {
        why: format!("the key reference cannot be read ({kind:?})"),
    })?;
    reference.map(|r| keys.resolve(&r)).transpose()
}

/// The extra arguments, checked again.
fn extra_args(settings: &TaskSettings) -> Result<Vec<String>, AgentError> {
    if let Some(arg) = forbidden_arg(&settings.extra_args) {
        return Err(AgentError::Settings {
            why: format!("the extra argument {arg} would widen what the agent may do"),
        });
    }
    Ok(split_args(&settings.extra_args))
}

/// The program of a CLI backend.
fn program(settings: &TaskSettings, name: &str, wiring: &Wiring) -> Result<PathBuf, AgentError> {
    find_program(&settings.program, name, &wiring.host, &|p| {
        (wiring.exists)(p)
    })
    .map_err(|searched| AgentError::NotInstalled {
        program: name.to_string(),
        searched,
    })
}

/// The set-up of a `claude -p` backend for `settings`.
///
/// # Errors
/// [`AgentError`]: not installed, a key that cannot be had, an argument that is refused.
pub fn claude_config(settings: &TaskSettings, wiring: &Wiring) -> Result<ClaudeConfig, AgentError> {
    Ok(ClaudeConfig {
        program: program(settings, "claude", wiring)?,
        model: settings.model.trim().to_string(),
        timeout_secs: settings.timeout_secs,
        budget_cents: settings.budget_cents,
        max_turns: settings.max_turns,
        effort: settings.effort.trim().to_string(),
        extra_args: extra_args(settings)?,
        key: resolve_key(settings, &wiring.keys)?,
        limits: wiring.limits.clone(),
    })
}

/// The set-up of a Gemini CLI backend for `settings`.
///
/// # Errors
/// [`AgentError`]: not installed, a key that cannot be had, an argument that is refused.
pub fn gemini_config(settings: &TaskSettings, wiring: &Wiring) -> Result<GeminiConfig, AgentError> {
    let config = GeminiConfig {
        program: program(settings, "gemini", wiring)?,
        model: settings.model.trim().to_string(),
        timeout_secs: settings.timeout_secs,
        extra_args: extra_args(settings)?,
        key: resolve_key(settings, &wiring.keys)?,
        scratch: wiring.scratch.clone(),
        limits: wiring.limits.clone(),
    };
    if let Some(dir) = &config.scratch {
        // an empty folder of the app's own: a failure to make it is no failure of the call
        let _ = std::fs::create_dir_all(dir);
    }
    Ok(config)
}

/// The key of an API backend: the one the settings name, else the conventional variable (so a key
/// that is not there is told as what to set).
fn api_key(
    settings: &TaskSettings,
    wiring: &Wiring,
    conventional: &str,
) -> Result<Secret, AgentError> {
    match resolve_key(settings, &wiring.keys)? {
        Some(key) => Ok(key),
        None => wiring.keys.resolve(&KeyRef::Env(conventional.to_string())),
    }
}

/// What an API backend sends its requests with.
fn transport(wiring: &Wiring) -> Result<Arc<dyn Transport>, AgentError> {
    wiring
        .transport
        .clone()
        .ok_or_else(|| AgentError::Unsupported {
            why: "this build has no HTTP client for the APIs".to_string(),
        })
}

/// The model of an API backend: one must be named.
fn api_model_name(settings: &TaskSettings) -> Result<String, AgentError> {
    let model = settings.model.trim();
    if model.is_empty() {
        return Err(AgentError::Settings {
            why: format!(
                "the {} needs a model: name one in the settings",
                settings.backend.english()
            ),
        });
    }
    Ok(model.to_string())
}

/// The set-up of an Anthropic API backend for `settings`.
///
/// # Errors
/// [`AgentError`]: no HTTP client, no model, no key.
pub fn anthropic_config(
    settings: &TaskSettings,
    wiring: &Wiring,
) -> Result<AnthropicConfig, AgentError> {
    Ok(AnthropicConfig {
        model: api_model_name(settings)?,
        base_url: settings.base_url.clone(),
        timeout_secs: settings.timeout_secs,
        max_tokens: settings.max_output_tokens,
        effort: settings.effort.trim().to_string(),
        key: api_key(settings, wiring, "ANTHROPIC_API_KEY")?,
        limits: wiring.limits.clone(),
    })
}

/// The set-up of a Gemini API backend for `settings`.
///
/// # Errors
/// [`AgentError`]: no model, no key.
pub fn gemini_api_config(
    settings: &TaskSettings,
    wiring: &Wiring,
) -> Result<GeminiApiConfig, AgentError> {
    Ok(GeminiApiConfig {
        model: api_model_name(settings)?,
        base_url: settings.base_url.clone(),
        timeout_secs: settings.timeout_secs,
        max_tokens: settings.max_output_tokens,
        key: api_key(settings, wiring, "GEMINI_API_KEY")?,
        limits: wiring.limits.clone(),
    })
}

/// The backend `settings` ask for `task`.
///
/// # Errors
/// [`AgentError`]: what is wrong with the set-up.
pub fn build_backend(
    _task: Task,
    settings: &TaskSettings,
    wiring: &Wiring,
) -> Result<Box<dyn Backend>, AgentError> {
    match settings.backend {
        BackendKind::ClaudePrint => Ok(Box::new(ClaudePrint {
            config: claude_config(settings, wiring)?,
        })),
        BackendKind::GeminiCli => Ok(Box::new(GeminiCli {
            config: gemini_config(settings, wiring)?,
        })),
        BackendKind::Mock => Ok(Box::new(MockBackend::auto(wiring.mock_delay_ms))),
        BackendKind::AnthropicApi => {
            let transport = transport(wiring)?;
            Ok(Box::new(AnthropicApi {
                config: anthropic_config(settings, wiring)?,
                transport,
            }))
        }
        BackendKind::GeminiApi => {
            let transport = transport(wiring)?;
            Ok(Box::new(GeminiApi {
                config: gemini_api_config(settings, wiring)?,
                transport,
            }))
        }
    }
}

/// The factory a [`crate::agent_pool::JobSpec`] carries: it makes the backend on the worker.
#[must_use]
pub fn factory(
    task: Task,
    settings: TaskSettings,
    wiring: Arc<Wiring>,
) -> crate::agent_pool::Factory {
    Box::new(move || build_backend(task, &settings, &wiring))
}

#[cfg(test)]
#[path = "agent_factory_test.rs"]
mod agent_factory_test;
