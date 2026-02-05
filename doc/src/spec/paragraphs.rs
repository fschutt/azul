//! W3C Spec Paragraph Registry
//!
//! Maps spec sections to grep-able IDs for source code annotations.
//! 
//! ## Annotation Format
//! 
//! In source code, use comments like:
//! ```
//! // +spec:css22-box-8.3.1-p1 - margin collapsing between siblings
//! ```
//! 
//! Format: `+spec:{spec-id}-{section}-p{paragraph}`

use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};

/// A single paragraph from a W3C spec
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecParagraph {
    /// Grep-able ID (e.g., "css22-box-8.3.1-p1")
    pub id: String,
    /// Human-readable title
    pub title: String,
    /// Brief description of what this paragraph specifies
    pub description: String,
    /// The spec file this comes from
    pub spec_file: String,
    /// Section number (e.g., "8.3.1")
    pub section: String,
    /// Paragraph number within section
    pub paragraph: usize,
}

/// Registry of all known spec paragraphs
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ParagraphRegistry {
    pub paragraphs: BTreeMap<String, SpecParagraph>,
}

impl ParagraphRegistry {
    pub fn new() -> Self {
        let mut reg = Self::default();
        
        // ================================================================
        // CSS 2.2 Box Model (Chapter 8)
        // https://www.w3.org/TR/CSS22/box.html
        // ================================================================
        
        reg.add("css22-box-8.1-p1", "Box dimensions", 
            "Content area, padding, border, margin areas",
            "css22-box.html", "8.1", 1);
        
        reg.add("css22-box-8.3.1-p1", "Collapsing margins - adjoining", 
            "Vertical margins collapse between adjacent block boxes",
            "css22-box.html", "8.3.1", 1);
        
        reg.add("css22-box-8.3.1-p2", "Collapsing margins - parent/child", 
            "Parent and first/last child margins collapse if no separation",
            "css22-box.html", "8.3.1", 2);
        
        reg.add("css22-box-8.3.1-p3", "Collapsing margins - empty boxes", 
            "Empty box's own margins collapse",
            "css22-box.html", "8.3.1", 3);
        
        reg.add("css22-box-8.3.1-p4", "Collapsing margins - rules", 
            "Max of positive, min of negative, sum for mixed",
            "css22-box.html", "8.3.1", 4);
        
        reg.add("css22-box-8.3.1-p5", "Collapsing margins - prevention", 
            "Floats, abs pos, inline-blocks, overflow!=visible prevent collapse",
            "css22-box.html", "8.3.1", 5);
        
        // ================================================================
        // CSS 2.2 Visual Formatting Model (Chapter 9)
        // https://www.w3.org/TR/CSS22/visuren.html
        // ================================================================
        
        reg.add("css22-visuren-9.2.1-p1", "Block-level elements", 
            "Elements with display:block/list-item/table generate block boxes",
            "css22-visuren.html", "9.2.1", 1);
        
        reg.add("css22-visuren-9.2.1.1-p1", "Anonymous block boxes", 
            "Inline content in block container wrapped in anonymous block",
            "css22-visuren.html", "9.2.1.1", 1);
        
        reg.add("css22-visuren-9.2.2-p1", "Inline-level elements", 
            "Elements with display:inline/inline-block/inline-table",
            "css22-visuren.html", "9.2.2", 1);
        
        reg.add("css22-visuren-9.2.3-p1", "Run-in boxes", 
            "Run-in box may become inline or block based on context",
            "css22-visuren.html", "9.2.3", 1);
        
        reg.add("css22-visuren-9.4.1-p1", "BFC establishment", 
            "Floats, abs pos, inline-blocks, overflow!=visible establish BFC",
            "css22-visuren.html", "9.4.1", 1);
        
        reg.add("css22-visuren-9.4.1-p2", "BFC layout", 
            "Boxes laid out vertically, margins collapse",
            "css22-visuren.html", "9.4.1", 2);
        
        reg.add("css22-visuren-9.4.2-p1", "IFC establishment", 
            "IFC established when block container has only inline-level content",
            "css22-visuren.html", "9.4.2", 1);
        
        reg.add("css22-visuren-9.4.2-p2", "Line boxes", 
            "Inline boxes laid out in line boxes",
            "css22-visuren.html", "9.4.2", 2);
        
        reg.add("css22-visuren-9.4.2-p3", "Line box width", 
            "Line box width determined by containing block and floats",
            "css22-visuren.html", "9.4.2", 3);
        
        reg.add("css22-visuren-9.4.3-p1", "Relative positioning", 
            "Box offset from normal flow position",
            "css22-visuren.html", "9.4.3", 1);
        
        reg.add("css22-visuren-9.5-p1", "Float positioning", 
            "Float shifted to left or right of line box",
            "css22-visuren.html", "9.5", 1);
        
        reg.add("css22-visuren-9.5-p2", "Float stacking", 
            "Float may not overlap other floats",
            "css22-visuren.html", "9.5", 2);
        
        reg.add("css22-visuren-9.5.1-p1", "Float rules 1-3", 
            "Float positioned rules (left edge, no higher, etc.)",
            "css22-visuren.html", "9.5.1", 1);
        
        reg.add("css22-visuren-9.5.2-p1", "Clear property", 
            "Clear moves element below preceding floats",
            "css22-visuren.html", "9.5.2", 1);
        
        reg.add("css22-visuren-9.6.1-p1", "Absolute positioning", 
            "Box removed from flow, positioned relative to containing block",
            "css22-visuren.html", "9.6.1", 1);
        
        reg.add("css22-visuren-9.7-p1", "Display-position-float", 
            "Interaction between display, position, and float",
            "css22-visuren.html", "9.7", 1);
        
        // ================================================================
        // CSS 2.2 Visual Formatting Model Details (Chapter 10)
        // https://www.w3.org/TR/CSS22/visudet.html
        // ================================================================
        
        reg.add("css22-visudet-10.1-p1", "Containing block definition", 
            "Rules for determining containing block",
            "css22-visudet.html", "10.1", 1);
        
        reg.add("css22-visudet-10.2-p1", "Content width", 
            "Width property applies to block-level and replaced elements",
            "css22-visudet.html", "10.2", 1);
        
        reg.add("css22-visudet-10.3.1-p1", "Inline non-replaced width", 
            "Width property does not apply",
            "css22-visudet.html", "10.3.1", 1);
        
        reg.add("css22-visudet-10.3.2-p1", "Inline replaced width", 
            "Intrinsic width if width:auto",
            "css22-visudet.html", "10.3.2", 1);
        
        reg.add("css22-visudet-10.3.3-p1", "Block non-replaced width", 
            "Constraint: margin-left + border-left + padding-left + width + ... = containing block width",
            "css22-visudet.html", "10.3.3", 1);
        
        reg.add("css22-visudet-10.3.3-p2", "Block width auto resolution", 
            "If width:auto, other autos become 0, width fills remaining",
            "css22-visudet.html", "10.3.3", 2);
        
        reg.add("css22-visudet-10.3.4-p1", "Block replaced width", 
            "Width calculation for replaced block elements",
            "css22-visudet.html", "10.3.4", 1);
        
        reg.add("css22-visudet-10.3.5-p1", "Float non-replaced width", 
            "Shrink-to-fit width",
            "css22-visudet.html", "10.3.5", 1);
        
        reg.add("css22-visudet-10.3.7-p1", "Abs pos non-replaced width", 
            "Constraint equation for absolutely positioned elements",
            "css22-visudet.html", "10.3.7", 1);
        
        reg.add("css22-visudet-10.3.8-p1", "Abs pos replaced width", 
            "Width calculation for abs pos replaced elements",
            "css22-visudet.html", "10.3.8", 1);
        
        reg.add("css22-visudet-10.3.9-p1", "Inline-block non-replaced width", 
            "Shrink-to-fit width for inline-blocks",
            "css22-visudet.html", "10.3.9", 1);
        
        reg.add("css22-visudet-10.4-p1", "Min/max width", 
            "Tentative width clamped by min-width and max-width",
            "css22-visudet.html", "10.4", 1);
        
        reg.add("css22-visudet-10.5-p1", "Content height", 
            "Height property applies to block-level and replaced elements",
            "css22-visudet.html", "10.5", 1);
        
        reg.add("css22-visudet-10.6.1-p1", "Inline non-replaced height", 
            "Line-height used, height property ignored",
            "css22-visudet.html", "10.6.1", 1);
        
        reg.add("css22-visudet-10.6.2-p1", "Inline replaced height", 
            "Intrinsic height if height:auto",
            "css22-visudet.html", "10.6.2", 1);
        
        reg.add("css22-visudet-10.6.3-p1", "Block non-replaced height", 
            "Height of content if height:auto (normal flow)",
            "css22-visudet.html", "10.6.3", 1);
        
        reg.add("css22-visudet-10.6.4-p1", "Abs pos non-replaced height", 
            "Constraint equation for absolutely positioned height",
            "css22-visudet.html", "10.6.4", 1);
        
        reg.add("css22-visudet-10.6.7-p1", "Auto heights for BFC roots", 
            "Height includes floats for BFC roots",
            "css22-visudet.html", "10.6.7", 1);
        
        reg.add("css22-visudet-10.7-p1", "Min/max height", 
            "Tentative height clamped by min-height and max-height",
            "css22-visudet.html", "10.7", 1);
        
        reg.add("css22-visudet-10.8-p1", "Line height calculation", 
            "CSS 2.2 line height and leading model",
            "css22-visudet.html", "10.8", 1);
        
        reg.add("css22-visudet-10.8.1-p1", "Leading and half-leading", 
            "Difference between line-height and font-size",
            "css22-visudet.html", "10.8.1", 1);
        
        // ================================================================
        // CSS Text Level 3
        // https://www.w3.org/TR/css-text-3/
        // ================================================================
        
        reg.add("css-text-3-4.1-p1", "White-space phase I", 
            "Segment breaks and surrounding white space handling",
            "css-text-3.html", "4.1", 1);
        
        reg.add("css-text-3-4.1-p2", "White-space phase I rules", 
            "Segment break transformation rules",
            "css-text-3.html", "4.1", 2);
        
        reg.add("css-text-3-4.1.1-p1", "White-space phase I details", 
            "Tab, segment break, and space collapsing",
            "css-text-3.html", "4.1.1", 1);
        
        reg.add("css-text-3-4.1.2-p1", "White-space phase II", 
            "Space/tab collapsing after Phase I",
            "css-text-3.html", "4.1.2", 1);
        
        reg.add("css-text-3-4.1.3-p1", "White-space trimming", 
            "Collapsible spaces at line start/end removed",
            "css-text-3.html", "4.1.3", 1);
        
        reg.add("css-text-3-5.1-p1", "Line breaking basics", 
            "Soft wrap opportunities and word-break",
            "css-text-3.html", "5.1", 1);
        
        reg.add("css-text-3-5.2-p1", "Word-break property", 
            "Controls line break opportunities within words",
            "css-text-3.html", "5.2", 1);
        
        reg.add("css-text-3-5.5-p1", "Overflow-wrap property", 
            "Break unbreakable strings if necessary",
            "css-text-3.html", "5.5", 1);
        
        // ================================================================
        // CSS Sizing Level 3
        // https://www.w3.org/TR/css-sizing-3/
        // ================================================================
        
        reg.add("css-sizing-3-4.1-p1", "Min-content size", 
            "Smallest size without overflow",
            "css-sizing-3.html", "4.1", 1);
        
        reg.add("css-sizing-3-4.2-p1", "Max-content size", 
            "Smallest size with no soft wrapping",
            "css-sizing-3.html", "4.2", 1);
        
        reg.add("css-sizing-3-4.3-p1", "Fit-content size", 
            "min(max-content, max(min-content, stretch))",
            "css-sizing-3.html", "4.3", 1);
        
        reg.add("css-sizing-3-5.1-p1", "Intrinsic size of replaced", 
            "Natural width/height of replaced elements",
            "css-sizing-3.html", "5.1", 1);
        
        reg
    }
    
