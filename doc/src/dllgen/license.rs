pub struct License {
    pub name: String,
    pub version: String,
    pub license_type: String,
    pub authors: Vec<String>,
}

/// Format license authors into text
pub fn format_license_authors(dependencies: &[License]) -> String {
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
