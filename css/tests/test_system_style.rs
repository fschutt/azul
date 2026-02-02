//! Test SystemStyle discovery

#[test]
#[cfg(feature = "io")]
fn test_system_style_discovery_selection_colors() {
    use azul_css::system::SystemStyle;
    use azul_css::color::ColorU;
    
    let style = SystemStyle::detect();
    
    println!("\n=== SystemStyle Discovery Test ===");
    println!("Theme: {:?}", style.theme);
    println!("OS: {:?}", style.os);
    println!("Version: {:?}", style.version);
    
    println!("\n=== Colors ===");
    println!("Accent: {:?}", style.colors.accent);
    println!("Selection Background: {:?}", style.colors.selection_background);
    println!("Selection Text: {:?}", style.colors.selection_text);
    
    // The selection_background should be set on macOS
    #[cfg(target_os = "macos")]
    {
        match style.colors.selection_background {
            OptionColorU::Some(color) => {
                println!("\n✓ Selection background color detected:");
                println!("  RGB: ({}, {}, {}) Alpha: {}", color.r, color.g, color.b, color.a);
                
                // Check if it matches user's red accent color
                // AppleHighlightColor was: 1.000000 0.733333 0.721569 Red
                // That's approximately RGB(255, 187, 184)
                if color.r > 200 {
                    println!("  ✓ RED accent color detected - matches user's macOS settings!");
                }
            }
            OptionColorU::None => {
                println!("\n✗ No selection background color detected!");
                println!("  This might mean AppleHighlightColor couldn't be read.");
            }
        }
    }
}
