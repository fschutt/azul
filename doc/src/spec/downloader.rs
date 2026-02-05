//! W3C Specification Downloader
//!
//! Downloads W3C CSS specifications and stores them locally for offline analysis.

use std::path::{Path, PathBuf};
use std::collections::HashMap;

/// Known W3C specifications with their download URLs
pub struct SpecRegistry {
    specs: HashMap<String, SpecInfo>,
}

#[derive(Debug, Clone)]
pub struct SpecInfo {
    pub id: String,
    pub name: String,
    pub urls: Vec<SpecUrl>,
}

#[derive(Debug, Clone)]
pub struct SpecUrl {
    pub section: String,
    pub url: String,
    pub local_filename: String,
}

impl Default for SpecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SpecRegistry {
    pub fn new() -> Self {
        let mut specs = HashMap::new();
        
        // CSS 2.2 (the normative CSS 2 spec)
        specs.insert("css22".to_string(), SpecInfo {
            id: "css22".to_string(),
            name: "CSS 2.2 Specification".to_string(),
            urls: vec![
                SpecUrl {
                    section: "Box Model".to_string(),
                    url: "https://www.w3.org/TR/CSS22/box.html".to_string(),
                    local_filename: "css22-box.html".to_string(),
                },
                SpecUrl {
                    section: "Visual Formatting Model".to_string(),
                    url: "https://www.w3.org/TR/CSS22/visuren.html".to_string(),
                    local_filename: "css22-visuren.html".to_string(),
                },
                SpecUrl {
                    section: "Visual Formatting Model Details".to_string(),
                    url: "https://www.w3.org/TR/CSS22/visudet.html".to_string(),
                    local_filename: "css22-visudet.html".to_string(),
                },
                SpecUrl {
                    section: "Tables".to_string(),
                    url: "https://www.w3.org/TR/CSS22/tables.html".to_string(),
                    local_filename: "css22-tables.html".to_string(),
                },
            ],
        });
        
        // CSS Text Level 3
        specs.insert("css-text-3".to_string(), SpecInfo {
            id: "css-text-3".to_string(),
            name: "CSS Text Module Level 3".to_string(),
            urls: vec![
                SpecUrl {
                    section: "Full Spec".to_string(),
                    url: "https://www.w3.org/TR/css-text-3/".to_string(),
                    local_filename: "css-text-3.html".to_string(),
                },
            ],
        });
        
        // CSS Flexbox
        specs.insert("css-flexbox-1".to_string(), SpecInfo {
            id: "css-flexbox-1".to_string(),
            name: "CSS Flexible Box Layout Module Level 1".to_string(),
            urls: vec![
                SpecUrl {
                    section: "Full Spec".to_string(),
                    url: "https://www.w3.org/TR/css-flexbox-1/".to_string(),
                    local_filename: "css-flexbox-1.html".to_string(),
                },
            ],
        });
        
        // CSS Grid
        specs.insert("css-grid-1".to_string(), SpecInfo {
            id: "css-grid-1".to_string(),
            name: "CSS Grid Layout Module Level 1".to_string(),
            urls: vec![
                SpecUrl {
                    section: "Full Spec".to_string(),
                    url: "https://www.w3.org/TR/css-grid-1/".to_string(),
                    local_filename: "css-grid-1.html".to_string(),
                },
            ],
        });
        
        // CSS Sizing
        specs.insert("css-sizing-3".to_string(), SpecInfo {
            id: "css-sizing-3".to_string(),
            name: "CSS Box Sizing Module Level 3".to_string(),
            urls: vec![
                SpecUrl {
                    section: "Full Spec".to_string(),
                    url: "https://www.w3.org/TR/css-sizing-3/".to_string(),
                    local_filename: "css-sizing-3.html".to_string(),
                },
            ],
        });
        
        // CSS Display
        specs.insert("css-display-3".to_string(), SpecInfo {
            id: "css-display-3".to_string(),
            name: "CSS Display Module Level 3".to_string(),
            urls: vec![
                SpecUrl {
                    section: "Full Spec".to_string(),
                    url: "https://www.w3.org/TR/css-display-3/".to_string(),
                    local_filename: "css-display-3.html".to_string(),
                },
            ],
        });
        
        Self { specs }
    }
    
