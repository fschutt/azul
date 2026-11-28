//! The tasks of a workspace, for the Run view: what its folder says can be run - a Cargo project's
//! `cargo run`, `build`, `test` and `check`, the `scripts` of a `package.json`, the targets of a
//! `Makefile`. Plain Rust over the text of those files (read on the drive thread when the folder
//! is first listed); a task is a command line the terminal panel types into a shell, so a name is
//! kept only when it is plain: nothing a shell would read as more than one word.

/// The most tasks of one kind the view lists.
pub const MAX_TASKS: usize = 40;

/// Where a task comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Cargo,
    Npm,
    Make,
}

impl Source {
    /// The key of the words that name the source (`azcode-task-cargo`).
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Source::Cargo => "azcode-task-cargo",
            Source::Npm => "azcode-task-npm",
            Source::Make => "azcode-task-make",
        }
    }

    /// The file the tasks are read from.
    #[must_use]
    pub const fn file(self) -> &'static str {
        match self {
            Source::Cargo => "Cargo.toml",
            Source::Npm => "package.json",
            Source::Make => "Makefile",
        }
    }
}

/// One thing to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub source: Source,
    /// What the list calls it (`build`, the script's name, the target's).
    pub label: String,
    /// What the terminal is typed (`cargo build`).
    pub command: String,
}

/// Whether `name` is plain enough to be one word of a command line: letters, digits and `_ - . : / @`,
/// not starting with `-` (an option) and not empty.
#[must_use]
pub fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.len() <= 100
        && name.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/' | '@'))
}

/// The cargo tasks of a `Cargo.toml`: build, test and check for a package or a workspace, run for a
/// package (a manifest that is a workspace alone has nothing of its own to run).
#[must_use]
pub fn from_cargo(manifest: &str) -> Vec<Task> {
    let has = |header: &str| manifest.lines().any(|l| l.trim() == header);
    let package = has("[package]");
    let workspace = has("[workspace]");
    if !package && !workspace {
        return Vec::new();
    }
    let mut verbs = Vec::new();
    if package {
        verbs.push("run");
    }
    verbs.extend(["build", "test", "check"]);
    verbs
        .into_iter()
        .map(|verb| Task { source: Source::Cargo, label: verb.to_string(), command: format!("cargo {verb}") })
        .collect()
}

/// The scripts of a `package.json`: `npm run <name>`, by name (a JSON object has no order), plain names
/// only, [`MAX_TASKS`] at most.
#[must_use]
pub fn from_package_json(text: &str) -> Vec<Task> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(text) else {
        return Vec::new();
    };
    let Some(scripts) = value.get("scripts").and_then(|s| s.as_object()) else {
        return Vec::new();
    };
    scripts
        .keys()
        .filter(|name| plain_name(name))
        .take(MAX_TASKS)
        .map(|name| Task { source: Source::Npm, label: name.clone(), command: format!("npm run {name}") })
        .collect()
}

/// The targets of a `Makefile`: the names before a `:` at the start of a line, not the pattern rules
/// (`%`), the special targets (`.PHONY`) or the assignments (`:=`), once each, [`MAX_TASKS`] at most.
#[must_use]
pub fn from_makefile(text: &str) -> Vec<Task> {
    let mut targets: Vec<String> = Vec::new();
    for line in text.lines() {
        // A recipe line (tab) or an indented one is no target.
        if line.starts_with(['\t', ' ', '#']) {
            continue;
        }
        let Some((head, tail)) = line.split_once(':') else {
            continue;
        };
        if tail.starts_with('=') || tail.starts_with(':') {
            continue;
        }
        for name in head.split_whitespace() {
            if name.starts_with('.') || name.contains('%') || name.contains('$') || !plain_name(name) {
                continue;
            }
            if !targets.iter().any(|t| t == name) {
                targets.push(name.to_string());
            }
        }
        if targets.len() >= MAX_TASKS {
            break;
        }
    }
    targets.truncate(MAX_TASKS);
    targets
        .into_iter()
        .map(|name| Task { source: Source::Make, command: format!("make {name}"), label: name })
        .collect()
}

/// The tasks the root files of a workspace give: `files` are `(file name, its text)` for those
/// of [`Source::file`] that the folder has.
#[must_use]
pub fn detect(files: &[(String, String)]) -> Vec<Task> {
    let mut tasks = Vec::new();
    for source in [Source::Cargo, Source::Npm, Source::Make] {
        let Some((_, text)) = files.iter().find(|(name, _)| name == source.file()) else {
            continue;
        };
        tasks.extend(match source {
            Source::Cargo => from_cargo(text),
            Source::Npm => from_package_json(text),
            Source::Make => from_makefile(text),
        });
    }
    tasks
}

#[cfg(test)]
#[path = "tasks_test.rs"]
mod tests;
