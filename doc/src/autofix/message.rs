/// Message recording system for autofix operations
use std::fmt;

use super::workspace::TypeOrigin;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageLevel {
    Info,
    Warning,
    Error,
}

/// Typed message variants for autofix operations
#[derive(Debug, Clone)]
pub enum AutofixMessage {
    // Info-level: Successful operations
    TypeDiscovered {
        type_name: String,
        path: String,
        reason: TypeOrigin,
    },
    PathChanged {
        type_name: String,
        old_path: String,
        new_path: String,
    },
    IterationStarted {
        iteration: usize,
        count: usize,
    },
    IterationComplete {
        iteration: usize,
    },
    VirtualPatchApplied {
        count: usize,
    },

    // Warning-level: Non-fatal issues
    TypeSkipped {
        type_name: String,
        reason: SkipReason,
    },
    TypeNotFound {
        type_name: String,
    },
    MaxIterationsReached {
        iteration: usize,
    },
    GenericWarning {
        message: String,
    },

    // Error-level: Fatal issues
    WorkspaceIndexFailed {
        path: String,
        error: String,
    },
    PatchGenerationFailed {
        type_name: String,
        error: String,
    },
}

impl AutofixMessage {
    pub fn level(&self) -> MessageLevel {
        match self {
            Self::TypeDiscovered { .. }
            | Self::PathChanged { .. }
            | Self::IterationStarted { .. }
            | Self::IterationComplete { .. }
            | Self::VirtualPatchApplied { .. } => MessageLevel::Info,

            Self::TypeSkipped { .. }
            | Self::TypeNotFound { .. }
            | Self::MaxIterationsReached { .. }
            | Self::GenericWarning { .. } => MessageLevel::Warning,

            Self::WorkspaceIndexFailed { .. } | Self::PatchGenerationFailed { .. } => {
                MessageLevel::Error
            }
        }
    }
}

impl fmt::Display for AutofixMessage {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::TypeDiscovered {
                type_name,
                path,
                reason,
            } => {
                write!(
                    f,
                    "OK: Discovered {}\n  Path: {}\n  Reason: {}",
                    type_name, path, reason
                )
            }
            Self::PathChanged {
                type_name,
                old_path,
                new_path,
            } => {
                write!(
                    f,
                    "INFO: Path changed for {}\n  {} → {}",
                    type_name, old_path, new_path
                )
            }
            Self::IterationStarted { iteration, count } => {
                write!(
                    f,
                    "INFO: Iteration {}: {} types to discover",
                    iteration, count
                )
            }
            Self::IterationComplete { iteration } => {
                write!(f, "OK: Iteration {} complete", iteration)
            }
            Self::VirtualPatchApplied { count } => {
                write!(f, "OK: Applied {} virtual patches", count)
            }
            Self::TypeSkipped { type_name, reason } => {
                write!(f, "WARN: Skipped {}: {}", type_name, reason)
            }
            Self::TypeNotFound { type_name } => {
                write!(f, "WARN: Could not find type: {}", type_name)
            }
            Self::MaxIterationsReached { iteration } => {
                write!(f, "WARN:  Reached maximum iteration limit ({})", iteration)
            }
            Self::GenericWarning { message } => {
                write!(f, "WARN:  {}", message)
            }
            Self::WorkspaceIndexFailed { path, error } => {
                write!(f, "ERROR: Failed to index {}: {}", path, error)
            }
            Self::PatchGenerationFailed { type_name, error } => {
                write!(
                    f,
                    "ERROR: Failed to generate patch for {}: {}",
                    type_name, error
                )
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum SkipReason {
    ExternalCrate(String),
    MissingReprC,
    AlreadyInApi,
    CallbackTypedef,
    AlreadyVisited,
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::ExternalCrate(name) => write!(f, "external crate '{}'", name),
            Self::MissingReprC => write!(f, "missing #[repr(C)]"),
            Self::AlreadyInApi => write!(f, "already in API"),
            Self::CallbackTypedef => write!(f, "is callback typedef"),
            Self::AlreadyVisited => write!(f, "already visited (cycle)"),
        }
    }
}

#[derive(Debug, Default)]
pub struct AutofixMessages {
    messages: Vec<AutofixMessage>,
}

