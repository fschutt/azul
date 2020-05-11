use azul_core::window::WindowCreateOptions;
use azul_core::callbacks::LayoutCallback;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct AppConfig {

}

impl Default for AppConfig {
    fn default() -> Self { AppConfig { } }
}

pub struct App<T> {
    pub data: T,
    pub config: AppConfig,
    pub callback: LayoutCallback,
}

impl<T> App<T> {

    pub fn new(data: T, config: AppConfig, callback: LayoutCallback) -> Self {
        Self { data, config, callback }
    }

    pub fn run(self, _window: WindowCreateOptions) -> ! {
        loop { }
    }
}