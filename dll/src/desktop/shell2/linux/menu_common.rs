//! Shared menu layout callback and data types for Linux (X11 and Wayland).

use azul_core::{
    callbacks::LayoutCallbackInfo,
    menu::Menu,
    refany::RefAny,
};
use azul_css::system::SystemStyle;

use super::common::debug_server::LogCategory;
use crate::log_error;

#[derive(Debug, Clone)]
pub(crate) struct MenuLayoutData {
    pub menu: Menu,
    pub system_style: SystemStyle,
}

pub(crate) extern "C" fn menu_layout_callback(mut data: RefAny, _info: LayoutCallbackInfo) -> azul_core::dom::Dom {
    let data_clone = data.clone();

    let menu_data = match data.downcast_ref::<MenuLayoutData>() {
        Some(d) => d,
        None => {
            log_error!(
                LogCategory::Layout,
                "[Menu Layout] Failed to downcast menu data"
            );
            return azul_core::dom::Dom::create_body();
        }
    };

    crate::desktop::menu_renderer::create_menu_dom_with_css(
        &menu_data.menu,
        &menu_data.system_style,
        data_clone,
    )
}
