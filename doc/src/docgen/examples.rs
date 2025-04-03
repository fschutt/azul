use std::collections::HashMap;

use super::HTML_ROOT;
use crate::utils::string::{render_example_code, render_example_description};

/// Example information structure
pub struct Example {
    pub id: String,
    pub title: String,
    pub description: String,
    pub screenshot_path: String,
    pub screenshot_url: String,
    pub cpu_usage: String,
    pub memory_usage: String,
    pub image_alt: String,
    pub code: HashMap<String, String>,
}

/// Get a list of all examples
pub fn get_example_list() -> Vec<Example> {
    // In a real implementation, this would scan the examples directory
    // and read the code files

    let mut examples = Vec::new();

    // Example 1: Widgets
    let mut widgets_code = HashMap::new();
    widgets_code.insert(
        "c".to_string(),
        "// C code for widgets example would be here".to_string(),
    );
    widgets_code.insert(
        "cpp".to_string(),
        "// C++ code for widgets example would be here".to_string(),
    );
    widgets_code.insert(
        "rust".to_string(),
        "// Rust code for widgets example would be here".to_string(),
    );
    widgets_code.insert(
        "python".to_string(),
        "# Python code for widgets example would be here".to_string(),
    );

    examples.push(Example {
        id: "widgets".to_string(),
        title: "Widgets".to_string(),
        description: render_example_description(
            "Objects are composed into a DOM hierarchy which
            only gets re-rendered when a callback returns
            <code>RefreshDom</code>. The resulting DOM tree
            can be styled with CSS.",
            true,
        ),
        screenshot_path: "examples/assets/screenshots/helloworld.png".to_string(),
        screenshot_url: format!("{}/images/helloworld.png", HTML_ROOT),
        cpu_usage: "CPU: 0%".to_string(),
        memory_usage: "Memory: 23MB".to_string(),
        image_alt: "Rendering a simple UI using the Azul GUI toolkit".to_string(),
        code: widgets_code,
    });

    // Example 2: Hello World
    let mut hello_world_code = HashMap::new();
    hello_world_code.insert(
        "c".to_string(),
        "// C code for hello world example would be here".to_string(),
    );
    hello_world_code.insert(
        "cpp".to_string(),
        "// C++ code for hello world example would be here".to_string(),
    );
    hello_world_code.insert(
        "rust".to_string(),
        "// Rust code for hello world example would be here".to_string(),
    );
    hello_world_code.insert(
        "python".to_string(),
        "# Python code for hello world example would be here".to_string(),
    );

    examples.push(Example {
        id: "helloworld".to_string(),
        title: "Hello World".to_string(),
        description: render_example_description(
            "The UI structure is created via composition instead of inheritance.
            Callbacks can modify the application data and then tell the framework to
            reconstruct the entire UI again - but only if it's necessary, not on every frame.",
            true,
        ),
        screenshot_path: "examples/assets/screenshots/helloworld.png".to_string(),
        screenshot_url: format!("{}/images/helloworld.png", HTML_ROOT),
        cpu_usage: "CPU: 0%".to_string(),
        memory_usage: "Memory: 23MB".to_string(),
        image_alt: "Rendering a simple UI using the Azul GUI toolkit".to_string(),
        code: hello_world_code,
    });

    // Add more examples as needed

    examples
}

/// Generate HTML for the examples section of the home page
pub fn generate_examples_html() -> String {
    let mut html = String::new();

    for (index, example) in get_example_list().iter().enumerate() {
        // Load the HTML template for section - in a real implementation, this would be read from a
        // file
        let section_template = "\
<section class=\"feature\">
  <div class=\"col-1\">
    <img class=\"showcase\" id=\"showcase-image\" src=\"$$EXAMPLE_IMAGE_SOURCE$$\" \
                                alt=\"$$EXAMPLE_IMAGE_ALT$$\">
    <div>
      <div class=\"stats\">
        <ul>
          <li id=\"showcase-stats-memory\">$$EXAMPLE_STATS_MEMORY$$</li>
          <li id=\"showcase-stats-cpu\">$$EXAMPLE_STATS_CPU$$</li>
        </ul>
      </div>
      <div class=\"description\">
        <p id=\"showcase-description\">$$EXAMPLE_DESCRIPTION$$</p>
      </div>
    </div>
  </div>
  <div class=\"col-2\">
    <div class=\"code-container\">
      <div class=\"select-language\">
        <button id=\"select-language-python-$$EXAMPLE_ID$$\" \
                                class=\"select-language-btn-$$EXAMPLE_ID$$ active\" \
                                onclick=\"select_python_code($$EXAMPLE_ID$$)\">Python</button>
        <button id=\"select-language-cpp-$$EXAMPLE_ID$$\" \
                                class=\"select-language-btn-$$EXAMPLE_ID$$\" \
                                onclick=\"select_cpp_code($$EXAMPLE_ID$$)\">C++</button>
        <button id=\"select-language-rust-$$EXAMPLE_ID$$\" \
                                class=\"select-language-btn-$$EXAMPLE_ID$$\" \
                                onclick=\"select_rust_code($$EXAMPLE_ID$$)\">Rust</button>
        <button id=\"select-language-c-$$EXAMPLE_ID$$\" \
                                class=\"select-language-btn-$$EXAMPLE_ID$$\" \
                                onclick=\"select_c_code($$EXAMPLE_ID$$)\">C</button>
      </div>
      <code class=\"language-c\" id=\"code-$$EXAMPLE_ID$$\">$$EXAMPLE_CODE$$</code>
    </div>
  </div>
</section>";

        let section_html = section_template
            .replace("$$EXAMPLE_IMAGE_SOURCE$$", &example.screenshot_url)
            .replace("$$EXAMPLE_IMAGE_ALT$$", &example.image_alt)
            .replace("$$EXAMPLE_STATS_MEMORY$$", &example.memory_usage)
            .replace("$$EXAMPLE_STATS_CPU$$", &example.cpu_usage)
            .replace("$$EXAMPLE_DESCRIPTION$$", &example.description)
            .replace("$$EXAMPLE_ID$$", &index.to_string())
            .replace(
                "$$EXAMPLE_CODE$$",
                &render_example_code(&example.code["python"], true),
            );

        html.push_str(&section_html);
    }

    html
}

/// Generate JavaScript for the examples section
pub fn generate_examples_javascript() -> String {
    let examples = get_example_list();

    // Create JSON representation of examples for JavaScript
    let mut json = String::from("[\n");

    for example in &examples {
        json.push_str("  {\n");
        json.push_str(&format!("    \"id\": \"{}\",\n", example.id));
        json.push_str(&format!(
            "    \"description\": \"{}\",\n",
            example.description
        ));
        json.push_str(&format!(
            "    \"screenshot_url\": \"{}\",\n",
            example.screenshot_url
        ));
        json.push_str(&format!("    \"image_alt\": \"{}\",\n", example.image_alt));
        json.push_str(&format!("    \"memory\": \"{}\",\n", example.memory_usage));
        json.push_str(&format!("    \"cpu\": \"{}\",\n", example.cpu_usage));

        // Add code for each language
        for (lang, code) in &example.code {
            json.push_str(&format!(
                "    \"code:{}\": \"{}\",\n",
                lang,
                render_example_code(code, true)
            ));
        }

        // Remove trailing comma
        if json.ends_with(",\n") {
            json.truncate(json.len() - 2);
            json.push_str("\n");
        }

        json.push_str("  },\n");
    }

    // Remove trailing comma
    if json.ends_with(",\n") {
        json.truncate(json.len() - 2);
        json.push_str("\n");
    }

    json.push_str("]");

    json
}
