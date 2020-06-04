    use crate::dom::Dom;

    /// Callback fn that returns the DOM of the app
    pub type LayoutCallback = fn(RefAny, LayoutInfo) -> Dom;