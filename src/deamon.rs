use app_state::AppState;
use std::sync::{Arc, Mutex};
use traits::LayoutScreen;
use window::WindowId;
use dom::UpdateScreen;

pub(crate) struct DeamonCallback<T: LayoutScreen> {
    pub(crate) callback: fn(&mut T) -> UpdateScreen,
}

impl<T: LayoutScreen> DeamonCallback<T> {
    pub(crate) fn new(callback: fn(&mut T) -> UpdateScreen) -> Self {
        Self { callback }
    }
}

impl<T: LayoutScreen> Clone for DeamonCallback<T> {
    fn clone(&self) -> Self {
        Self { callback: self.callback.clone() }
    }
}