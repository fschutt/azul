mod apidocs;
mod guide;
pub mod donate;
use std::{
    collections::{BTreeMap, HashMap},
    path::Path,
};

use anyhow::Context;
use serde_derive::{Deserialize, Serialize};

use crate::api::{ApiData, LoadedExample, VersionData};

const HTML_ROOT: &str = "https://azul.rs";

/// Generate all documentation files
pub fn generate_docs(
    api_data: &ApiData,
    imageoutput_path: &Path,
    imageoutput_url: &str,
) -> anyhow::Result<HashMap<String, String>> {
    let mut docs = HashMap::new();

    // Generate main index.html
    docs.insert(
        "index.html".to_string(),
        generate_index_html(&api_data, imageoutput_path, imageoutput_url)?,
    );

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

    let latest_version = api_data.get_latest_version_str().unwrap();

    // Generate guide pages
    for guide in guide::get_guide_list() {
        let guide_html = guide::generate_guide_html(&guide, latest_version);
        docs.insert(
            format!("guide/{}/{}.html", latest_version, guide.file_name),
            guide_html,
        );
    }

    for version in api_data.get_sorted_versions() {
        docs.insert(
            format!("guide/{version}.html"),
            guide::generate_guide_mainpage(latest_version),
        );
    }

    // Generate combined guide page
    docs.insert(
        "guide.html".to_string(),
        guide::generate_guide_index(&api_data.get_sorted_versions()),
    );

    Ok(docs)
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct ExampleRendered {
    id: String,
    description: String,
    alt: String,
    #[serde(rename = "screenshot:windows")]
    screenshot_windows: String,
    #[serde(rename = "screenshot:linux")]
    screenshot_linux: String,
    #[serde(rename = "screenshot:mac")]
    screenshot_mac: String,
    #[serde(rename = "code:c")]
    code_c: String,
    #[serde(rename = "code:cpp")]
    code_cpp: String,
    #[serde(rename = "code:python")]
    code_python: String,
    #[serde(rename = "code:rust")]
    code_rust: String,
}

/// Generate the main index.html page - imageoutput_path is the folder where all the screenshots go
fn generate_index_html(
    api_data: &ApiData,
    imageoutput_path: &Path,
    imageoutput_url: &str,
) -> anyhow::Result<String> {
    let latest_version_str = api_data.get_latest_version_str().unwrap();
    let latest_version = api_data.get_version(latest_version_str).unwrap();
    let latest_version_date = &latest_version.date;

    let imagepath = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../examples/assets/screenshots"
    );
    let examples_path = concat!(env!("CARGO_MANIFEST_DIR"), "/../examples");
    let imageoutput_path = Path::new(imageoutput_path);

    assert!(Path::new(imagepath).exists());
    assert!(Path::new(examples_path).exists());
    assert!(imageoutput_path.exists());

    let examples = latest_version
        .examples
        .iter()
        .map(|s| s.load(examples_path, &imagepath))
        .collect::<anyhow::Result<Vec<LoadedExample>>>()?;

    let mut ex = Vec::new();
    for e in examples {
        let name = e.name;
        ex.push(ExampleRendered {
            id: name.clone(),
            description: comrak::markdown_to_html(
                &e.description.join("\r\n"),
                &comrak::Options::default(),
            ),
            alt: e.alt.clone(),
            screenshot_windows: {
                let _ = std::fs::write(
                    imageoutput_path.join(&format!("{name}.windows.png")),
                    &e.screenshot.windows,
                );
                format!("{imageoutput_url}/{name}.windows.png")
            },
            screenshot_linux: {
                let _ = std::fs::write(
                    imageoutput_path.join(&format!("{name}.linux.png")),
                    &e.screenshot.linux,
                );
                format!("{imageoutput_url}/{name}.linux.png")
            },
            screenshot_mac: {
                let _ = std::fs::write(
                    imageoutput_path.join(&format!("{name}.mac.png")),
                    &e.screenshot.mac,
                );
                format!("{imageoutput_url}/{name}.mac.png")
            },
            code_c: escape_code(&String::from_utf8_lossy(&e.code.c).to_string()),
            code_cpp: escape_code(&String::from_utf8_lossy(&e.code.cpp).to_string()),
            code_python: escape_code(&String::from_utf8_lossy(&e.code.python).to_string()),
            code_rust: escape_code(&String::from_utf8_lossy(&e.code.rust).to_string()),
        })
    }

    let index_html_template = include_str!("../../templates/index.template.html")
        .replace("$$ROOT_RELATIVE$$", "https://azul.rs")
        .replace("<!-- HEAD -->", &get_common_head_tags())
        .replace("<!-- SIDEBAR -->", &get_sidebar());

    let index_example_html_template = include_str!("../../templates/index.section.template.html")
        .replace("$$ROOT_RELATIVE$$", "https://azul.rs");

    let examples = ex
        .iter()
        .map(|ex| {
            index_example_html_template
                .replace("$$EXAMPLE_DESCRIPTION$$", &ex.description)
                .replace("$$EXAMPLE_ID$$", &ex.id)
                .replace("$$EXAMPLE_CODE$$", &ex.code_python)
                .replace("$$EXAMPLE_IMAGE_ALT$$", &ex.alt)
                .replace("$$EXAMPLE_IMAGE_SOURCE_LINUX$$", &ex.screenshot_linux)
                .replace("$$EXAMPLE_IMAGE_SOURCE_MAC$$", &ex.screenshot_mac)
                .replace("$$EXAMPLE_IMAGE_SOURCE_WINDOWS$$", &ex.screenshot_windows)
        })
        .collect::<Vec<_>>()
        .join("\r\n");

    let ex_json = serde_json::to_string(
        &ex.iter()
            .map(|s| (s.id.clone(), s))
            .collect::<BTreeMap<_, _>>(),
    )
    .unwrap_or_default();

    Ok(index_html_template
        .replace("$$INDEX_SECTION_EXAMPLES$$", &examples)
        .replace("$$JAVASCRIPT_EXAMPLES$$", &ex_json)
        .replace("$$LATEST_VERSION$$", latest_version_str)
        .replace("$$LATEST_DATE$$", &latest_version_date))
}

