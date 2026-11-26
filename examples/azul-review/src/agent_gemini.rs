//! `gemini`: Google's Gemini CLI in headless mode as a backend.
//!
//! Its command line is built here ([`args`]), from `gemini --help` (0.65.0) and from what the CLI
//! was seen to do:
//!
//! - `-p` marks the run as headless and its text is APPENDED to what is on standard input, so the
//!   instructions and the prompt go in on standard input (the system part first: the CLI has no
//!   option for one) and `-p` carries the one line that says what to do with them;
//! - `-o json`: one JSON object on standard output; a failure is a JSON object on standard error
//!   with the exit code in it (41: not signed in);
//! - `--approval-mode plan`: read-only; the CLI quietly turns it into `default` in a folder it does
//!   not trust, so `--skip-trust` goes with it;
//! - the model, when one is named (`-m`); none is the CLI's own routing.
//!
//! The CLI cannot be told to use no tools, so the editing and export prompts say "answer from the
//! text, do not use tools", and those calls run in an empty folder of their own (nothing to read, no
//! project notes of the user's to load); the exploration runs in the workspace.

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

/// What a Gemini CLI backend is set up with.
#[derive(Debug, Clone)]
pub struct GeminiConfig {
    pub program: PathBuf,
    /// Empty: the CLI's own routing.
    pub model: String,
    pub timeout_secs: u32,
    pub extra_args: Vec<String>,
    /// A key the user named: handed to the program as `GEMINI_API_KEY`.
    pub key: Option<Secret>,
    /// An empty folder to run the calls without tools in (none: the app's own folder).
    pub scratch: Option<PathBuf>,
    pub limits: Limits,
}

/// The Gemini CLI backend.
#[derive(Debug, Clone)]
pub struct GeminiCli {
    pub config: GeminiConfig,
}

/// The line `-p` carries: what to do with what was written on standard input.
#[must_use]
pub fn instruction(tools: Tools) -> &'static str {
    match tools {
        Tools::None => {
            "Follow the instructions above and answer from the text given, without using any tool."
        }
        Tools::ReadOnly => "Follow the instructions above; you may read files of this folder.",
    }
}

/// The first line of what goes in on standard input. A CLI whose sign-in has expired asks "Opening
/// authentication page in your browser. Do you want to continue? [Y/n]" and reads the answer from
/// standard input: the text must never begin like a "yes" (it would open a browser), so it begins
/// with a line that is no answer to anything.
const FIRST_LINE: &str =
    "# AzReview agent request (this is not an answer to a question of the program)";

/// What goes in on standard input: the standing instructions, then the request.
#[must_use]
pub fn input_text(call: &Call<'_>) -> String {
    format!("{FIRST_LINE}\n\n{}\n\n---\n\n{}", call.system, call.prompt)
}

/// The command line for `call`.
#[must_use]
pub fn args(config: &GeminiConfig, call: &Call<'_>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        "-p".to_string(),
        instruction(call.tools).to_string(),
        "-o".to_string(),
        "json".to_string(),
        "--approval-mode".to_string(),
        "plan".to_string(),
        "--skip-trust".to_string(),
    ];
    if !config.model.trim().is_empty() {
        a.extend(["-m".to_string(), config.model.trim().to_string()]);
    }
    a.extend(config.extra_args.iter().cloned());
    a
}

/// What to run for `call`.
#[must_use]
pub fn spec(config: &GeminiConfig, call: &Call<'_>) -> Spec {
    let mut env_set = Vec::new();
    if let Some(key) = &config.key {
        env_set.push(("GEMINI_API_KEY".to_string(), key.expose().to_string()));
    }
    Spec {
        program: config.program.clone(),
        args: args(config, call),
        cwd: call.workdir.clone().or_else(|| config.scratch.clone()),
        env_set,
        env_remove: Vec::new(),
        stdin: input_text(call).into_bytes(),
        timeout_ms: u64::from(config.timeout_secs) * 1_000,
        max_stdout: config.limits.max_stdout_bytes,
        max_stderr: config.limits.max_stderr_bytes,
    }
}

/// How a failure is told apart: by the CLI's exit code, then by what it says.
fn classify(
    message: &str,
    code: Option<i64>,
    config: &GeminiConfig,
    secrets: &[&str],
) -> AgentError {
    let detail = cap_chars(&scrub(message.trim(), secrets), 300).0;
    let lower = message.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    if code == Some(41)
        || has(&[
            "auth method",
            "not logged in",
            "api key not valid",
            "api_key_invalid",
            "invalid api key",
            "permission_denied",
            "unauthenticated",
            "login required",
        ])
    {
        AgentError::NotLoggedIn {
            program: "gemini".to_string(),
            detail,
        }
    } else if code == Some(53) {
        AgentError::MaxTurns { turns: 0 }
    } else if has(&[
        "429",
        "quota",
        "resource_exhausted",
        "rate limit",
        "too many requests",
        "overloaded",
        "503",
        "unavailable",
    ]) {
        AgentError::RateLimited {
            retry_after_secs: None,
            detail,
        }
    } else if has(&[
        "enotfound",
        "econnrefused",
        "econnreset",
        "etimedout",
        "fetch failed",
        "network",
        "unable to connect",
    ]) {
        AgentError::Network { why: detail }
    } else if has(&[
        "modelnotfound",
        "model not found",
        "is not found for api version",
        "not found: models/",
        "requested entity was not found",
        "invalid model",
    ]) {
        AgentError::Settings {
            why: format!(
                "the model {:?} is not one gemini knows: {detail}",
                config.model.trim()
            ),
        }
    } else {
        AgentError::Exit {
            program: "gemini".to_string(),
            code: code.and_then(|c| i32::try_from(c).ok()),
            detail,
        }
    }
}

