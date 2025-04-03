mod codegen;
mod docgen;
mod utils;
mod api;
mod license;

use std::error::Error;
use std::env;
use anyhow::Context;

fn main() -> anyhow::Result<()> {
    println!("Starting Azul Build Runner...");

    // Parse the api.json file
    let api_data = api::ApiData::from_str(include_str!("../../api.json"))
        .context("Failed to parse api.json")?;

    let versions = api_data.get_sorted_versions();
    if versions.is_empty() {
        anyhow::bail!("No versions found in api.json");
    }
    
    let latest_version = api_data.get_latest_version_str().unwrap_or("unknown");
    println!("Latest version detected: {}", latest_version);

    // Generate the Rust DLL code
    println!("Generating Rust DLL code...");
    let rust_dll_code = codegen::rust_dll::generate_rust_dll(&api_data);
    
    // Generate the Rust API code
    println!("Generating Rust API code...");
    let rust_api_code = codegen::rust_api::generate_rust_api(&api_data);
    
    // Generate the C API code
    println!("Generating C API code...");
    let c_api_code = codegen::c_api::generate_c_api(&api_data);
    
    // Generate the C++ API code
    println!("Generating C++ API code...");
    let cpp_api_code = codegen::cpp_api::generate_cpp_api(&api_data);
    
    // Generate the Python API code
    println!("Generating Python API code...");
    let python_api_code = codegen::python_api::generate_python_api(&api_data);
    
    // Generate the tests
    println!("Generating tests...");
    let test_code = codegen::tests::generate_size_test(&api_data);
    
    // Generate the documentation
    println!("Generating documentation...");
    
    // Generate the documentation
    println!("Generating documentation...");
    let docs = docgen::generate_docs(&api_data);
    
    // Generate license files
    println!("Generating license files...");
    let licenses = license::generate_license();

    // Print the generated code - in a real application, you would write this to files
    if env::var("PRINT_OUTPUT").is_ok() {
        
        println!("--- Rust DLL Code ---\n{}", rust_dll_code);
        println!("--- Rust API Code ---\n{}", rust_api_code);
        println!("--- C API Code ---\n{}", c_api_code);
        println!("--- C++ API Code ---\n{}", cpp_api_code);
        println!("--- Python API Code ---\n{}", python_api_code);
        println!("--- Test Code ---\n{}", test_code);
        
        // Print license files
        for (name, content) in &licenses {
            println!("--- License: {} ---\n{}", name, content);
        }
    }

    println!("Build runner completed successfully!");
    Ok(())
}