use azul_core::window::WindowCreateOptions;
use azul_core::callbacks::LayoutCallback;
use azul_core::traits::Layout;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct AppConfig {

}

impl Default for AppConfig {
    fn default() -> Self { AppConfig { } }
}

pub struct App<T> {
    pub data: T,
    pub config: AppConfig,
    pub callback: LayoutCallback<T>,
}

impl<T: Layout> App<T> {
    pub fn new(data: T, config: AppConfig) -> Self {
        Self::new_from_callback(data, config, T::layout)
    }
}

impl<T> App<T> {

    pub fn new_from_callback(data: T, config: AppConfig, callback: LayoutCallback<T>) -> Self {
        Self { data, config, callback }
    }

    pub fn run(self, _window: WindowCreateOptions<T>) -> ! {
        loop { }
    }
}