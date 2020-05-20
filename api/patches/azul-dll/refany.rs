/// Pointer to rust-allocated `Box<RefAny>` struct
pub use ::azul_core::callbacks::RefAny as AzRefAny;

/// Creates a new `RefAny` instance
#[no_mangle] pub extern "C" fn az_ref_any_new(ptr: *const u8, len: usize, type_id: u64, type_name: AzStringPtr, custom_destructor: fn(AzRefAny)) -> AzRefAny {
    AzRefAny::new_c(ptr, len, type_id, *az_string_downcast(type_name), custom_destructor)
}
/// Returns the internal pointer of the `RefAny` as a `*mut c_void` or a nullptr if the types don't match
#[no_mangle] pub extern "C" fn az_ref_any_get_ptr(ptr: &AzRefAny, len: usize, type_id: u64) -> *const c_void { ptr.get_ptr(len, type_id) }
/// Returns the internal pointer of the `RefAny` as a `*mut c_void` or a nullptr if the types don't match
#[no_mangle] pub extern "C" fn az_ref_any_get_mut_ptr(ptr: &AzRefAny, len: usize, type_id: u64) -> *mut c_void { ptr.get_mut_ptr(len, type_id) }
/// Creates a new reference of the pointer, pointing to the same object: WARNING: After calling this function you'll have two pointers to the same Box<`RefAny`>!.
#[no_mangle] pub extern "C" fn az_ref_any_shallow_copy(ptr: &AzRefAny) -> AzRefAny { ptr.clone() }
/// Destructor: Takes ownership of the `RefAny` pointer and deletes it.
#[no_mangle] pub extern "C" fn az_ref_any_delete(ptr: &mut AzRefAny) { az_ref_any_core_copy(ptr).drop_c() }
/// Copies the pointer without invoking the destructor
#[no_mangle] pub extern "C" fn az_ref_any_core_copy(ptr: &AzRefAny) -> AzRefAny {
    AzRefAny {
        _internal_ptr: ptr._internal_ptr,
        _internal_len: ptr._internal_len,
        _internal_layout_size: ptr._internal_layout_size,
        _internal_layout_align: ptr._internal_layout_align,
        type_id: ptr.type_id,
        type_name: ptr.type_name.clone(),
        strong_count: ptr.strong_count,
        is_currently_mutable: ptr.is_currently_mutable,
        custom_destructor: ptr.custom_destructor,
    }
}
