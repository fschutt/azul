    /// `UpdateScreen` struct
    pub use crate::dll::AzUpdateScreen as UpdateScreen;

    impl std::fmt::Debug for UpdateScreen { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}", (crate::dll::get_azul_dll().az_update_screen_fmt_debug)(self)) } }
    impl Clone for UpdateScreen { fn clone(&self) -> Self { (crate::dll::get_azul_dll().az_update_screen_deep_copy)(self) } }
    impl Drop for UpdateScreen { fn drop(&mut self) { (crate::dll::get_azul_dll().az_update_screen_delete)(self); } }

    impl From<Option<()>> for UpdateScreen { fn from(o: Option<()>) -> Self { match o { None => UpdateScreen::DontRedraw, Some(_) => UpdateScreen::Redraw } } }

    impl From<UpdateScreen> for Option<()> { fn from(o: UpdateScreen) -> Self { match o { UpdateScreen::Redraw => Some(()), _ => None } } }
