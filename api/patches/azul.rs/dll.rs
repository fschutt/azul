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
    pub type AzCallback = fn(AzCallbackInfoPtr) -> AzCallbackReturn;
    /// Callback fn that returns the DOM of the app
    pub type AzLayoutCallback = fn(AzRefAny, AzLayoutInfoPtr) -> AzDomPtr;
    /// Callback for rendering to an OpenGL texture
    pub type AzGlCallback = fn(AzGlCallbackInfoPtr) -> AzGlCallbackReturnPtr;
    /// Callback for rendering iframes (infinite data structures that have to know how large they are rendered)
    pub type AzIFrameCallback = fn(AzIFrameCallbackInfoPtr) -> AzIFrameCallbackReturnPtr;
