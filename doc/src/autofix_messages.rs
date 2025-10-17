/// Message recording system for autofix operations

#[derive(Debug, Clone)]
pub enum AutofixMessageLevel {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub struct AutofixMessage {
    pub level: AutofixMessageLevel,
    pub category: String,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct AutofixMessages {
    messages: Vec<AutofixMessage>,
}

impl AutofixMessages {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn info(&mut self, category: impl Into<String>, message: impl Into<String>) {
        self.messages.push(AutofixMessage {
            level: AutofixMessageLevel::Info,
            category: category.into(),
            message: message.into(),
        });
    }

    pub fn warning(&mut self, category: impl Into<String>, message: impl Into<String>) {
        self.messages.push(AutofixMessage {
            level: AutofixMessageLevel::Warning,
            category: category.into(),
            message: message.into(),
        });
    }

    #[allow(dead_code)]
    pub fn error(&mut self, category: impl Into<String>, message: impl Into<String>) {
        self.messages.push(AutofixMessage {
            level: AutofixMessageLevel::Error,
            category: category.into(),
            message: message.into(),
        });
    }

    /// Print only warnings and errors
    pub fn print_warnings_and_errors(&self) {
        for msg in &self.messages {
            match msg.level {
                AutofixMessageLevel::Warning => {
                    eprintln!("⚠️  [{}] {}", msg.category, msg.message);
                }
                AutofixMessageLevel::Error => {
                    eprintln!("❌ [{}] {}", msg.category, msg.message);
                }
                AutofixMessageLevel::Info => {
                    // Skip info messages in quiet mode
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn get_messages(&self) -> &[AutofixMessage] {
        &self.messages
    }

    pub fn count_by_level(&self) -> (usize, usize, usize) {
        let mut info = 0;
        let mut warnings = 0;
        let mut errors = 0;

        for msg in &self.messages {
            match msg.level {
                AutofixMessageLevel::Info => info += 1,
                AutofixMessageLevel::Warning => warnings += 1,
                AutofixMessageLevel::Error => errors += 1,
            }
        }

        (info, warnings, errors)
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
        println!("\n╔════════════════════════════════════════════════════════════════╗");
        println!("║                    Patch Summary                               ║");
        println!("╚════════════════════════════════════════════════════════════════╝\n");

        if !self.external_path_changes.is_empty() {
            println!(
                "📍 External Path Changes ({}):",
                self.external_path_changes.len()
            );
            for change in &self.external_path_changes {
                println!("  • {}", change.class_name);
                println!("    {} → {}", change.old_path, change.new_path);
            }
            println!();
        }

        if !self.classes_added.is_empty() {
            println!("➕ Classes Added ({}):", self.classes_added.len());
            for added in &self.classes_added {
                println!("  • {}.{}", added.module, added.class_name);
                println!("    ({})", added.external_path);
            }
            println!();
        }

        if !self.field_changes.is_empty() {
            use std::collections::HashMap;

            println!("🔧 Field Changes ({}):", self.field_changes.len());
            let mut by_class: HashMap<String, Vec<&FieldChange>> = HashMap::new();
            for change in &self.field_changes {
                by_class
                    .entry(change.class_name.clone())
                    .or_default()
                    .push(change);
            }

            for (class_name, changes) in by_class {
                println!("  • {}", class_name);
                for change in changes {
                    match &change.change_type {
                        FieldChangeType::Added => {
                            println!("    + {}", change.field_name);
                        }
                        FieldChangeType::Removed => {
                            println!("    - {}", change.field_name);
                        }
                        FieldChangeType::TypeChanged { old_type, new_type } => {
                            println!("    ~ {} : {} → {}", change.field_name, old_type, new_type);
                        }
                    }
                }
            }
            println!();
        }

        if !self.documentation_changes.is_empty() {
            println!(
                "📝 Documentation Changes ({}):",
                self.documentation_changes.len()
            );
            for change in &self.documentation_changes {
                println!("  • {}", change.class_name);
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
