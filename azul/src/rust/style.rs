    #![allow(dead_code, unused_imports)]
    //! DOM to CSS cascading and styling module
    use crate::dll::*;
    use std::ffi::c_void;
    use crate::dom::Dom;
    use crate::css::Css;


    /// `Node` struct
    #[doc(inline)] pub use crate::dll::AzNode as Node;

    impl Clone for Node { fn clone(&self) -> Self { *self } }
    impl Copy for Node { }


    /// `CascadeInfo` struct
    #[doc(inline)] pub use crate::dll::AzCascadeInfo as CascadeInfo;

    impl Clone for CascadeInfo { fn clone(&self) -> Self { *self } }
    impl Copy for CascadeInfo { }


    /// `RectStyle` struct
    #[doc(inline)] pub use crate::dll::AzRectStyle as RectStyle;

    impl Clone for RectStyle { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_rect_style_deep_copy)(self) } }
    impl Drop for RectStyle { fn drop(&mut self) { (crate::dll::get_azul_dll().az_rect_style_delete)(self); } }


    /// `RectLayout` struct
    #[doc(inline)] pub use crate::dll::AzRectLayout as RectLayout;

    impl Clone for RectLayout { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_rect_layout_deep_copy)(self) } }
    impl Drop for RectLayout { fn drop(&mut self) { (crate::dll::get_azul_dll().az_rect_layout_delete)(self); } }


    /// `CascadedCssPropertyWithSource` struct
    #[doc(inline)] pub use crate::dll::AzCascadedCssPropertyWithSource as CascadedCssPropertyWithSource;

    impl Clone for CascadedCssPropertyWithSource { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_cascaded_css_property_with_source_deep_copy)(self) } }
    impl Drop for CascadedCssPropertyWithSource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_cascaded_css_property_with_source_delete)(self); } }


    /// `CssPropertySource` struct
    #[doc(inline)] pub use crate::dll::AzCssPropertySource as CssPropertySource;

    impl Clone for CssPropertySource { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_css_property_source_deep_copy)(self) } }
    impl Drop for CssPropertySource { fn drop(&mut self) { (crate::dll::get_azul_dll().az_css_property_source_delete)(self); } }


    /// `StyledNodeState` struct
    #[doc(inline)] pub use crate::dll::AzStyledNodeState as StyledNodeState;

    impl Clone for StyledNodeState { fn clone(&self) -> Self { *self } }
    impl Copy for StyledNodeState { }


    /// `StyledNode` struct
    #[doc(inline)] pub use crate::dll::AzStyledNode as StyledNode;

    impl Clone for StyledNode { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_styled_node_deep_copy)(self) } }
    impl Drop for StyledNode { fn drop(&mut self) { (crate::dll::get_azul_dll().az_styled_node_delete)(self); } }


    /// `TagId` struct
    #[doc(inline)] pub use crate::dll::AzTagId as TagId;

    impl Clone for TagId { fn clone(&self) -> Self { *self } }
    impl Copy for TagId { }


    /// `TagIdToNodeIdMapping` struct
    #[doc(inline)] pub use crate::dll::AzTagIdToNodeIdMapping as TagIdToNodeIdMapping;

    impl Clone for TagIdToNodeIdMapping { fn clone(&self) -> Self { *self } }
    impl Copy for TagIdToNodeIdMapping { }


    /// `ParentWithNodeDepth` struct
    #[doc(inline)] pub use crate::dll::AzParentWithNodeDepth as ParentWithNodeDepth;

    impl Clone for ParentWithNodeDepth { fn clone(&self) -> Self { *self } }
    impl Copy for ParentWithNodeDepth { }


    /// `ContentGroup` struct
    #[doc(inline)] pub use crate::dll::AzContentGroup as ContentGroup;

    impl Clone for ContentGroup { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_content_group_deep_copy)(self) } }
    impl Drop for ContentGroup { fn drop(&mut self) { (crate::dll::get_azul_dll().az_content_group_delete)(self); } }


    /// `StyledDom` struct
    #[doc(inline)] pub use crate::dll::AzStyledDom as StyledDom;

    impl StyledDom {
        /// Styles a `Dom` with the given `Css`, returning the `StyledDom` - complexity `O(count(dom_nodes) * count(css_blocks))`: make sure that the `Dom` and the `Css` are as small as possible, use inline CSS if the performance isn't good enough
        pub fn new(dom: Dom, css: Css) -> Self { (crate::dll::get_azul_dll().az_styled_dom_new)(dom, css) }
        /// Appends an already styled list of DOM nodes to the current `dom.root` - complexity `O(count(dom.dom_nodes))`
        pub fn append(&mut self, dom: StyledDom)  { (crate::dll::get_azul_dll().az_styled_dom_append)(self, dom) }
    }

    impl Clone for StyledDom { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_styled_dom_deep_copy)(self) } }
    impl Drop for StyledDom { fn drop(&mut self) { (crate::dll::get_azul_dll().az_styled_dom_delete)(self); } }