    fn add(&mut self, id: &str, title: &str, desc: &str, file: &str, section: &str, para: usize) {
        self.paragraphs.insert(id.to_string(), SpecParagraph {
            id: id.to_string(),
            title: title.to_string(),
            description: desc.to_string(),
            spec_file: file.to_string(),
            section: section.to_string(),
            paragraph: para,
        });
    }
    
    /// Get a paragraph by ID
    pub fn get(&self, id: &str) -> Option<&SpecParagraph> {
        self.paragraphs.get(id)
    }
    
    /// Get all paragraphs for a section prefix (e.g., "css22-box-8.3")
    pub fn get_by_section(&self, prefix: &str) -> Vec<&SpecParagraph> {
        self.paragraphs.values()
            .filter(|p| p.id.starts_with(prefix))
            .collect()
    }
    
    /// Search for annotations in source files and return found locations
    pub fn find_annotations_in_file(&self, content: &str, file_path: &str) -> Vec<(String, usize, String)> {
        let mut found = Vec::new();
        
        for (line_num, line) in content.lines().enumerate() {
            if let Some(start) = line.find("+spec:") {
                // Extract the spec ID
                let rest = &line[start + 6..];
                let end = rest.find(|c: char| c.is_whitespace() || c == '-' && rest[..rest.find(c).unwrap_or(rest.len())].matches('-').count() >= 3)
                    .unwrap_or(rest.len());
                
                // Find the full ID (up to the description separator)
                let full_rest = &line[start + 6..];
                let id_end = full_rest.find(" -").unwrap_or(full_rest.find(char::is_whitespace).unwrap_or(full_rest.len()));
                let spec_id = full_rest[..id_end].trim().to_string();
                
                if !spec_id.is_empty() {
                    found.push((spec_id, line_num + 1, line.trim().to_string()));
                }
            }
        }
        
        found
    }
    
