use azul::prelude::{HotReloadHandler, AppStyle};

#[cfg(debug_assertions)]
pub struct HotReloader {
    file_path: String,
}

#[cfg(debug_assertions)]
impl HotReloader {
    pub fn new(file_path: String) -> Self {
        HotReloader { file_path }
    }
}

#[cfg(debug_assertions)]
impl HotReloadHandler for HotReloader {
    fn reload_style(&mut self) -> Option<Result<AppStyle, String>> {
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