impl AutofixMessages {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, message: AutofixMessage) {
        self.messages.push(message);
    }

    pub fn get_messages(&self) -> &[AutofixMessage] {
        &self.messages
    }

    pub fn count_by_level(&self) -> (usize, usize, usize) {
        let mut info = 0;
        let mut warnings = 0;
        let mut errors = 0;

        for msg in &self.messages {
            match msg.level() {
                MessageLevel::Info => info += 1,
                MessageLevel::Warning => warnings += 1,
                MessageLevel::Error => errors += 1,
            }
        }

        (info, warnings, errors)
    }

    /// Get messages of a specific level
    pub fn messages_by_level(&self, level: MessageLevel) -> Vec<&AutofixMessage> {
        self.messages
            .iter()
            .filter(|m| m.level() == level)
            .collect()
    }

    /// Print comprehensive report after analysis
    pub fn print_report(
        &self,
        patch_summary: &PatchSummary,
        duration_secs: f32,
        patches_dir: &std::path::Path,
        patch_count: usize,
    ) {
        println!();
        println!("════════════════════════════════════════════════════════════════");
        println!("                        AUTOFIX SUMMARY                          ");
        println!("════════════════════════════════════════════════════════════════");
        println!();

        // Statistics
        let (info_count, warning_count, error_count) = self.count_by_level();
        let discoveries: Vec<_> = self
            .messages
            .iter()
            .filter_map(|m| match m {
                AutofixMessage::TypeDiscovered { .. } => Some(m),
                _ => None,
            })
            .collect();

        println!("Completed in {:.1}s", duration_secs);
        println!("{} patches generated", patch_count);
        if warning_count > 0 || error_count > 0 {
            println!("{} warnings, {} errors", warning_count, error_count);
        }
        println!();

        // Discovered types (new types found that need to be added to api.json)
        if !discoveries.is_empty() {
            println!("NEW TYPES ({}):", discoveries.len());
            for msg in &discoveries {
                if let AutofixMessage::TypeDiscovered {
                    type_name,
                    path,
                    reason,
                } = msg
                {
                    let location = if path.is_empty() { "?" } else { path.as_str() };
                    println!("   {} -> {} ({})", type_name, location, reason);
                }
            }
            println!();
        }

        // Path corrections (existing types with wrong paths in api.json)
        if !patch_summary.external_path_changes.is_empty() {
            // Deduplicate changes (same class may appear multiple times)
            let mut seen = std::collections::HashSet::new();
            let unique_changes: Vec<_> = patch_summary
                .external_path_changes
                .iter()
                .filter(|c| seen.insert((&c.class_name, &c.old_path, &c.new_path)))
                .collect();

            println!("PATH CORRECTIONS ({}):", unique_changes.len());

            // Only show first 15 unique changes
            for change in unique_changes.iter().take(15) {
                println!(
                    "   {} : {} -> {}",
                    change.class_name, change.old_path, change.new_path
                );
            }
            if unique_changes.len() > 15 {
                println!("   ... and {} more", unique_changes.len() - 15);
            }
            println!();
        }

        // Errors (always show)
        let errors = self.messages_by_level(MessageLevel::Error);
        if !errors.is_empty() {
            println!("ERRORS ({}):", errors.len());
            for msg in errors {
                println!("   {}", msg);
            }
            println!();
        }

        // Next steps
        if patch_count > 0 {
            println!("NEXT STEPS:");
            println!("   1. Review:  ls {}", patches_dir.display());
            println!(
                "   2. Apply:   cargo run -- patch {}",
                patches_dir.display()
            );
            println!("   3. Verify:  git diff api.json");
        } else {
            println!("No patches needed - API is up to date!");
        }
        println!();
    }
}

/// Summary of patch changes
#[derive(Debug, Default)]
pub struct PatchSummary {
    pub external_path_changes: Vec<ExternalPathChange>,
    pub documentation_changes: Vec<DocumentationChange>,
    pub field_changes: Vec<FieldChange>,
    pub classes_added: Vec<ClassAdded>,
}

#[derive(Debug)]
pub struct ExternalPathChange {
    pub class_name: String,
    pub module_name: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug)]
pub struct DocumentationChange {
    pub class_name: String,
    pub changed: bool,
}

#[derive(Debug)]
pub struct FieldChange {
    pub class_name: String,
    pub field_name: String,
    pub change_type: FieldChangeType,
}

#[derive(Debug)]
pub enum FieldChangeType {
    Added,
    Removed,
    TypeChanged { old_type: String, new_type: String },
}

#[derive(Debug)]
pub struct ClassAdded {
    pub class_name: String,
    pub module: String,
    pub external_path: String,
}

impl PatchSummary {
    pub fn print(&self) {
        println!("\nPatch Summary\n");

        if !self.external_path_changes.is_empty() {
            println!(
                "External Path Changes ({}):",
                self.external_path_changes.len()
            );
            for change in &self.external_path_changes {
                println!("  - {}", change.class_name);
                println!("    {} -> {}", change.old_path, change.new_path);
            }
            println!();
        }

        if !self.classes_added.is_empty() {
            println!("[ADD] Classes Added ({}):", self.classes_added.len());
            for added in &self.classes_added {
                println!("  - {}.{}", added.module, added.class_name);
                println!("    ({})", added.external_path);
            }
            println!();
        }

        if !self.field_changes.is_empty() {
            use std::collections::HashMap;

            println!("[FIX] Field Changes ({}):", self.field_changes.len());
            let mut by_class: HashMap<String, Vec<&FieldChange>> = HashMap::new();
            for change in &self.field_changes {
                by_class
                    .entry(change.class_name.clone())
                    .or_default()
                    .push(change);
            }

            for (class_name, changes) in by_class {
                println!("  - {}", class_name);
                for change in changes {
                    match &change.change_type {
                        FieldChangeType::Added => {
                            println!("    + {}", change.field_name);
                        }
                        FieldChangeType::Removed => {
                            println!("    - {}", change.field_name);
                        }
                        FieldChangeType::TypeChanged { old_type, new_type } => {
                            println!("    ~ {} : {} -> {}", change.field_name, old_type, new_type);
                        }
                    }
                }
            }
            println!();
        }

        if !self.documentation_changes.is_empty() {
            println!(
                "[NOTE] Documentation Changes ({}):",
                self.documentation_changes.len()
            );
            for change in &self.documentation_changes {
                println!("  - {}", change.class_name);
            }
            println!();
        }

        if self.is_empty() {
            println!("✨ No changes to apply - API is up to date!");
        }
    }

    pub fn is_empty(&self) -> bool {
        self.external_path_changes.is_empty()
            && self.documentation_changes.is_empty()
            && self.field_changes.is_empty()
            && self.classes_added.is_empty()
    }
}
