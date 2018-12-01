use azul::prelude::{HotReloadable, AppStyle};

#[cfg(debug_assertions)]
pub struct HotReloader {
    file_path: String,
}

impl HotReloader {
    pub fn new(file_path: String) -> Self {
        HotReloader { file_path }
    }
}

#[cfg(debug_assertions)]
impl HotReloadable for HotReloader {
    fn reload_style(&mut self) -> Option<AppStyle> {
        use std::fs;

        let file_path = &self.file_path.clone();

        let reloaded_css = match fs::read_to_string(&file_path) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    format!("Failed to hot-reload CSS file: Io error: {} when loading file: \"{}\"", e, file);
                }
                return None;
            },
        };

        match ::css::new_from_str(&reloaded_css) {
            Ok(style) => Some(style),
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to hot-reload CSS file: - parse error \"{}\":\r\n{}\n", file_path, e);
                }
                None
            },
        }
    }
}
