
use azul_core::icon::*;

#[test]
fn test_icon_provider_new() {
    let provider = IconProviderHandle::new();
    assert!(provider.list_packs().is_empty());
}

// Dummy destructor for test RefAny
extern "C" fn dummy_destructor(_: *mut core::ffi::c_void) {}

#[test]
fn test_icon_registration() {
    let mut provider = IconProviderHandle::new();
    
    // Use a dummy RefAny for testing
    let dummy_data = azul_core::refany::RefAny::new_c(
        core::ptr::null(),
        0,
        1,
        0,
        "".into(),
        dummy_destructor,
        0, // serialize_fn
        0, // deserialize_fn
    );
    
    provider.register_icon("images", "home", dummy_data.clone());
    assert!(provider.has_icon("home"));
    assert!(provider.has_icon("HOME")); // case-insensitive
    
    provider.unregister_icon("images", "home");
    assert!(!provider.has_icon("home"));
}

#[test]
fn test_icon_provider_lookup() {
    let mut provider = IconProviderHandle::new();
    
    let dummy_data = azul_core::refany::RefAny::new_c(
        core::ptr::null(),
        0,
        1,
        0,
        "".into(),
        dummy_destructor,
        0, // serialize_fn
        0, // deserialize_fn
    );
    
    provider.register_icon("images", "logo", dummy_data);
    
    assert!(provider.has_icon("logo"));
    assert!(!provider.has_icon("missing"));
    
    let lookup = provider.lookup("logo");
    assert!(lookup.is_some());
}

#[test]
fn test_pack_operations() {
    let mut provider = IconProviderHandle::new();
    
    let dummy_data = azul_core::refany::RefAny::new_c(
        core::ptr::null(),
        0,
        1,
        0,
        "".into(),
        dummy_destructor,
        0, // serialize_fn
        0, // deserialize_fn
    );
    
    // Register icons in different packs
    provider.register_icon("pack1", "icon1", dummy_data.clone());
    provider.register_icon("pack2", "icon2", dummy_data);
    
    assert_eq!(provider.list_packs().len(), 2);
    assert!(provider.has_icon("icon1"));
    assert!(provider.has_icon("icon2"));
    
    // Unregister entire pack
    provider.unregister_pack("pack1");
    assert!(!provider.has_icon("icon1"));
    assert!(provider.has_icon("icon2"));
    assert_eq!(provider.list_packs().len(), 1);
}
