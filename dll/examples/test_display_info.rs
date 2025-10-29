//! Test display/monitor enumeration on all platforms
//!
//! This example demonstrates the cross-platform display information API.

use azul_dll::desktop::display::{get_displays, get_primary_display, DisplayInfo};

fn main() {
    println!("=== Display/Monitor Enumeration Test ===\n");
    
    // Get all displays
    let displays = get_displays();
    
    println!("Found {} display(s):\n", displays.len());
    
    for (i, display) in displays.iter().enumerate() {
        println!("Display {}:", i);
        println!("  Name: {}", display.name);
        println!("  Bounds: {} x {} @ ({}, {})", 
            display.bounds.size.width,
            display.bounds.size.height,
            display.bounds.origin.x,
            display.bounds.origin.y
        );
        println!("  Work Area: {} x {} @ ({}, {})", 
            display.work_area.size.width,
            display.work_area.size.height,
            display.work_area.origin.x,
            display.work_area.origin.y
        );
        println!("  Scale Factor: {:.2}", display.scale_factor);
        println!("  Is Primary: {}", display.is_primary);
        
        // Calculate DPI
        let dpi = display.scale_factor * 96.0;
        println!("  Effective DPI: {:.0}", dpi);
        println!();
    }
    
    // Test primary display
    if let Some(primary) = get_primary_display() {
        println!("Primary display: {}", primary.name);
        println!("  Resolution: {} x {}", 
            primary.bounds.size.width,
            primary.bounds.size.height
        );
    } else {
        println!("No primary display found!");
    }
    
    println!("\n=== Test Complete ===");
}