fn escape_code(s: &str) -> String {
    s.replace("<", "&lt;").replace(">", "&gt;")
}

pub fn get_common_head_tags() -> String {
    format!("
      <meta charset='utf-8'/>
      <meta name='viewport' content='width=device-width, initial-scale=1'>
      <meta http-equiv='Content-Type' content='text/html; charset=utf-8'/>
      <meta name='description' content='Cross-platform MIT-licensed desktop GUI framework for C and Rust using the Mozilla WebRender rendering engine'>
      <meta name='keywords' content='gui, rust, user interface'>
  
      <link rel='preload' as='font' href='https://azul.rs/fonts/SourceSerifPro-Regular.ttf' type='font/ttf'>
      <link rel='preload' as='font' href='../fonts/Morris Jenson Initialen.ttf' type='font/ttf'>
      <link rel='shortcut icon' type='image/x-icon' href='https://azul.rs/favicon.ico'>
      <link rel='stylesheet' type='text/css' href='https://azul.rs/main.css'>
    ")
}

pub fn get_sidebar() -> String {
    format!(
        "
        <nav>
        <ul>
          <li><a href='https://azul.rs'>overview</a></li>
          <li><a href='https://azul.rs/releases'>releases</a></li>
          <li><a href='https://github.com/fschutt/azul'>code</a></li>
          <li><a href='https://discord.gg/V96ZGKqQvn'>discord</a></li>
          <li><a href='https://azul.rs/guide'>guide</a></li>
          <li><a href='https://azul.rs/api'>api</a></li>
          <li><a href='https://azul.rs/reftest'>reftests</a></li>
          <li> <a href='https://azul.rs/donate'>donate</a></li>
        </ul>

      </nav>
    "
    )
}
