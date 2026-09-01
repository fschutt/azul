//! A tiny plan/execute layer shared by `mobile install` and `mobile doctor`.
//!
//! Installing a mobile toolchain means running a dozen commands that each take
//! anywhere from a second to twenty minutes, some of which cannot be undone
//! and one of which (Xcode) cannot be automated at all. So nothing runs until
//! the whole plan has been *shown*: every step reports whether it is already
//! satisfied, exactly what would run, or why a human has to do it.
//!
//! That also makes `doctor` free — it is the same plan, printed and not run.

use std::io::IsTerminal as _;

use super::toolchain::Cmd;

/// What has to happen for one step.
pub enum Action {
    /// Nothing to do. The string says how we know.
    Satisfied(String),
    /// We can do this ourselves.
    Run(Cmd),
    /// Needs a GUI, an Apple ID, `sudo`, or a decision we should not make.
    Manual { why: String, commands: Vec<String> },
    /// Not applicable on this host — recorded rather than hidden, so the
    /// report reads the same everywhere.
    Skipped(String),
}

pub struct Step {
    pub label: String,
    pub action: Action,
    /// Optional steps never fail the run; they are conveniences (device
    /// deploy helpers, headless drivers) rather than build requirements.
    pub optional: bool,
}

impl Step {
    pub fn satisfied(label: impl Into<String>, how: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: Action::Satisfied(how.into()),
            optional: false,
        }
    }

    pub fn run(label: impl Into<String>, cmd: Cmd) -> Self {
        Self {
            label: label.into(),
            action: Action::Run(cmd),
            optional: false,
        }
    }

    pub fn manual(
        label: impl Into<String>,
        why: impl Into<String>,
        commands: Vec<String>,
    ) -> Self {
        Self {
            label: label.into(),
            action: Action::Manual {
                why: why.into(),
                commands,
            },
            optional: false,
        }
    }

    pub fn skipped(label: impl Into<String>, why: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            action: Action::Skipped(why.into()),
            optional: false,
        }
    }

    pub fn optional(mut self) -> Self {
        self.optional = true;
        self
    }
}

pub struct Plan {
    pub title: String,
    pub steps: Vec<Step>,
}

/// Outcome of executing a plan, so the caller can set an exit code.
pub struct PlanOutcome {
    pub ran: usize,
    pub failed: Vec<String>,
    /// Required steps that a human still has to do.
    pub blocked: Vec<String>,
}

impl PlanOutcome {
    pub fn ok(&self) -> bool {
        self.failed.is_empty() && self.blocked.is_empty()
    }
}

const OK: &str = "\x1b[32m[ok]\x1b[0m";
const RUN: &str = "\x1b[33m[run]\x1b[0m";
const MAN: &str = "\x1b[35m[you]\x1b[0m";
const SKIP: &str = "\x1b[90m[skip]\x1b[0m";
const ERR: &str = "\x1b[31m[fail]\x1b[0m";

impl Plan {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            steps: Vec::new(),
        }
    }

    pub fn push(&mut self, step: Step) {
        self.steps.push(step);
    }

    pub fn runnable(&self) -> usize {
        self.steps
            .iter()
            .filter(|s| matches!(s.action, Action::Run(_)))
            .count()
    }

    /// Required steps a human has to perform.
    pub fn blocking_manual(&self) -> Vec<&Step> {
        self.steps
            .iter()
            .filter(|s| !s.optional && matches!(s.action, Action::Manual { .. }))
            .collect()
    }

    pub fn print(&self) {
        println!("\n\x1b[1m==> {}\x1b[0m\n", self.title);
        for step in &self.steps {
            let tail = if step.optional { " (optional)" } else { "" };
            match &step.action {
                Action::Satisfied(how) => {
                    println!("  {OK}   {}{tail}", step.label);
                    if !how.is_empty() {
                        println!("         \x1b[90m{how}\x1b[0m");
                    }
                }
                Action::Run(cmd) => {
                    println!("  {RUN}  {}{tail}", step.label);
                    println!("         \x1b[90m{}\x1b[0m", cmd.display());
                }
                Action::Manual { why, commands } => {
                    println!("  {MAN}  {}{tail}", step.label);
                    println!("         \x1b[90m{why}\x1b[0m");
                    for c in commands {
                        println!("         \x1b[90m$ {c}\x1b[0m");
                    }
                }
                Action::Skipped(why) => {
                    println!("  {SKIP} {}{tail}", step.label);
                    println!("         \x1b[90m{why}\x1b[0m");
                }
            }
        }
        println!();
    }

    /// Print, confirm, then run the `Run` steps in order.
    ///
    /// `assume_yes` skips the prompt. Without it and without a terminal we
    /// refuse rather than install software nobody asked for in that moment —
    /// a CI job should be explicit.
    pub fn execute(&self, assume_yes: bool, dry_run: bool) -> anyhow::Result<PlanOutcome> {
        self.print();

        let mut outcome = PlanOutcome {
            ran: 0,
            failed: Vec::new(),
            blocked: self
                .blocking_manual()
                .iter()
                .map(|s| s.label.clone())
                .collect(),
        };

        let todo = self.runnable();
        if dry_run {
            println!("--dry-run: {todo} command(s) not executed.");
            return Ok(outcome);
        }
        if todo == 0 {
            println!("Nothing to install.");
            return Ok(outcome);
        }

        if !assume_yes {
            if !std::io::stdin().is_terminal() {
                anyhow::bail!(
                    "{todo} command(s) would run, but stdin is not a terminal. \
                     Re-run with --yes to install unattended, or --dry-run to preview."
                );
            }
            print!("Run these {todo} command(s)? [y/N] ");
            use std::io::Write as _;
            let _ = std::io::stdout().flush();
            let mut answer = String::new();
            std::io::stdin().read_line(&mut answer)?;
            if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
                println!("Aborted.");
                return Ok(outcome);
            }
        }

        for step in &self.steps {
            let Action::Run(cmd) = &step.action else {
                continue;
            };
            println!("\n\x1b[1m--> {}\x1b[0m", step.label);
            println!("    \x1b[90m{}\x1b[0m", cmd.display());
            match cmd.run() {
                Ok(()) => outcome.ran += 1,
                Err(e) => {
                    println!("  {ERR}  {}: {e}", step.label);
                    if !step.optional {
                        outcome.failed.push(step.label.clone());
                    }
                }
            }
        }
        Ok(outcome)
    }
}
