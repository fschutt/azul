//! Dump the SystemStyle that azul actually detects on this machine.
//!
//! The public `AzSystemStyle_detect()` is a STUB that returns hard-coded
//! platform defaults. The REAL runtime discovery (XDG portal / gsettings /
//! kreadconfig on Linux) lives in azul-dll and runs inside `App::create()`,
//! which overwrites `config.system_style`. This example uses the internal
//! `App` type to surface that real result for comparison against the desktop's
//! actual settings (e.g. `kreadconfig6` / KDE System Settings).
//!
//! Run:
//!     cargo run --release -p azul-dll --example system_style --features link-static
//!
//! It does NOT open a window or run the event loop.

use azul::desktop::app::App;
use azul_core::refany::RefAny;
use azul_core::resources::AppConfig;

fn main() {
    let app = App::create(RefAny::new(()), AppConfig::default());
    let style = &app.ptr.config.system_style;
    println!("===== azul detected SystemStyle =====");
    println!("{:#?}", style);
}