    pub fn get_spec(&self, id: &str) -> Option<&SpecInfo> {
        self.specs.get(id)
    }
    
    pub fn list_specs(&self) -> Vec<&SpecInfo> {
        self.specs.values().collect()
    }
    
    pub fn get_all_urls(&self) -> Vec<&SpecUrl> {
        self.specs.values()
            .flat_map(|s| s.urls.iter())
            .collect()
    }
}

/// Download a single spec URL
pub fn download_spec(url: &str, output_dir: &Path, filename: &str) -> Result<PathBuf, String> {
    let output_path = output_dir.join(filename);
    
    // Check if already downloaded
    if output_path.exists() {
        println!("  [skip] {} already exists", filename);
        return Ok(output_path);
    }
    
    println!("  [download] {} -> {}", url, filename);
    
    // Use curl command (available on macOS/Linux)
    let output = std::process::Command::new("curl")
        .args(["-L", "-s", "-o"])
        .arg(&output_path)
        .arg(url)
        .output()
        .map_err(|e| format!("Failed to run curl: {}", e))?;
    
    if !output.status.success() {
        return Err(format!(
            "curl failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    
    // Verify file was created and has content
    let metadata = std::fs::metadata(&output_path)
        .map_err(|e| format!("Failed to read downloaded file: {}", e))?;
    
    if metadata.len() == 0 {
        std::fs::remove_file(&output_path).ok();
        return Err("Downloaded file is empty".to_string());
    }
    
    println!("  [ok] Downloaded {} bytes", metadata.len());
    
    Ok(output_path)
}

/// Download all registered specs
pub fn download_all_specs(output_dir: &Path) -> Result<Vec<PathBuf>, String> {
    // Create output directory
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create spec directory: {}", e))?;
    
    let registry = SpecRegistry::new();
    let mut downloaded = Vec::new();
    let mut errors = Vec::new();
    
    for spec in registry.list_specs() {
        println!("\nDownloading: {}", spec.name);
        for url_info in &spec.urls {
            match download_spec(&url_info.url, output_dir, &url_info.local_filename) {
                Ok(path) => downloaded.push(path),
                Err(e) => errors.push(format!("{}: {}", url_info.url, e)),
            }
        }
    }
    
    if !errors.is_empty() {
        eprintln!("\nErrors occurred:");
        for e in &errors {
            eprintln!("  - {}", e);
        }
    }
    
    println!("\nDownloaded {} files", downloaded.len());
    
    Ok(downloaded)
}

/// Download specs needed for a specific skill tree node
pub fn download_specs_for_node(
    node: &super::skill_tree::SkillNode,
    output_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    std::fs::create_dir_all(output_dir)
        .map_err(|e| format!("Failed to create spec directory: {}", e))?;
    
    let registry = SpecRegistry::new();
    let mut downloaded = Vec::new();
    
    for spec_url in &node.spec_urls {
        // Find matching URL in registry
        let filename = spec_url
            .trim_start_matches("https://www.w3.org/TR/")
            .replace('/', "-")
            .trim_end_matches('-')
            .to_string() + ".html";
        
        // Check registry for known filename
        let known_filename = registry.get_all_urls()
            .iter()
            .find(|u| u.url == *spec_url)
            .map(|u| u.local_filename.clone())
            .unwrap_or(filename);
        
        match download_spec(spec_url, output_dir, &known_filename) {
            Ok(path) => downloaded.push(path),
            Err(e) => eprintln!("Warning: Failed to download {}: {}", spec_url, e),
        }
    }
    
    Ok(downloaded)
}