/// An object with an `error` in it, anywhere in `text` (the CLI writes its failure as JSON on
/// standard error, after a line or two of notes).
fn error_of(text: &str, limit: usize) -> Option<(String, Option<i64>)> {
    let found = agent_json::find_object_where(text, limit.max(1), &|v| {
        agent_json::get(v, "error").is_some()
    })
    .ok()
    .flatten()?;
    let error = agent_json::get(&found.value, "error")?;
    let message = agent_json::str_of(error, "message")
        .unwrap_or("")
        .to_string();
    let code = agent_json::get(error, "code").and_then(Json::as_i64);
    Some((message, code))
}

/// The tokens a model of the stats counted: the first model that has some.
fn token_counts(stats: &Json) -> (Option<u64>, Option<u64>, String) {
    let Some(models) = agent_json::get(stats, "models").and_then(Json::as_object) else {
        return (None, None, String::new());
    };
    let mut input = None;
    let mut output = None;
    let mut name = String::new();
    for (model, entry) in models {
        let tokens = agent_json::get(entry, "tokens");
        let count = |key: &str| tokens.and_then(|t| agent_json::u64_of(t, key));
        let (p, c) = (
            count("prompt").or_else(|| count("input")),
            count("candidates").or_else(|| count("output")),
        );
        if name.is_empty() {
            name.clone_from(model);
        }
        input = Some(input.unwrap_or(0) + p.unwrap_or(0));
        output = Some(output.unwrap_or(0) + c.unwrap_or(0));
    }
    (input, output, name)
}

/// Whether what the program said is its sign-in asking to be made again.
fn asks_to_sign_in(text: &str) -> bool {
    let lower = text.to_lowercase();
    [
        "opening authentication page",
        "authentication cancelled",
        "fatalcancellationerror",
        "initoauthclient",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

/// Reads what `gemini -o json` left.
///
/// # Errors
/// [`AgentError`]: the failure it says on standard error (or standard output), or what it said on its way out.
pub fn read_output(
    out: &Output,
    config: &GeminiConfig,
    secrets: &[&str],
) -> Result<Completion, AgentError> {
    let limit = config.limits.max_stdout_bytes;
    let stdout = out.stdout_text();
    let stderr = out.stderr_text();
    if asks_to_sign_in(&stdout) || asks_to_sign_in(&stderr) {
        return Err(AgentError::NotLoggedIn {
            program: "gemini".to_string(),
            detail: "its sign-in has expired".to_string(),
        });
    }
    let answer = agent_json::find_object_where(&stdout, limit.max(1), &|v| {
        agent_json::get(v, "response").is_some() || agent_json::get(v, "error").is_some()
    })
    .ok()
    .flatten();
    if let Some(found) = &answer {
        if let Some(text) = agent_json::str_of(&found.value, "response") {
            let (input, output, model) = agent_json::get(&found.value, "stats")
                .map_or((None, None, String::new()), token_counts);
            return Ok(Completion {
                text: text.to_string(),
                usage: Usage {
                    input_tokens: input,
                    output_tokens: output,
                    cost_usd: None,
                    elapsed_ms: 0,
                    turns: None,
                },
                model,
                notes: Vec::new(),
            });
        }
    }
    if let Some((message, code)) =
        error_of(&stdout, limit).or_else(|| error_of(&stderr, config.limits.max_stderr_bytes))
    {
        return Err(classify(
            &message,
            code.or_else(|| out.code.map(i64::from)),
            config,
            secrets,
        ));
    }
    if out.code == Some(0) {
        let shown = if stdout.trim().is_empty() {
            stderr
        } else {
            stdout
        };
        return Err(AgentError::BadOutput {
            why: "gemini answered without a JSON response".to_string(),
            excerpt: cap_chars(scrub(shown.trim(), secrets).as_str(), 200).0,
        });
    }
    let said = if stderr.trim().is_empty() {
        stdout
    } else {
        stderr
    };
    Err(classify(&said, out.code.map(i64::from), config, secrets))
}

impl Backend for GeminiCli {
    fn identity(&self) -> Identity {
        Identity {
            kind: BackendKind::GeminiCli,
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
            .map_err(|failure| failure.into_error("gemini", Vec::new(), spec.timeout_ms))?;
        read_output(&out, &self.config, &secrets)
    }
}

#[cfg(test)]
#[path = "agent_gemini_test.rs"]
mod agent_gemini_test;
