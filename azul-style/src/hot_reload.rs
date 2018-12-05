//! Traits and datatypes associated with reloading styles at runtime.

use style::AppStyle;

/// Public interface that can be used to reload an AppStyle while an application is running. This
/// is useful for quickly iterating over different styles during development -- you can load from
/// a file, from an online source, or perhaps even from an AI style generator!
pub trait HotReloadHandler {
    fn reload_style(&mut self) -> Option<Result<AppStyle, String>>;
}

/// Custom hot-reloader combinator that can be used to merge hot-reloaded styles onto a base style.
/// Can be useful when working from a base configuration, such as the OS-native styles.
pub struct HotReloadOverride {
    base_style: AppStyle,
    hot_reloader: Box<dyn HotReloadHandler>,
}

impl HotReloadOverride {
    /// Creates a new HotReloadHandler type that merges styles from the given `hot_reloader` onto
    /// the given `base_style`.
    pub fn new(base_style: AppStyle, hot_reloader: Box<dyn HotReloadHandler>) -> Box<dyn HotReloadHandler> {
        Box::new(Self {
            base_style,
            hot_reloader,
        })
    }
}

impl HotReloadHandler for HotReloadOverride {
    fn reload_style(&mut self) -> Option<Result<AppStyle, String>> {
        match self.hot_reloader.reload_style() {
            Some(Ok(style)) => {
                let mut base = self.base_style.clone();
                base.merge(style);
                Some(Ok(base))
            },
            other => other,
        }
    }
}
