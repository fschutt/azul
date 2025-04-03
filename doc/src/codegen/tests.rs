use crate::api::ApiData;
use std::collections::{BTreeMap, HashMap};
use indexmap::IndexMap;

/// Generate size test code that verifies struct layouts
pub fn generate_size_test(api_data: &ApiData) -> String {
    let mut test_str = String::new();
    
    // Get the latest version
    let latest_version = api_data.get_latest_version_str().unwrap();
    let version_data = api_data.get_version(latest_version).unwrap();
    
    test_str.push_str("#[cfg(all(test, not(feature = \"rlib\")))]\r\n");
    test_str.push_str("#[allow(dead_code)]\r\n");
    test_str.push_str("mod test_sizes {\r\n");
    
    // Add test framework code
    test_str.push_str("    // Test framework code would be included here from patch file\r\n\r\n");
    
    // Generate struct definitions
    test_str.push_str("    // Generated struct definitions would go here\r\n\r\n");
    
    test_str.push_str("    use core::ffi::c_void;\r\n");
    test_str.push_str("    use azul_impl::css::*;\r\n");
    test_str.push_str("\r\n");
    
    test_str.push_str("    #[test]\r\n");
    test_str.push_str("    fn test_size() {\r\n");
    test_str.push_str("        use core::alloc::Layout;\r\n");
    
    // Loop through all classes in all modules
    for (module_name, module) in &version_data.modules {
        for (class_name, class_data) in &module.classes {
            if let Some(external) = &class_data.external {
                let struct_name = format!("Az{}", class_name);
                
                test_str.push_str(&format!(
                    "        assert_eq!((Layout::new::<{}>(), \"{}\"), (Layout::new::<{}>(), \"{}\"));\r\n",
                    external, struct_name, struct_name, struct_name
                ));
            }
        }
    }
    
    test_str.push_str("    }\r\n");
    test_str.push_str("}\r\n");
    
    test_str
}