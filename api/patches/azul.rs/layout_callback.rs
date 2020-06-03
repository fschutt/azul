    use crate::dom::Dom;

    /// Callback fn that returns the layout
    pub type LayoutCallback = fn(RefAny, LayoutInfo) -> Dom;

    fn default_callback(_: RefAny, _: LayoutInfo) -> Dom {
        Dom::div()
    }

    pub(crate) static mut CALLBACK: LayoutCallback = default_callback;

    pub(crate) fn translate_callback(data: crate::dll::AzRefAny, layout: crate::dll::AzLayoutInfoPtr) -> crate::dll::AzDomPtr {
        unsafe { CALLBACK(RefAny(data), LayoutInfo { ptr: layout }) }.leak()
    }
