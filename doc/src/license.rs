use std::collections::HashMap;

pub struct License {
    pub name: String,
    pub version: String,
    pub license_type: String,
    pub authors: Vec<String>,
}

/// Generate license information for Windows, Linux, Mac
pub fn generate_license() -> HashMap<String, String> {
    let mut licenses = HashMap::new();
    
    // In a real implementation, this would run cargo-license to get dependency information
    // and generate license files for each platform
    

    let default_license_text = vec![
        "[program] is based in part on the AZUL GUI toolkit (https://azul.rs),",
        "licensed under the MIT License (C) 2018 Felix Schütt.",
        "",
        "The AZUL GUI toolkit itself uses the following libraries:",
        "",
        ""
        ].join("\r\n");
    
    let license_posttext = vec![
        "",
        "To generate the full text of the license for the license, please visit",
        "https://spdx.org/licenses/ and replace the license author in the source",
        "text in any given license with the name of the author listed above."
    ].join("\r\n");
    
    // Generate Windows license
    let windows_license = format!(
        "{}{}{}",
        default_license_text,
        format_license_authors(&get_windows_dependencies()),
        license_posttext
    );
    licenses.insert("LICENSE-WINDOWS.txt".to_string(), windows_license);
    
    // Generate Linux license
    let linux_license = format!(
        "{}{}{}",
        default_license_text,
        format_license_authors(&get_linux_dependencies()),
        license_posttext
    );
    licenses.insert("LICENSE-LINUX.txt".to_string(), linux_license);
    
    // Generate Mac license
    let mac_license = format!(
        "{}{}{}",
        default_license_text,
        format_license_authors(&get_mac_dependencies()),
        license_posttext
    );
    licenses.insert("LICENSE-MAC.txt".to_string(), mac_license);
    
    licenses
}

/// Format license authors into text
fn format_license_authors(dependencies: &[License]) -> String {
    let mut license_txt = String::new();
    
    for dep in dependencies {
        let authors_str = dep.authors.join(", ");
        license_txt.push_str(&format!(
            "{} v{} licensed {} \r\n    by {}\r\n",
            dep.name, dep.version, dep.license_type, authors_str
        ));
    }
    
    license_txt
}

/// Get Windows dependencies
fn get_windows_dependencies() -> Vec<License> {
    // In a real implementation, this would run cargo-license to get dependency information
    vec![
        License {
            name: "azul-core".to_string(),
            version: "1.0.0".to_string(),
            license_type: "MPL-2.0".to_string(),
            authors: vec!["Felix Schütt <felix.schuett@maps4print.com>".to_string()],
        },
        License {
            name: "webrender".to_string(),
            version: "0.61.0".to_string(),
            license_type: "MPL-2.0".to_string(),
            authors: vec!["Mozilla Foundation".to_string()],
        },
        // Add more dependencies as needed
    ]
}

/// Get Linux dependencies
fn get_linux_dependencies() -> Vec<License> {
    // In a real implementation, this would run cargo-license to get dependency information
    vec![
        License {
            name: "azul-core".to_string(),
            version: "1.0.0".to_string(),
            license_type: "MPL-2.0".to_string(),
            authors: vec!["Felix Schütt <felix.schuett@maps4print.com>".to_string()],
        },
        License {
            name: "webrender".to_string(),
            version: "0.61.0".to_string(),
            license_type: "MPL-2.0".to_string(),
            authors: vec!["Mozilla Foundation".to_string()],
        },
        // Add more dependencies as needed
    ]
}

/// Get Mac dependencies
fn get_mac_dependencies() -> Vec<License> {
    // In a real implementation, this would run cargo-license to get dependency information
    vec![
        License {
            name: "azul-core".to_string(),
            version: "1.0.0".to_string(),
            license_type: "MPL-2.0".to_string(),
            authors: vec!["Felix Schütt <felix.schuett@maps4print.com>".to_string()],
        },
        License {
            name: "webrender".to_string(),
            version: "0.61.0".to_string(),
            license_type: "MPL-2.0".to_string(),
            authors: vec!["Mozilla Foundation".to_string()],
        },
        // Add more dependencies as needed
    ]
}