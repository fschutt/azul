use crate::api::ApiData;
use super::HTML_ROOT;

/// Generate HTML for a specific release version
pub fn generate_release_html(version: &str) -> String {
    let mut html = String::new();
    
    // Load the HTML template - in a real implementation, this would be read from a file
    html.push_str("<!DOCTYPE html>\n<html><head><title>Release Notes - ");
    html.push_str(version);
    html.push_str("</title></head><body>\n");
    
    html.push_str(&format!("<h1>Release Notes - Version {}</h1>\n", version));
    
    // Load release notes - in a real implementation, this would be read from a file
    let release_content = match version {
        "1.0.0-alpha1" => {
            r#"
            <p>First stable ABI release - current Windows only!</p>

            <br/>
            <strong>Supported operating systems for this release:</strong>
            <ul style="margin-left:20px;">
              <li>Windows: 7, 8, 8.1, 10</li>
            </ul>

            <br/>

            <strong>Links:</strong>
            <ul style="margin-left:20px;">
              <li><a href="https://azul.rs/api/1.0.0-alpha.html">Documentation for this release</a></li>
              <li><a href="https://azul.rs/guide/1.0.0-alpha.html">Guide for this release</a></li>
              <br/>
              <li><a href="https://github.com/fschutt/azul/releases">GitHub release</a></li>
              <li><a href="https://crates.io/crates/azul">Crates.io</a></li>
              <li><a href="https://docs.rs/azul">Docs.rs</a></li>
            </ul>

            <br/>

            <strong>Files:</strong>
            <br/>
            <ul style="margin-left:20px;">
              <li><a href="./1.0.0-alpha/files/azul.dll">Windows 64-bit DLL (azul.dll - 2.6Mb)</a></li>
              <li><a href="./1.0.0-alpha/files/azul.dll">Windows 64-bit Python extension (azul.pyd  - 3.1Mb)</a></li>
              <li><a href="./1.0.0-alpha/files/license.txt">LICENSE-WINDOWS.txt (19KB)</a></li>
              <br/>
              <li><a href="./1.0.0-alpha/files/sourcecode.zip">Source code .zip (25Mb)</a></li>
              <li><a href="./1.0.0-alpha/files/dependencies.zip">Source code for all dependencies .zip (350Mb)</a></li>
              <li><a href="./1.0.0-alpha/files/api.json">API Description - api.json (714KB)</a></li>
              <li><a href="./1.0.0-alpha/files/azul.h">C Header (azul.h - 978KB)</a></li>
            </ul>

            <br/>
            <strong>Examples:</strong>
            <br/>
            <ul style="margin-left:20px;">
              <li><a href="./1.0.0-alpha/files/examples-windows.zip">Windows examples with source code (.zip - 154KB)</a></li>
            </ul>
            "#
        },
        // Add more versions as needed
        _ => "<p>Release notes not available for this version.</p>",
    };
    
    html.push_str(release_content);
    html.push_str("</body></html>");
    
    html
}

/// Generate a combined releases index page
pub fn generate_releases_index(api_data: &ApiData) -> String {
    let mut html = String::new();
    
    // Load the HTML template - in a real implementation, this would be read from a file
    html.push_str("<!DOCTYPE html>\n<html><head><title>Releases</title></head><body>\n");
    
    html.push_str("<h1>Choose release version</h1>\n");
    
    // Create a list of releases
    html.push_str("<ul>\n");
    
    for version in api_data.get_sorted_versions() {
        html.push_str(&format!("<li><a href=\"{}/release/{}\">{}</a></li>\n", 
                             HTML_ROOT, version, version));
    }
    
    html.push_str("</ul>\n");
    html.push_str("</body></html>");
    
    html
}