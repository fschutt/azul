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

        let target_css = if true {
            format!("{}\r\n{}\n", ::css::NATIVE_CSS, reloaded_css)
        } else {
            reloaded_css
        };

        let css = match ::css::new_from_str(&target_css) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to reload - parse error \"{}\":\r\n{}\n", file_path, e);
                }
                return None;
            },
        };

        Some(css)
    }
}
