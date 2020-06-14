    impl From<On> for AzEventFilter {
        fn from(on: On) -> AzEventFilter {
            on.into_event_filter()
        }
    }