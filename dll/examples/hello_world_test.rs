//! Simple hello world test for debugging C-API issues
//!
//! Run with: cargo run --bin hello_world_test --package azul-dll --features "c-api desktop"

use azul_core::{
    refany::RefAny,
    resources::AppConfig,
};
use azul_dll::desktop::app::App;

#[derive(Debug, Clone)]
struct MyDataModel {
    counter: u32,
}

fn main() {
    eprintln!("[main] Size checks:");
    eprintln!("  sizeof(RefAny) = {}", std::mem::size_of::<RefAny>());
    eprintln!("  sizeof(azul_core::refany::RefCount) = {}", std::mem::size_of::<azul_core::refany::RefCount>());
    eprintln!("  alignof(RefAny) = {}", std::mem::align_of::<RefAny>());
    eprintln!("  alignof(RefCount) = {}", std::mem::align_of::<azul_core::refany::RefCount>());
    
    eprintln!("[main] Creating MyDataModel...");
    let model = MyDataModel { counter: 5 };
    
    eprintln!("[main] Creating RefAny...");
    let data = RefAny::new(model);
    
    eprintln!("[main] RefAny created, checking clone...");
    let mut data_clone = data.clone();
    eprintln!("[main] Clone successful!");
    
    eprintln!("[main] Checking downcast...");
    if let Some(m) = data_clone.downcast_ref::<MyDataModel>() {
        eprintln!("[main] Downcast successful, counter = {}", m.counter);
    } else {
        eprintln!("[main] Downcast failed!");
    }
    
    eprintln!("[main] Creating AppConfig...");
    let config = AppConfig::new();
    
    eprintln!("[main] Creating App...");
    let _app = App::new(data, config);
    
    eprintln!("[main] App created successfully!");
    
    // Don't actually run the app for now, just test creation
    eprintln!("[main] Test passed!");
}
