//! CSS Feature Skill Tree
//!
//! Organizes CSS features from fundamental to advanced, creating a dependency graph
//! that determines the order in which features should be verified.

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// A single feature in the skill tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillNode {
    /// Unique identifier
    pub id: String,
    /// Human-readable name
    pub name: String,
    /// Brief description
    pub description: String,
    /// Difficulty level (1-5)
    pub difficulty: u8,
    /// Dependencies - features that must be verified first
    pub depends_on: Vec<String>,
    /// W3C spec URLs
    pub spec_urls: Vec<String>,
    /// Specific spec sections to verify (e.g., "9.4.1", "10.3.3")
    #[serde(default)]
    pub spec_sections: Vec<String>,
    /// Keywords for extracting relevant paragraphs
    pub keywords: Vec<String>,
    /// Source files to review (whole files - use sparingly)
    pub source_files: Vec<String>,
    /// Specific functions/structs to extract (format: "file.rs::function_name")
    #[serde(default)]
    pub source_functions: Vec<String>,
    /// Whether this feature needs the text3 engine code
    #[serde(default)]
    pub needs_text_engine: bool,
    /// Verification status
    #[serde(default)]
    pub status: VerificationStatus,
    /// Found spec annotations in source code (// +spec:node-id:section:paragraph)
    #[serde(default)]
    pub found_annotations: Vec<SpecAnnotation>,
}

/// A spec annotation found in source code
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SpecAnnotation {
    /// The file where this annotation was found
    pub file: String,
    /// Line number
    pub line: usize,
    /// The spec section (e.g., "9.4.1")
    pub section: String,
    /// The paragraph number within the section
    pub paragraph: Option<usize>,
    /// The line of code following the annotation
    pub code_context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VerificationStatus {
    /// Not started - no work done yet
    NotStarted,
    /// Prompt has been built and saved to file
    PromptBuilt,
    /// Prompt has been sent to Gemini, response received
    PromptSent { needs_changes: bool },
    /// Implementation has been updated based on review
    Implemented,
    /// Fully verified and tested
    Verified,
}

impl Default for VerificationStatus {
    fn default() -> Self {
        VerificationStatus::NotStarted
    }
}

impl VerificationStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            VerificationStatus::NotStarted => "[ ]",
            VerificationStatus::PromptBuilt => "[P]",
            VerificationStatus::PromptSent { needs_changes: false } => "[S]",
            VerificationStatus::PromptSent { needs_changes: true } => "[!]",
            VerificationStatus::Implemented => "[I]",
            VerificationStatus::Verified => "[✓]",
        }
    }
    
    pub fn description(&self) -> &'static str {
        match self {
            VerificationStatus::NotStarted => "Not started",
            VerificationStatus::PromptBuilt => "Prompt built",
            VerificationStatus::PromptSent { needs_changes: false } => "Sent (OK)",
            VerificationStatus::PromptSent { needs_changes: true } => "Sent (needs changes)",
            VerificationStatus::Implemented => "Implemented",
            VerificationStatus::Verified => "Verified",
        }
    }
}

/// The complete skill tree
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTree {
    pub nodes: BTreeMap<String, SkillNode>,
}

