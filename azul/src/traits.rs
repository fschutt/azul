use dom::Dom;
use std::sync::{Arc, Mutex};

#[cfg(not(test))]
use window::WindowInfo;

/// The core trait that has to be implemented for the app model to provide a
/// Model -> View serialization.
pub trait Layout {
    /// Updates the DOM, must be provided by the final application.
    ///
    /// On each frame, a completely new DOM tree is generated. The final
    /// application can cache the DOM tree, but this isn't in the scope of `azul`.
    ///
    /// The `style_dom` looks through the given DOM rules, applies the style and
    /// recalculates the layout. This is done on each frame (except there are shortcuts
    /// when the DOM doesn't have to be recalculated).
    #[cfg(not(test))]
    fn layout(&self, window_id: WindowInfo<Self>) -> Dom<Self>
    where
        Self: Sized;
    #[cfg(test)]
    fn layout(&self) -> Dom<Self>
    where
        Self: Sized;
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
    fn modify<F>(&self, closure: F) -> bool
    where
        F: FnOnce(&mut T);
}

impl<T> Modify<T> for Arc<Mutex<T>> {
    fn modify<F>(&self, closure: F) -> bool
    where
        F: FnOnce(&mut T),
    {
        match self.lock().as_mut() {
            Ok(lock) => {
                closure(&mut *lock);
                true
            }
            Err(_) => false,
        }
    }
}
