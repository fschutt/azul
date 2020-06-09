    #[no_mangle]
    #[repr(C)]
    pub struct AzRefAny {
        pub _internal_ptr: *const c_void,
        pub _internal_len: usize,
        pub _internal_layout_size: usize,
        pub _internal_layout_align: usize,
        pub type_id: u64,
        pub type_name: AzString,
        pub strong_count: usize,
        pub is_currently_mutable: bool,
        pub custom_destructor: fn(AzRefAny),
    }

    /// Return type of a regular callback - currently `AzUpdateScreen`
    pub type AzCallbackReturn = AzUpdateScreen;
    /// Callback for responding to window events
    pub type AzCallbackType = fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallbackType = fn(AzRefAny, AzLayoutInfoPtr) -> AzDom;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallbackType = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturn;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallbackType = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturn;

    pub type AzTimerCallbackType = fn(AzTimerCallbackInfoPtr) -> AzTimerCallbackReturn;
    pub type AzThreadCallbackType = fn(AzRefAny) -> AzRefAny;
    pub type AzTaskCallbackType= fn(AzArcMutexRefAnyPtr, AzDropCheckPtr) -> AzUpdateScreen;

    impl From<AzOn> for AzEventFilter {
        fn from(on: AzOn) -> AzEventFilter {
            on.into_event_filter()
        }
    }

