use traits::LayoutScreen;

/// Wrapper for your application data. In order to be layout-able,
/// you need to satisfy the `LayoutScreen` trait (how the application
/// should be laid out)
pub struct AppState<T: LayoutScreen> {
    /// Your data (the global struct which all callbacks will have access to)
    pub data: T,
}

impl<T: LayoutScreen> AppState<T> {

    /// Creates a new `AppState`
    pub fn new(initial_data: T) -> Self {
        Self {
            data: initial_data,
        }
    }
}