//! `claude -p`: the Claude Code CLI in print mode as a backend. It needs no key (it uses the login
//! the user made once in a terminal), it answers with ONE JSON object (`--output-format json`), and
//! it is the one backend that can read the workspace (read-only tools, for the exploration).
//!
//! The command line is built here ([`args`]) and nowhere else, from what the CLI's own `--help`
//! says (2.1.332):
//!
//! - the prompt goes in on standard input: `--tools` takes any number of values, so a prompt after
//!   it on the command line would be read as a tool name;
//! - `--safe-mode`: the user's CLAUDE.md, hooks, plugins and MCP servers (and the workspace's) are
//!   not loaded - an agent that is meant to be read-only must not run a project's hooks, and its
//!   answer must not depend on a project's notes; the login still works;
//! - no tools for the editing and the export (`--tools ""`, the instructions replace the default
//!   system prompt); for the exploration only `Read`, `Grep` and `Glob`, anything else denied
//!   without asking (`--permission-mode dontAsk`), run in the workspace's folder;
//! - a cost limit (`--max-budget-usd`) and, when asked for, a turn limit (`--max-turns`, which the
//!   CLI accepts and does not list);
//! - `--no-session-persistence`: no session file is left behind for each question.
//!
//! The answer is read by [`read_envelope`]: `is_error` says it failed (the `subtype` of a failure
//! can still say `success`: "Not logged in" does), and the failure is told apart by its words.

use std::path::PathBuf;

use crate::{
    agent_backend::{
        scrub, AgentError, Backend, BackendKind, Call, Completion, Control, Identity, Tools, Usage,
    },
    agent_json::{self, Json},
    agent_limits::{cap_chars, Limits},
    agent_process::{self, Output, Spec},
    agent_settings::Secret,
};

/// What a `claude -p` backend is set up with.
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// The program, found already.
    pub program: PathBuf,
    /// Empty: the CLI's own default.
    pub model: String,
    pub timeout_secs: u32,
    /// US cents (0: no limit).
    pub budget_cents: u32,
    /// 0: no limit.
    pub max_turns: u32,
    /// `low` to `max`; empty: the model's own.
    pub effort: String,
    pub extra_args: Vec<String>,
    /// A key the user named: handed to the program as `ANTHROPIC_API_KEY` (it then bills the API,
    /// not the login); none: the program's own login.
    pub key: Option<Secret>,
    pub limits: Limits,
}

/// The `claude -p` backend.
#[derive(Debug, Clone)]
pub struct ClaudePrint {
    pub config: ClaudeConfig,
}

/// The tools the exploration may use.
const READ_ONLY_TOOLS: &str = "Read,Grep,Glob";

/// The command line for `call`.
#[must_use]
pub fn args(config: &ClaudeConfig, call: &Call<'_>) -> Vec<String> {
    let mut a: Vec<String> = ["-p", "--no-session-persistence", "--safe-mode"]
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    match call.tools {
        Tools::None => {
            a.extend(["--tools".to_string(), String::new()]);
            a.extend(["--system-prompt".to_string(), call.system.clone()]);
        }
        Tools::ReadOnly => {
            a.extend(["--tools".to_string(), READ_ONLY_TOOLS.to_string()]);
            a.extend(["--permission-mode".to_string(), "dontAsk".to_string()]);
            a.extend(["--allowedTools".to_string(), READ_ONLY_TOOLS.to_string()]);
            a.extend(["--append-system-prompt".to_string(), call.system.clone()]);
        }
    }
    if !config.model.trim().is_empty() {
        a.extend(["--model".to_string(), config.model.trim().to_string()]);
    }
    if config.budget_cents > 0 {
        a.extend([
            "--max-budget-usd".to_string(),
            format!(
                "{}.{:02}",
                config.budget_cents / 100,
                config.budget_cents % 100
            ),
        ]);
    }
    if config.max_turns > 0 {
        a.extend(["--max-turns".to_string(), config.max_turns.to_string()]);
    }
    if !config.effort.trim().is_empty() {
        a.extend(["--effort".to_string(), config.effort.trim().to_string()]);
    }
    // the last built option takes exactly one value, so an extra argument is never read as one
    // more value of a list option
    a.extend(["--output-format".to_string(), "json".to_string()]);
    a.extend(config.extra_args.iter().cloned());
    a
}

/// What to run for `call`.
#[must_use]
pub fn spec(config: &ClaudeConfig, call: &Call<'_>) -> Spec {
    let mut env_set = vec![("DISABLE_AUTOUPDATER".to_string(), "1".to_string())];
    if let Some(key) = &config.key {
        env_set.push(("ANTHROPIC_API_KEY".to_string(), key.expose().to_string()));
    }
    Spec {
        program: config.program.clone(),
        args: args(config, call),
        cwd: call.workdir.clone(),
        env_set,
        env_remove: Vec::new(),
        stdin: call.prompt.clone().into_bytes(),
        timeout_ms: u64::from(config.timeout_secs) * 1_000,
        max_stdout: config.limits.max_stdout_bytes,
        max_stderr: config.limits.max_stderr_bytes,
    }
}

/// The last lines of what a program said on its standard error, scrubbed and cut: what is shown
/// when it stopped without an answer.
#[must_use]
pub fn tail_of(text: &str, secrets: &[&str], max_chars: usize) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let from = lines.len().saturating_sub(4);
    cap_chars(&scrub(&lines[from..].join(" | "), secrets), max_chars).0
}

