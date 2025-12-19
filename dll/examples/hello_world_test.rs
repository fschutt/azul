//! Simple hello world test for debugging C-API issues
//!
//! Run with: cargo run --bin hello_world_test --package azul-dll --features "c-api desktop"

use azul_core::{refany::RefAny, resources::AppConfig};
use azul_dll::desktop::app::App;
use azul_layout::window_state::{FullWindowState, WindowCreateOptions};

#[derive(Debug, Clone)]
struct MyDataModel {
    counter: u32,
}

fn main() {
    eprintln!("[main] Size checks:");
    eprintln!("  sizeof(RefAny) = {}", std::mem::size_of::<RefAny>());
    eprintln!(
        "  sizeof(azul_core::refany::RefCount) = {}",
        std::mem::size_of::<azul_core::refany::RefCount>()
    );
    eprintln!("  alignof(RefAny) = {}", std::mem::align_of::<RefAny>());
    eprintln!(
        "  alignof(RefCount) = {}",
        std::mem::align_of::<azul_core::refany::RefCount>()
    );

    eprintln!("\n[main] WindowCreateOptions size checks:");
    eprintln!(
        "  sizeof(WindowCreateOptions) = {} bytes",
        std::mem::size_of::<WindowCreateOptions>()
    );
    eprintln!(
        "  alignof(WindowCreateOptions) = {} bytes",
        std::mem::align_of::<WindowCreateOptions>()
    );
    eprintln!(
        "  sizeof(FullWindowState) = {} bytes",
        std::mem::size_of::<FullWindowState>()
    );
    eprintln!(
        "  alignof(FullWindowState) = {} bytes",
        std::mem::align_of::<FullWindowState>()
    );

    // Check nested types
    eprintln!("\n[main] Nested type sizes:");
    eprintln!(
        "  sizeof(OptionRendererOptions) = {} bytes",
        std::mem::size_of::<azul_core::window::OptionRendererOptions>()
    );
    eprintln!(
        "  sizeof(OptionWindowTheme) = {} bytes",
        std::mem::size_of::<azul_core::window::OptionWindowTheme>()
    );
    eprintln!(
        "  sizeof(OptionCallback) = {} bytes",
        std::mem::size_of::<azul_layout::callbacks::OptionCallback>()
    );
    eprintln!(
        "  sizeof(LayoutCallback) = {} bytes",
        std::mem::size_of::<azul_core::callbacks::LayoutCallback>()
    );
    eprintln!(
        "  sizeof(KeyboardState) = {} bytes",
        std::mem::size_of::<azul_core::window::KeyboardState>()
    );
    eprintln!(
        "  sizeof(MouseState) = {} bytes",
        std::mem::size_of::<azul_core::window::MouseState>()
    );
    eprintln!(
        "  sizeof(TouchState) = {} bytes",
        std::mem::size_of::<azul_core::window::TouchState>()
    );
    eprintln!(
        "  sizeof(PlatformSpecificOptions) = {} bytes",
        std::mem::size_of::<azul_core::window::PlatformSpecificOptions>()
    );

    eprintln!("\n[main] PlatformSpecificOptions breakdown:");
    eprintln!(
        "  sizeof(WindowsWindowOptions) = {} bytes",
        std::mem::size_of::<azul_core::window::WindowsWindowOptions>()
    );
    eprintln!(
        "  sizeof(LinuxWindowOptions) = {} bytes",
        std::mem::size_of::<azul_core::window::LinuxWindowOptions>()
    );
    eprintln!(
        "  sizeof(MacWindowOptions) = {} bytes",
        std::mem::size_of::<azul_core::window::MacWindowOptions>()
    );
    eprintln!(
        "  sizeof(WasmWindowOptions) = {} bytes",
        std::mem::size_of::<azul_core::window::WasmWindowOptions>()
    );

    eprintln!("\n[main] LinuxWindowOptions breakdown:");
    eprintln!(
        "  sizeof(OptionX11Visual) = {} bytes",
        std::mem::size_of::<azul_core::window::OptionX11Visual>()
    );
    eprintln!(
        "  sizeof(OptionI32) = {} bytes",
        std::mem::size_of::<azul_css::corety::OptionI32>()
    );
    eprintln!(
        "  sizeof(StringPairVec) = {} bytes",
        std::mem::size_of::<azul_core::window::StringPairVec>()
    );
    eprintln!(
        "  sizeof(XWindowTypeVec) = {} bytes",
        std::mem::size_of::<azul_core::window::XWindowTypeVec>()
    );
    eprintln!(
        "  sizeof(OptionString) = {} bytes",
        std::mem::size_of::<azul_css::OptionString>()
    );
    eprintln!(
        "  sizeof(OptionLogicalSize) = {} bytes",
        std::mem::size_of::<azul_core::geom::OptionLogicalSize>()
    );
    eprintln!(
        "  sizeof(OptionWaylandTheme) = {} bytes",
        std::mem::size_of::<azul_core::window::OptionWaylandTheme>()
    );
    eprintln!(
        "  sizeof(WaylandTheme) = {} bytes",
        std::mem::size_of::<azul_core::window::WaylandTheme>()
    );
    eprintln!(
        "  sizeof(UserAttentionType) = {} bytes",
        std::mem::size_of::<azul_core::window::UserAttentionType>()
    );
    eprintln!(
        "  sizeof(OptionWindowIcon) = {} bytes",
        std::mem::size_of::<azul_core::window::OptionWindowIcon>()
    );
    eprintln!(
        "  sizeof(OptionLinuxDecorationsState) = {} bytes",
        std::mem::size_of::<azul_core::window::OptionLinuxDecorationsState>()
    );

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
    let config = AppConfig::create();

    eprintln!("[main] Creating App...");
    let _app = App::create(data, config);

    eprintln!("[main] App created successfully!");

    // Don't actually run the app for now, just test creation
    eprintln!("[main] Test passed!");
}
