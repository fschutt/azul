//! W3C Specification Paragraph Extractor
//!
//! Parses downloaded W3C spec HTML files and extracts relevant paragraphs
//! based on keywords. Uses simple HTML parsing without external dependencies.

use std::path::Path;

/// A single extracted paragraph with context
#[derive(Debug, Clone)]
pub struct ExtractedParagraph {
    /// The section heading this paragraph is under
    pub section: String,
    /// The section ID (for linking)
    pub section_id: Option<String>,
    /// The paragraph text (with HTML stripped)
    pub text: String,
    /// Keywords that matched
    pub matched_keywords: Vec<String>,
    /// Source file
    pub source_file: String,
    /// Approximate line number
    pub approx_line: usize,
}

/// Extract paragraphs from an HTML file that match given keywords
pub fn extract_paragraphs(
    html_path: &Path,
    keywords: &[String],
) -> Result<Vec<ExtractedParagraph>, String> {
    let html = std::fs::read_to_string(html_path)
        .map_err(|e| format!("Failed to read {}: {}", html_path.display(), e))?;
    
    let source_file = html_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    
    let mut results = Vec::new();
    let mut current_section = String::new();
    let mut current_section_id = None;
    
    // Split into lines for approximate line tracking
    let lines: Vec<&str> = html.lines().collect();
    
    for (line_num, line) in lines.iter().enumerate() {
        // Track section headings (h1-h6)
        if let Some(heading) = extract_heading(line) {
            current_section = heading.text.clone();
            current_section_id = heading.id;
        }
        
        // Extract paragraphs and definition lists
        if line.contains("<p") || line.contains("<dd") || line.contains("<li") {
            // Collect full paragraph (may span multiple lines)
            let para_text = collect_element(&lines, line_num);
            let plain_text = strip_html(&para_text);
            
            // Check for keyword matches
            let matched: Vec<String> = keywords
                .iter()
                .filter(|kw| {
                    plain_text.to_lowercase().contains(&kw.to_lowercase())
                })
                .cloned()
                .collect();
            
            if !matched.is_empty() && plain_text.len() > 20 {
                results.push(ExtractedParagraph {
                    section: current_section.clone(),
                    section_id: current_section_id.clone(),
                    text: plain_text,
                    matched_keywords: matched,
                    source_file: source_file.clone(),
                    approx_line: line_num + 1,
                });
            }
        }
    }
    
    // Deduplicate by text (same paragraph might match multiple times)
    results.dedup_by(|a, b| a.text == b.text);
    
    Ok(results)
}

struct Heading {
    text: String,
    id: Option<String>,
}

fn extract_heading(line: &str) -> Option<Heading> {
    // Match <h1> through <h6>
    for level in 1..=6 {
        let open_tag = format!("<h{}", level);
        if line.contains(&open_tag) {
            // Extract ID if present
            let id = extract_id_attr(line);
            
            // Extract text content
            let text = strip_html(line);
            if !text.is_empty() {
                return Some(Heading { text, id });
            }
        }
    }
    None
}

