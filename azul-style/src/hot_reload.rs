use style::AppStyle;

/// Public interface that can be used to reload an AppStyle while an application is running. This
/// is useful for quickly iterating over different styles during development -- you can load from
/// a file, from an online source, or perhaps even from an AI style generator!
///
/// This trait is only available when debug_assertions are enabled.
pub trait HotReloadHandler {
    fn reload_style(&mut self) -> Option<Result<AppStyle, String>>;
}
