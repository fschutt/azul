use style::AppStyle;

/// Public interface that can be used to reload an AppStyle while an application is running. This
/// is useful for quickly iterating over different styles during development -- you can load from
/// a file, from an online source, or perhaps even from an AI style generator!
///
/// This trait is only available when debug_assertions are enabled.
pub trait HotReloadHandler {
    fn reload_style(&mut self) -> Option<Result<AppStyle, String>>;
}

pub struct HotReloadOverride {
    base_style: AppStyle,
    hot_reloader: Box<dyn HotReloadHandler>,
}

impl HotReloadOverride {
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
