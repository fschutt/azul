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
        pub custom_destructor: fn(RefAny),
    }
