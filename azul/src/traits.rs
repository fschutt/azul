use std::sync::{Arc, Mutex};
use {
    dom::Dom,
};

#[cfg(not(test))]
use callbacks::LayoutInfo;

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
    /// use azul::{dom::Dom, traits::Layout, callbacks::LayoutInfo};
    ///
    /// struct MyDataModel { }
    ///
    /// impl Layout for MyDataModel {
    ///     fn layout(&self, _: LayoutInfo<MyDataModel>) -> Dom<MyDataModel> {
    ///         Dom::label("Hello World!").with_id("my-label")
    ///     }
    /// }
    ///
    /// // This is done by azul internally
    /// // let new_ui = MyDataModel::layout();
    /// ```
    #[cfg(not(test))]
    fn layout(&self, layout_info: LayoutInfo<Self>) -> Dom<Self> where Self: Sized;
    #[cfg(test)]
    fn layout(&self) -> Dom<Self> where Self: Sized;
}

/// Convenience trait that allows the `app_state.modify()` - only implemented for
/// `Arc<Mutex<T>` - shortly locks the app state mutex, modifies it and unlocks
/// it again.
///
/// Note: Usually when doing asynchronous programming you don't want to block the main
/// UI. While Rust executes the `app_state.modify()` closure, your `AppState` gets
/// locked, meaning that no layout can happen and no other thread or callback can write
/// to the apps data. In order to make your app performant, don't do heavy computations
/// inside the closure, only use it to write or copy data in and out of the application
/// state.
pub trait Modify<T> {

    /// Modifies the app state and then returns if the modification was successful
    /// Takes a FnMut that modifies the state
    fn modify<F>(&self, closure: F) -> Option<()> where F: FnOnce(&mut T);

    /// Same as `.modify`, but the closure can now copy some data out of the locked data model.
    fn modify_clone<F, S>(&self, closure: F) -> Option<S> where F: FnOnce(&mut T) -> S {
        let mut initial: Option<S> = None;
        self.modify(|lock| initial = Some(closure(lock)))?;
        initial
    }

    /// Same as `.modify`, but the closure can now return `Option<()>`
    fn modify_opt<F>(&self, closure: F) -> Option<()> where F: FnOnce(&mut T) -> Option<()> {
        self.modify_clone(|lock| {
            let result: Option<()> = closure(lock);
            result
        })?
    }
    /// Same as `.modify_opt`, but the closure returns `Option<S>` instead of `S`.
    fn modify_opt_clone<F, S>(&self, closure: F) -> Option<S> where F: FnOnce(&mut T) -> Option<S> {
        let mut initial: Option<S> = None;
        self.modify(|lock| initial = closure(lock))?;
        initial
    }
}

impl<T> Modify<T> for Arc<Mutex<T>> {
    fn modify<F>(&self, closure: F) -> Option<()> where F: FnOnce(&mut T) {
        let mut lock = self.lock().ok()?;
        closure(&mut *lock);
        Some(())
    }
}
