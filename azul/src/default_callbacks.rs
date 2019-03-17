use std::{
    collections::BTreeMap,
    fmt,
    hash::Hasher,
    sync::atomic::{AtomicUsize, Ordering},
};
use {
    dom::{UpdateScreen, DontRedraw},
    traits::Layout,
    app::AppStateNoData,
    window::CallbackInfo,
};
pub use stack_checked_pointer::StackCheckedPointer;

pub type DefaultCallbackType<T, U> = fn(&mut U, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>) -> UpdateScreen;
pub type DefaultCallbackTypeUnchecked<T> = fn(&StackCheckedPointer<T>, app_state_no_data: &mut AppStateNoData<T>, window_event: &mut CallbackInfo<T>) -> UpdateScreen;

static LAST_DEFAULT_CALLBACK_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct DefaultCallbackId(usize);

pub(crate) fn get_new_unique_default_callback_id() -> DefaultCallbackId {
    DefaultCallbackId(LAST_DEFAULT_CALLBACK_ID.fetch_add(1, Ordering::SeqCst))
}

pub struct DefaultCallback<T: Layout>(pub DefaultCallbackTypeUnchecked<T>);

impl_callback_bounded!(DefaultCallback<T: Layout>);

pub(crate) struct DefaultCallbackSystem<T: Layout> {
    callbacks: BTreeMap<DefaultCallbackId, (StackCheckedPointer<T>, DefaultCallback<T>)>,
}

impl<T: Layout> DefaultCallbackSystem<T> {

    /// Creates a new, empty list of callbacks
    pub(crate) fn new() -> Self {
        Self {
            callbacks: BTreeMap::new(),
        }
    }

    pub fn add_callback(
        &mut self,
        callback_id: DefaultCallbackId,
        ptr: StackCheckedPointer<T>,
        func: DefaultCallback<T>)
    {
        self.callbacks.insert(callback_id, (ptr, func));
    }

    /// NOTE: `app_data` is required so we know that we don't
    /// accidentally alias the data in `self.internal` (which could lead to UB).
    ///
    /// What we know is that the pointer (`self.internal`) points to somewhere
    /// in `T`, so we know that `self.internal` isn't aliased
    pub(crate) fn run_callback(
        &self,
        _app_data: &mut T,
        callback_id: &DefaultCallbackId,
        app_state_no_data: &mut AppStateNoData<T>,
        window_event: &mut CallbackInfo<T>)
    -> UpdateScreen
    {
        if let Some((callback_ptr, callback_fn)) = self.callbacks.get(callback_id) {
            (callback_fn.0)(callback_ptr, app_state_no_data, window_event)
        } else {
            #[cfg(feature = "logging")] {
                warn!("Calling default callback with invalid ID {:?}", callback_id);
            }
            DontRedraw
        }
    }

    /// Clears all callbacks
    pub(crate) fn clear(&mut self) {
        self.callbacks.clear();
    }
}

impl<T: Layout> Clone for DefaultCallbackSystem<T> {
    fn clone(&self) -> Self {
        Self {
            callbacks: self.callbacks.clone(),
        }
    }
}