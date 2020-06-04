    /// `UpdateScreen` struct
    use crate::dll::AzUpdateScreen as UpdateScreen;

    impl<T> From<Option<T>> for UpdateScreen { fn from(o: Option<T>) -> Self { Self { object: match o { None => AzDontRedraw, Some(_) => AzRedraw }} } }