    /// Print all paragraphs as a reference
    pub fn print_all(&self) {
        println!("W3C Spec Paragraph Registry");
        println!("============================\n");
        println!("Use in source code: // +spec:<id> - <description>\n");
        
        let mut current_file = String::new();
        
        for para in self.paragraphs.values() {
            if para.spec_file != current_file {
                current_file = para.spec_file.clone();
                println!("\n## {}\n", current_file);
            }
            
            println!("  {} - {}", para.id, para.title);
            println!("      {}", para.description);
        }
    }
}

/// Scan source files for +spec: annotations
pub fn scan_source_for_annotations(
    azul_root: &std::path::Path,
) -> std::collections::BTreeMap<String, Vec<(String, usize, String)>> {
    let mut results = std::collections::BTreeMap::new();
    let registry = ParagraphRegistry::new();
    
    let dirs_to_scan = [
        "layout/src/solver3",
        "layout/src/text3",
    ];
    
    for dir in dirs_to_scan {
        let dir_path = azul_root.join(dir);
        if !dir_path.exists() {
            continue;
        }
        
        if let Ok(entries) = std::fs::read_dir(&dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map(|e| e == "rs").unwrap_or(false) {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        let rel_path = path.strip_prefix(azul_root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .to_string();
                        
                        let annotations = registry.find_annotations_in_file(&content, &rel_path);
                        if !annotations.is_empty() {
                            results.insert(rel_path, annotations);
                        }
                    }
                }
            }
        }
    }
    
    results
}
