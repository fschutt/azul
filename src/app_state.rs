use traits::LayoutScreen;

/// Wrapper for your application data. In order to be layout-able,
/// you need to satisfy the `LayoutScreen` trait (how the application
/// should be laid out)
pub struct AppState<T: LayoutScreen> {
    pub data: T,
}

impl<T> AppState<T> where T: LayoutScreen {

    pub fn new(initial_data: T) -> Self {
        Self {
            data: initial_data,
        }
    }
}