/// How a failure that came as words is told apart: by what it says.
fn classify(text: &str, code: Option<i32>, config: &ClaudeConfig, secrets: &[&str]) -> AgentError {
    let detail = tail_of(text, secrets, 300);
    let lower = text.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if has(&[
        "not logged in",
        "/login",
        "invalid api key",
        "invalid x-api-key",
        "oauth token has expired",
        "please run claude login",
        "authentication",
    ]) {
        AgentError::NotLoggedIn {
            program: "claude".to_string(),
            detail,
        }
    } else if has(&[
        "rate limit",
        "429",
        "overloaded",
        "529",
        "usage limit",
        "hit your limit",
        "limit reached",
        "credit balance",
        "quota",
    ]) {
        AgentError::RateLimited {
            retry_after_secs: None,
            detail,
        }
    } else if has(&[
        "unable to connect",
        "econnrefused",
        "connectionrefused",
        "connection refused",
        "enotfound",
        "network error",
        "timed out",
        "etimedout",
        "fetch failed",
    ]) {
        AgentError::Network { why: detail }
    } else if has(&[
        "selected model",
        "model not found",
        "invalid model",
        "may not exist",
        "not_found_error",
    ]) {
        AgentError::Settings {
            why: format!(
                "the model {:?} is not one claude knows: {detail}",
                config.model.trim()
            ),
        }
    } else {
        AgentError::Exit {
            program: "claude".to_string(),
            code,
            detail,
        }
    }
}

/// The tokens a call read: what was new, what was written to the cache, what came from it.
fn input_tokens(usage: &Json) -> Option<u64> {
    let parts = [
        "input_tokens",
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
    ];
    let found: Vec<u64> = parts
        .iter()
        .filter_map(|k| agent_json::u64_of(usage, k))
        .collect();
    (!found.is_empty()).then(|| found.iter().sum())
}

/// Reads what `claude -p --output-format json` left.
///
/// # Errors
/// [`AgentError`]: the failure the envelope says, or what the program said on its way out.
pub fn read_envelope(
    out: &Output,
    config: &ClaudeConfig,
    secrets: &[&str],
) -> Result<Completion, AgentError> {
    let stdout = out.stdout_text();
    let envelope =
        agent_json::find_object_where(&stdout, config.limits.max_stdout_bytes.max(1), &|v| {
            agent_json::str_of(v, "type") == Some("result")
                || agent_json::get(v, "result").is_some()
        })
        .ok()
        .flatten();
    let Some(found) = envelope else {
        let stderr = out.stderr_text();
        let said = if stderr.trim().is_empty() {
            stdout
        } else {
            stderr
        };
        return if out.code == Some(0) {
            Err(AgentError::BadOutput {
                why: "claude answered without a JSON result".to_string(),
                excerpt: cap_chars(scrub(said.trim(), secrets).as_str(), 200).0,
            })
        } else {
            Err(classify(&said, out.code, config, secrets))
        };
    };
    let env = found.value;
    let result = agent_json::str_of(&env, "result").unwrap_or("");
    let subtype = agent_json::str_of(&env, "subtype").unwrap_or("");
    let turns = agent_json::u64_of(&env, "num_turns").and_then(|n| u32::try_from(n).ok());
    if subtype == "error_max_turns" {
        return Err(AgentError::MaxTurns {
            turns: turns.unwrap_or(config.max_turns),
        });
    }
    if subtype == "error_max_budget_usd" {
        return Err(AgentError::Budget {
            limit_usd: f64::from(config.budget_cents) / 100.0,
        });
    }
    if agent_json::bool_of(&env, "is_error") == Some(true) || subtype.starts_with("error") {
        let text = if result.trim().is_empty() {
            stdout.as_str()
        } else {
            result
        };
        return Err(classify(text, out.code, config, secrets));
    }
    let usage = agent_json::get(&env, "usage");
    let model = agent_json::get(&env, "modelUsage")
        .and_then(Json::as_object)
        .and_then(|m| m.keys().next().cloned())
        .unwrap_or_default();
    Ok(Completion {
        text: result.to_string(),
        usage: Usage {
            input_tokens: usage.and_then(input_tokens),
            output_tokens: usage.and_then(|u| agent_json::u64_of(u, "output_tokens")),
            cost_usd: agent_json::f64_of(&env, "total_cost_usd"),
            elapsed_ms: 0,
            turns,
        },
        model,
        notes: Vec::new(),
    })
}

impl Backend for ClaudePrint {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::ClaudePrint,
            model: self.config.model.trim().to_string(),
        }
    }

    fn complete(
        &mut self,
        call: &Call<'_>,
        ctl: &mut Control<'_>,
    ) -> Result<Completion, AgentError> {
        let spec = spec(&self.config, call);
        let secrets: Vec<&str> = self.config.key.iter().map(Secret::expose).collect();
        let out = agent_process::run(&spec, ctl)
            .map_err(|failure| failure.into_error("claude", Vec::new(), spec.timeout_ms))?;
        read_envelope(&out, &self.config, &secrets)
    }
}

#[cfg(test)]
#[path = "agent_claude_test.rs"]
mod agent_claude_test;
