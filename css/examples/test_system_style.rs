use azul_css::system::SystemStyle;

fn main() {
    println!("Testing SystemStyle discovery on macOS...\n");
    
    let style = SystemStyle::discover();
    
    println!("Theme: {:?}", style.theme);
    println!("Language: {:?}", style.language);
    println!("OS: {:?}", style.os);
    println!("Version: {:?}", style.version);
    println!();
    
    println!("=== System Colors ===");
    println!("Accent: {:?}", style.colors.accent);
    println!("Selection Background: {:?}", style.colors.selection_background);
    println!("Selection Text: {:?}", style.colors.selection_text);
    println!("Text Primary: {:?}", style.colors.text_primary);
    println!("Background: {:?}", style.colors.background);
    println!();
    
    // Check if selection colors are the user's custom color (red)
    if let azul_css::color::OptionColorU::Some(sel_bg) = style.colors.selection_background {
        println!("Selection Background RGB: r={}, g={}, b={}, a={}", 
            sel_bg.r, sel_bg.g, sel_bg.b, sel_bg.a);
        
        // Check if it's red-ish (user has red accent color)
        if sel_bg.r > 200 && sel_bg.g < 200 && sel_bg.b < 200 {
            println!("✓ Detected RED accent color - matches user's macOS settings!");
        } else if sel_bg.r < 100 && sel_bg.g < 150 && sel_bg.b > 200 {
            println!("✗ Detected BLUE accent color - this is the default, not user's custom red!");
        }
    }
}
