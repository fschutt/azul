//! Provides an implementation of the HotReloadHandler from the `azul_css` crate, allowing CSS
//! files to be dynamically reloaded at runtime.

use azul_css::{HotReloadHandler, Css};

/// Allows dynamic reloading of a CSS file at application runtime.
pub struct HotReloader {
    file_path: String,
}

impl HotReloader {
    /// Creates a HotReloader that will load a style directly from the CSS file
    /// at the given path.
    pub fn new(file_path: String) -> Box<dyn HotReloadHandler> {
        Box::new(HotReloader { file_path })
    }
}

impl HotReloadHandler for HotReloader {
    fn reload_style(&mut self) -> Option<Result<Css, String>> {
        use std::fs;

        let file_path = &self.file_path.clone();

        let reloaded_css = match fs::read_to_string(&file_path) {
            Ok(o) => o,
            Err(e) => {
                return Some(Err(format!("Io error: \"{}\" when loading file \"{}\"", e, file_path).to_string()));
            },
        };

        Some(match ::css::new_from_str(&reloaded_css) {
            Ok(style) => Ok(style),
            Err(e) => {
                Err(format!("Parse error \"{}\":\r\n{}\n", file_path, e).to_string())
            },
        })
    }
}
