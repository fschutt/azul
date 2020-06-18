    /// `UpdateScreen` struct
    use crate::dll::AzUpdateScreen as UpdateScreen;

    impl<T> From<Option<T>> for UpdateScreen { fn from(o: Option<T>) -> Self { match o { None => UpdateScreen::DontRedraw, Some(_) => UpdateScreen::Redraw } } }

    impl From<Option<()>> for UpdateScreen { fn from(o: Option<()>) -> Self { match o { None => UpdateScreen::DontRedraw, Some(_) => UpdateScreen::Redraw } } }

    impl<T> From<UpdateScreen> for Option<()> { fn from(o: UpdateScreen) -> Self { match o { UpdateScreen::DontRedraw => None, UpdateScreen::Redraw => Some(()) } } }