fn extract_id_attr(line: &str) -> Option<String> {
    // Simple extraction of id="..." or id='...'
    if let Some(start) = line.find("id=\"") {
        let rest = &line[start + 4..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    if let Some(start) = line.find("id='") {
        let rest = &line[start + 4..];
        if let Some(end) = rest.find('\'') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn collect_element(lines: &[&str], start_line: usize) -> String {
    // Simple approach: collect until we see closing tag or new element
    let mut result = String::new();
    let mut depth = 0;
    
    for line in &lines[start_line..] {
        result.push_str(line);
        result.push(' ');
        
        // Simple depth tracking
        depth += line.matches("<p").count() as i32;
        depth += line.matches("<dd").count() as i32;
        depth += line.matches("<li").count() as i32;
        depth -= line.matches("</p>").count() as i32;
        depth -= line.matches("</dd>").count() as i32;
        depth -= line.matches("</li>").count() as i32;
        
        if depth <= 0 || result.len() > 5000 {
            break;
        }
    }
    
    result
}

/// Strip HTML tags and decode common entities
pub fn strip_html(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut chars = html.chars().peekable();
    
    while let Some(c) = chars.next() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
            // Add space after tags to prevent word joining
            if !result.ends_with(' ') {
                result.push(' ');
            }
        } else if !in_tag {
            result.push(c);
        }
    }
    
    // Decode common HTML entities
    let result = result
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'");
    
    // Collapse whitespace
    let mut prev_space = false;
    result
        .chars()
        .filter_map(|c| {
            if c.is_whitespace() {
                if prev_space {
                    None
                } else {
                    prev_space = true;
                    Some(' ')
                }
            } else {
                prev_space = false;
                Some(c)
            }
        })
        .collect::<String>()
        .trim()
        .to_string()
}

/// Extract all paragraphs from multiple spec files for a skill node
pub fn extract_for_skill_node(
    node: &super::skill_tree::SkillNode,
    spec_dir: &Path,
) -> Result<Vec<ExtractedParagraph>, String> {
    let mut all_paragraphs = Vec::new();
    
    // Determine which spec files to search based on node's spec_urls
    let registry = super::downloader::SpecRegistry::new();
    
    for url in registry.get_all_urls() {
        let local_path = spec_dir.join(&url.local_filename);
        if local_path.exists() {
            match extract_paragraphs(&local_path, &node.keywords) {
                Ok(paragraphs) => {
                    all_paragraphs.extend(paragraphs);
                }
                Err(e) => {
                    eprintln!("Warning: {}", e);
                }
            }
        }
    }
    
    // Sort by relevance (number of matched keywords, then shorter text first
    // since shorter paragraphs tend to be more focused)
    all_paragraphs.sort_by(|a, b| {
        b.matched_keywords.len().cmp(&a.matched_keywords.len())
            .then(a.text.len().cmp(&b.text.len()))
    });

    // Deduplicate paragraphs with high text overlap (e.g., nested TOC entries
    // that produce near-identical paragraphs with slightly different prefixes)
    all_paragraphs = deduplicate_paragraphs(all_paragraphs, 0.75);

    Ok(all_paragraphs)
}

/// Format extracted paragraphs for inclusion in a prompt
pub fn format_paragraphs_for_prompt(paragraphs: &[ExtractedParagraph]) -> String {
    let mut output = String::new();
    
    output.push_str("## Relevant W3C Specification Paragraphs\n\n");
    
    let mut current_source = String::new();
    
    for para in paragraphs {
        if para.source_file != current_source {
            current_source = para.source_file.clone();
            output.push_str(&format!("\n### From: {}\n\n", current_source));
        }
        
        output.push_str(&format!(
            "**Section: {}**{}\n> {}\n\n",
            para.section,
            para.section_id.as_ref()
                .map(|id| format!(" (#{id})"))
                .unwrap_or_default(),
            para.text.chars().take(500).collect::<String>()
        ));
    }
    
    output
}

/// Deduplicate paragraphs whose text overlaps significantly.
///
/// Uses word-level Jaccard similarity: |A intersect B| / |A union B|.
/// Paragraphs are processed in input order (already sorted by relevance),
/// so the most relevant paragraph of any similar group is kept.
fn deduplicate_paragraphs(
    paragraphs: Vec<ExtractedParagraph>,
    threshold: f64,
) -> Vec<ExtractedParagraph> {
    use std::collections::HashSet;

    if paragraphs.len() <= 1 {
        return paragraphs;
    }

    // Pre-compute word sets (skip tiny words to reduce noise)
    let word_sets: Vec<HashSet<&str>> = paragraphs
        .iter()
        .map(|p| {
            p.text
                .split_whitespace()
                .filter(|w| w.len() >= 3)
                .collect()
        })
        .collect();

    let mut keep = vec![false; paragraphs.len()];

    for i in 0..paragraphs.len() {
        let dominated = keep.iter().enumerate().any(|(j, &kept)| {
            kept && jaccard_similarity(&word_sets[i], &word_sets[j]) > threshold
        });
        if !dominated {
            keep[i] = true;
        }
    }

    paragraphs
        .into_iter()
        .zip(keep)
        .filter_map(|(p, k)| if k { Some(p) } else { None })
        .collect()
}

fn jaccard_similarity(a: &std::collections::HashSet<&str>, b: &std::collections::HashSet<&str>) -> f64 {
    let intersection = a.iter().filter(|w| b.contains(*w)).count();
    let union = a.len() + b.len() - intersection;
    if union == 0 {
        return 0.0;
    }
    intersection as f64 / union as f64
}
