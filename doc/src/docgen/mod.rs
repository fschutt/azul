mod apidocs;
mod examples;
mod guide;
mod release;

use std::collections::HashMap;

use crate::{
    api::ApiData,
    utils::string::{format_doc, render_example_code, render_example_description},
};

const HTML_ROOT: &str = "https://azul.rs";

/// Generate all documentation files
pub fn generate_docs(api_data: &ApiData) -> HashMap<String, String> {
    let mut docs = HashMap::new();

    // Get the latest version
    let latest_version = api_data.get_latest_version_str().unwrap();

    // Generate main index.html
    docs.insert("index.html".to_string(), generate_index_html());

    // Generate API documentation for each version
    for version in api_data.get_sorted_versions() {
        let api_html = apidocs::generate_api_html(api_data, &version);
        docs.insert(format!("api/{}.html", version), api_html);
    }

    // Generate combined API page
    docs.insert(
        "api.html".to_string(),
        apidocs::generate_api_index(api_data),
    );

    // Generate guide pages
    for guide in guide::get_guide_list() {
        let guide_html = guide::generate_guide_html(&guide, latest_version);
        docs.insert(
            format!("guide/{}/{}.html", latest_version, guide.file_name),
            guide_html,
        );
    }

    // Generate combined guide page
    docs.insert(
        "guide.html".to_string(),
        guide::generate_guide_index(latest_version),
    );

    // Generate release notes
    for version in api_data.get_sorted_versions() {
        let release_html = release::generate_release_html(&version);
        docs.insert(format!("release/{}.html", version), release_html);
    }

    // Generate combined releases page
    docs.insert(
        "releases.html".to_string(),
        release::generate_releases_index(api_data),
    );

    docs
}

/// Generate the main index.html page
fn generate_index_html() -> String {
    // In a real implementation, this would load the template, fill in the examples, etc.
    "Main index.html template would be filled in here".to_string()
}
