use std::collections::HashMap;

use super::HTML_ROOT;

/// Guide information structure
pub struct Guide {
    pub title: String,
    pub file_name: String,
    pub content: String,
}

/// Get a list of all guides
pub fn get_guide_list() -> Vec<Guide> {
    // In a real implementation, this would scan the guide directory
    // and read the markdown files, converting them to HTML

    vec![
        Guide {
            title: "Installation".to_string(),
            file_name: "Installation".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Getting Started".to_string(),
            file_name: "GettingStarted".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Application Architecture".to_string(),
            file_name: "ApplicationArchitecture".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "CSS Styling".to_string(),
            file_name: "CssStyling".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Images, SVG and Charts".to_string(),
            file_name: "ImagesSvgAndCharts".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Timers, Threads and Animations".to_string(),
            file_name: "TimersThreadsAndAnimations".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "OpenGL".to_string(),
            file_name: "OpenGL".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Unit Testing".to_string(),
            file_name: "UnitTesting".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "XML and azulc".to_string(),
            file_name: "XmlAndAzulc".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Notes for C".to_string(),
            file_name: "NotesForC".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Notes for C++".to_string(),
            file_name: "NotesForCpp".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
        Guide {
            title: "Notes for Python".to_string(),
            file_name: "NotesForPython".to_string(),
            content: "Guide content would be read from markdown files".to_string(),
        },
    ]
}

/// Generate HTML for a specific guide
pub fn generate_guide_html(guide: &Guide, version: &str) -> String {
    let mut html = String::new();

    // Load the HTML template - in a real implementation, this would be read from a file
    html.push_str("<!DOCTYPE html>\n<html><head><title>Guide - ");
    html.push_str(&guide.title);
    html.push_str("</title></head><body>\n");

    html.push_str(&format!("<h1>{}</h1>\n", guide.title));

    // Include guide content
    html.push_str(&guide.content);

    // Add navigation links at the bottom
    html.push_str("<p><a href=\"");
    html.push_str(HTML_ROOT);
    html.push_str("/guide\">Back to guide index</a></p>");

    html.push_str("</body></html>");

    html
}

/// Generate a combined guide index page
pub fn generate_guide_index(version: &str) -> String {
    let mut html = String::new();

    // Load the HTML template - in a real implementation, this would be read from a file
    html.push_str("<!DOCTYPE html>\n<html><head><title>User Guide</title></head><body>\n");

    html.push_str("<h1>User Guide</h1>\n");

    // Create a list of guides
    html.push_str("<ul>\n");

    for guide in get_guide_list() {
        html.push_str(&format!(
            "<li><a href=\"{}/guide/{}/{}\">{}</a></li>\n",
            HTML_ROOT, version, guide.file_name, guide.title
        ));
    }

    html.push_str("</ul>\n");
    html.push_str("</body></html>");

    html
}