impl Default for SkillTree {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillTree {
    pub fn new() -> Self {
        let mut nodes = BTreeMap::new();
        
        // ============================================================
        // TIER 1: Fundamentals (difficulty 1)
        // ============================================================
        
        nodes.insert("box-model".to_string(), SkillNode {
            id: "box-model".to_string(),
            name: "Box Model".to_string(),
            description: "Content, padding, border, margin calculation".to_string(),
            difficulty: 1,
            depends_on: vec![],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/box.html".to_string(),
            ],
            keywords: vec![
                "content edge", "padding edge", "border edge", "margin edge",
                "box dimensions", "margin", "padding", "border",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/geometry.rs".to_string(),
                "layout/src/solver3/getters.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec!["8.1".to_string(), "8.2".to_string(), "8.3".to_string(), "8.4".to_string()],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("containing-block".to_string(), SkillNode {
            id: "containing-block".to_string(),
            name: "Containing Block".to_string(),
            description: "Containing block establishment and resolution".to_string(),
            difficulty: 1,
            depends_on: vec!["box-model".to_string()],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visudet.html#containing-block-details".to_string(),
            ],
            keywords: vec![
                "containing block", "initial containing block", "viewport",
                "ancestor", "padding edge",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/mod.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("display-property".to_string(), SkillNode {
            id: "display-property".to_string(),
            name: "Display Property".to_string(),
            description: "Box generation and display types".to_string(),
            difficulty: 1,
            depends_on: vec!["box-model".to_string()],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visuren.html#display-prop".to_string(),
                "https://www.w3.org/TR/css-display-3/".to_string(),
            ],
            keywords: vec![
                "display", "block-level", "inline-level", "block box",
                "inline box", "principal box", "anonymous",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/layout_tree.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        // ============================================================
        // TIER 2: Normal Flow (difficulty 2)
        // ============================================================
        
        nodes.insert("block-formatting-context".to_string(), SkillNode {
            id: "block-formatting-context".to_string(),
            name: "Block Formatting Context".to_string(),
            description: "BFC establishment and block layout".to_string(),
            difficulty: 2,
            depends_on: vec![
                "box-model".to_string(),
                "containing-block".to_string(),
                "display-property".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visuren.html#block-formatting".to_string(),
            ],
            keywords: vec![
                "block formatting context", "block-level boxes", "vertical",
                "margin collapsing", "normal flow",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("margin-collapsing".to_string(), SkillNode {
            id: "margin-collapsing".to_string(),
            name: "Margin Collapsing".to_string(),
            description: "Vertical margin collapse rules".to_string(),
            difficulty: 2,
            depends_on: vec!["block-formatting-context".to_string()],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/box.html#collapsing-margins".to_string(),
            ],
            keywords: vec![
                "collapsing margins", "adjoining margins", "collapsed margin",
                "escape", "through", "clearance",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("inline-formatting-context".to_string(), SkillNode {
            id: "inline-formatting-context".to_string(),
            name: "Inline Formatting Context".to_string(),
            description: "IFC establishment and inline layout".to_string(),
            difficulty: 2,
            depends_on: vec![
                "box-model".to_string(),
                "display-property".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visuren.html#inline-formatting".to_string(),
            ],
            keywords: vec![
                "inline formatting context", "line box", "inline-level boxes",
                "horizontal", "vertical-align", "baseline",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: true,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("width-calculation".to_string(), SkillNode {
            id: "width-calculation".to_string(),
            name: "Width Calculation".to_string(),
            description: "Computing widths for different box types".to_string(),
            difficulty: 2,
            depends_on: vec![
                "box-model".to_string(),
                "containing-block".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visudet.html#Computing_widths_and_margins".to_string(),
            ],
            keywords: vec![
                "width", "margin-left", "margin-right", "auto",
                "shrink-to-fit", "constraint equation",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/sizing.rs".to_string(),
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("height-calculation".to_string(), SkillNode {
            id: "height-calculation".to_string(),
            name: "Height Calculation".to_string(),
            description: "Computing heights for different box types".to_string(),
            difficulty: 2,
            depends_on: vec![
                "box-model".to_string(),
                "containing-block".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visudet.html#Computing_heights_and_margins".to_string(),
            ],
            keywords: vec![
                "height", "margin-top", "margin-bottom", "auto heights",
                "content height",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/sizing.rs".to_string(),
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        // ============================================================
        // TIER 3: Text & White-space (difficulty 3)
        // ============================================================
        
        nodes.insert("white-space-processing".to_string(), SkillNode {
            id: "white-space-processing".to_string(),
            name: "White Space Processing".to_string(),
            description: "Collapsing, trimming, and segment break handling".to_string(),
            difficulty: 3,
            depends_on: vec!["inline-formatting-context".to_string()],
            spec_urls: vec![
                "https://www.w3.org/TR/css-text-3/#white-space-processing".to_string(),
            ],
            keywords: vec![
                "white-space", "collapsible", "segment break", "phase I",
                "phase II", "trimming", "preserved",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
                "layout/src/text3/default.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: true,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("line-breaking".to_string(), SkillNode {
            id: "line-breaking".to_string(),
            name: "Line Breaking".to_string(),
            description: "Soft wrap opportunities and line break rules".to_string(),
            difficulty: 3,
            depends_on: vec![
                "inline-formatting-context".to_string(),
                "white-space-processing".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/css-text-3/#line-breaking".to_string(),
            ],
            keywords: vec![
                "line break", "soft wrap", "word-break", "overflow-wrap",
                "hyphenation", "forced line break",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/text3/knuth_plass.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: true,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("line-height".to_string(), SkillNode {
            id: "line-height".to_string(),
            name: "Line Height & Vertical Align".to_string(),
            description: "Leading, half-leading, and vertical alignment".to_string(),
            difficulty: 3,
            depends_on: vec!["inline-formatting-context".to_string()],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visudet.html#line-height".to_string(),
            ],
            keywords: vec![
                "line-height", "vertical-align", "leading", "half-leading",
                "baseline", "strut", "line box height",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/text3/glyphs.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: true,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("intrinsic-sizing".to_string(), SkillNode {
            id: "intrinsic-sizing".to_string(),
            name: "Intrinsic Sizing".to_string(),
            description: "Min-content, max-content, and fit-content".to_string(),
            difficulty: 3,
            depends_on: vec![
                "width-calculation".to_string(),
                "height-calculation".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/css-sizing-3/".to_string(),
            ],
            keywords: vec![
                "min-content", "max-content", "fit-content", "intrinsic size",
                "extrinsic size", "automatic size",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/sizing.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        // ============================================================
        // TIER 4: Positioning & Floats (difficulty 3-4)
        // ============================================================
        
        nodes.insert("floats".to_string(), SkillNode {
            id: "floats".to_string(),
            name: "Floats".to_string(),
            description: "Float positioning and flow interaction".to_string(),
            difficulty: 4,
            depends_on: vec![
                "block-formatting-context".to_string(),
                "width-calculation".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visuren.html#floats".to_string(),
            ],
            keywords: vec![
                "float", "left", "right", "clear", "clearance",
                "line box shortening", "margin box",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("positioning".to_string(), SkillNode {
            id: "positioning".to_string(),
            name: "Positioning".to_string(),
            description: "Relative, absolute, fixed positioning".to_string(),
            difficulty: 4,
            depends_on: vec![
                "containing-block".to_string(),
                "block-formatting-context".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visuren.html#positioning-scheme".to_string(),
            ],
            keywords: vec![
                "position", "relative", "absolute", "fixed", "static position",
                "offset", "top", "right", "bottom", "left",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
                "layout/src/solver3/mod.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        nodes.insert("inline-block".to_string(), SkillNode {
            id: "inline-block".to_string(),
            name: "Inline-Block".to_string(),
            description: "Inline-block box generation and layout".to_string(),
            difficulty: 3,
            depends_on: vec![
                "block-formatting-context".to_string(),
                "inline-formatting-context".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/visuren.html#inline-boxes".to_string(),
            ],
            keywords: vec![
                "inline-block", "atomic inline", "block container",
                "inline-level block container",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
                "layout/src/solver3/layout_tree.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        // NOTE: Flexbox and Grid are excluded - handled by taffy library (well-tested)
        
        // ============================================================
        // TIER 5: Tables (difficulty 5)
        // ============================================================
        
        nodes.insert("table-layout".to_string(), SkillNode {
            id: "table-layout".to_string(),
            name: "Table Layout".to_string(),
            description: "Table formatting context and layout algorithm".to_string(),
            difficulty: 5,
            depends_on: vec![
                "block-formatting-context".to_string(),
                "intrinsic-sizing".to_string(),
            ],
            spec_urls: vec![
                "https://www.w3.org/TR/CSS22/tables.html".to_string(),
            ],
            keywords: vec![
                "table", "table-row", "table-cell", "table-column",
                "caption", "border-collapse", "border-spacing",
            ].into_iter().map(String::from).collect(),
            source_files: vec![
                "layout/src/solver3/fc.rs".to_string(),
            ],
            source_functions: vec![],
            needs_text_engine: false,
            spec_sections: vec![],
            status: VerificationStatus::NotStarted,
            found_annotations: vec![],
        });
        
        Self { nodes }
    }
    
    /// Get nodes in dependency order (topological sort)
    pub fn get_ordered_nodes(&self) -> Vec<&SkillNode> {
        let mut result = Vec::new();
        let mut visited = std::collections::HashSet::new();
        
        fn visit<'a>(
            node_id: &str,
            nodes: &'a BTreeMap<String, SkillNode>,
            visited: &mut std::collections::HashSet<String>,
            result: &mut Vec<&'a SkillNode>,
        ) {
            if visited.contains(node_id) {
                return;
            }
            if let Some(node) = nodes.get(node_id) {
                for dep in &node.depends_on {
                    visit(dep, nodes, visited, result);
                }
                visited.insert(node_id.to_string());
                result.push(node);
            }
        }
        
        for node_id in self.nodes.keys() {
            visit(node_id, &self.nodes, &mut visited, &mut result);
        }
        
        result
    }
    
    /// Get the next unverified feature (respecting dependencies)
    pub fn get_next_unverified(&self) -> Option<&SkillNode> {
        // Priority: PromptSent that needs changes > PromptBuilt > NotStarted
        
        // First: Find PromptSent that needs changes (work in progress)
        for node in self.get_ordered_nodes() {
            if matches!(node.status, VerificationStatus::PromptSent { needs_changes: true }) {
                return Some(node);
            }
        }
        
        // Second: Find PromptBuilt (ready to send)
        for node in self.get_ordered_nodes() {
            if node.status == VerificationStatus::PromptBuilt {
                return Some(node);
            }
        }
        
        // Third: Find NotStarted with verified dependencies
        for node in self.get_ordered_nodes() {
            if node.status == VerificationStatus::NotStarted {
                let deps_ready = node.depends_on.iter().all(|dep_id| {
                    self.nodes.get(dep_id)
                        .map(|n| matches!(n.status, 
                            VerificationStatus::Verified | 
                            VerificationStatus::Implemented |
                            VerificationStatus::PromptSent { needs_changes: false }
                        ))
                        .unwrap_or(true)
                });
                if deps_ready {
                    return Some(node);
                }
            }
        }
        None
    }
    
    /// Print the skill tree as ASCII art
    pub fn print_tree(&self) {
        println!("CSS Layout Verification Skill Tree");
        println!("===================================\n");
        
        println!("Legend:");
        println!("  [ ] Not started     - No work done yet");
        println!("  [P] Prompt built    - Review prompt generated, ready to send");
        println!("  [S] Sent (OK)       - Gemini reviewed, implementation looks correct");
        println!("  [!] Needs changes   - Gemini found issues that need fixing");
        println!("  [I] Implemented     - Changes applied based on review");
        println!("  [✓] Verified        - Fully tested and W3C compliant\n");
        
        println!("Usage: spec extract <id> | spec review <id> | spec send <id>\n");
        
        for difficulty in 1..=5 {
            let mut nodes: Vec<_> = self.nodes.values()
                .filter(|n| n.difficulty == difficulty)
                .collect();
            nodes.sort_by_key(|n| &n.id);
            
            if nodes.is_empty() {
                continue;
            }
            
            println!("TIER {} (Difficulty {})", difficulty, difficulty);
            println!("{}", "-".repeat(80));
            
            for node in nodes {
                // Main line: status, ID, name
                println!("  {} {:30} {}", 
                    node.status.icon(),
                    node.id,
                    node.name
                );
                
                // Spec sections if available
                if !node.spec_sections.is_empty() {
                    println!("      Sections: {}", node.spec_sections.join(", "));
                }
                
                // Dependencies
                if !node.depends_on.is_empty() {
                    println!("      Depends:  {}", node.depends_on.join(", "));
                }
                
                // Source files (shortened)
                if !node.source_files.is_empty() {
                    let files: Vec<_> = node.source_files.iter()
                        .map(|f| f.rsplit('/').next().unwrap_or(f))
                        .collect();
                    println!("      Files:    {}", files.join(", "));
                }
                
                println!();
            }
        }
        
        // Summary
        let total = self.nodes.len();
        let not_started = self.nodes.values().filter(|n| n.status == VerificationStatus::NotStarted).count();
        let prompt_built = self.nodes.values().filter(|n| n.status == VerificationStatus::PromptBuilt).count();
        let prompt_sent = self.nodes.values().filter(|n| matches!(n.status, VerificationStatus::PromptSent { .. })).count();
        let implemented = self.nodes.values().filter(|n| n.status == VerificationStatus::Implemented).count();
        let verified = self.nodes.values().filter(|n| n.status == VerificationStatus::Verified).count();
        
        println!("Progress Summary");
        println!("{}", "-".repeat(40));
        println!("  [ ] Not started:    {:>3} / {}", not_started, total);
        println!("  [P] Prompt built:   {:>3} / {}", prompt_built, total);
        println!("  [S] Prompt sent:    {:>3} / {}", prompt_sent, total);
        println!("  [I] Implemented:    {:>3} / {}", implemented, total);
        println!("  [✓] Verified:       {:>3} / {}", verified, total);
        println!();
        
        if let Some(next) = self.get_next_unverified() {
            println!("Next to work on: {} ({})", next.id, next.name);
            println!("  Run: azul-doc spec review {}", next.id);
        }
    }
    
    /// Save skill tree to JSON file
    pub fn save(&self, path: &std::path::Path) -> std::io::Result<()> {
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }
    
    /// Load skill tree from JSON file
    pub fn load(path: &std::path::Path) -> std::io::Result<Self> {
        let json = std::fs::read_to_string(path)?;
        serde_json::from_str(&json)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
    }
}
