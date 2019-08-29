use crate::{
    dom::Dom,
    callbacks::LayoutInfo,
    ui_solver::{ResolvedTextLayoutOptions, InlineTextLayout},
};

/// The core trait that has to be implemented for the app model to provide a mapping from an
/// application state to a user interface state (Model -> View).
pub trait Layout {
    /// This function is called on each frame - if Azul determines that the screen
    /// needs to be redrawn, it calls this function on the application data, in order
    /// to get the new UI. This prevents the UI state from getting out-of-sync
    /// with the application state.
    ///
    /// The `layout_info` can give you important information about the window states current
    /// size (for example, to return a different DOM depending on the viewport size) as well as
    /// giving access to the `AppResources` (in order to look up image IDs or create OpenGL textures).
    ///
    /// You can append DOMs recursively, i.e. appending a DOM to a DOM - as well as
    /// breaking up your rendering into separate functions (to re-use DOM components as widgets).
    ///
    /// ## Example
    ///
    /// ```rust
    /// use azul_core::{dom::Dom, traits::Layout, callbacks::LayoutInfo};
    ///
    /// struct MyDataModel { }
    ///
    /// impl Layout for MyDataModel {
    ///     fn layout(&self, _: LayoutInfo) -> Dom<MyDataModel> {
    ///         Dom::label("Hello World!").with_id("my-label")
    ///     }
    /// }
    ///
    /// // The layout() function is called by Azul:
    /// // let new_ui = MyDataModel::layout();
    /// ```
    fn layout(&self, layout_info: LayoutInfo) -> Dom<Self> where Self: Sized;
}

pub trait GetTextLayout {
    fn get_text_layout(&mut self, text_layout_options: &ResolvedTextLayoutOptions) -> InlineTextLayout;
}