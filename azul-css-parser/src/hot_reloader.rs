//! Provides an implementation of the HotReloadHandler from the `azul_css` crate, allowing CSS
//! files to be dynamically reloaded at runtime.

use azul_css::{Css, HotReloadHandler};
use std::path::PathBuf;
use std::time::Duration;

pub const DEFAULT_RELOAD_INTERVAL: Duration = Duration::from_millis(500);

/// Allows dynamic reloading of a CSS file at application runtime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HotReloader {
    file_path: PathBuf,
    reload_interval: Duration,
}

impl HotReloader {
    /// Creates a HotReloader that will load a style directly from the CSS file
    /// at the given path.
    pub fn new<P: Into<PathBuf>>(file_path: P) -> Self {
        Self {
            file_path: file_path.into(),
            reload_interval: DEFAULT_RELOAD_INTERVAL,
        }
    }

    pub fn with_reload_interval(self, reload_interval: Duration) -> Self {
        Self {
            reload_interval,
            ..self
        }
    }
}

impl HotReloadHandler for HotReloader {
    fn reload_style(&mut self) -> Result<Css, String> {
        use std::fs;

        let reloaded_css = fs::read_to_string(&self.file_path).map_err(|e| {
            format!(
                "Io error: \"{}\" when loading file \"{}\"",
                e,
                self.file_path.to_str().unwrap_or("")
            )
        })?;

        ::css::new_from_str(&reloaded_css).map_err(|e| {
            let file_name = self
                .file_path
                .file_name()
                .and_then(|os_str| Some(os_str.to_string_lossy()))
                .unwrap_or_default();
            format!("{}: {}", file_name, e)
        })
    }

    fn get_reload_interval(&self) -> Duration {
        self.reload_interval
    }
}
