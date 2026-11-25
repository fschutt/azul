//! `AppConfig::create` of azul-core reads no environment (env pass 2026-11-23): with the
//! variables that used to change it set in the process, the config is the default one.
//!
//! What the environment asks for is `TextAntialiasing::from_lookup`'s business, and the dll's
//! `AppConfig::create` (the C API's, `azul_dll::env::app_config_create`) puts it on the config.
//!
//! The file is a test binary of its own with ONE test, so the variables are set before anything
//! reads them and no other test thread races them.

#![cfg(feature = "std")]

use azul_core::resources::{AppConfig, TextAntialiasMode, TextAntialiasing};

#[test]
fn the_config_is_the_same_whatever_the_environment_holds() {
    let before = AppConfig::create().text_antialiasing;
    assert_eq!(before, TextAntialiasing::default());

    std::env::set_var("AZ_TEXT_AA", "grayscale");
    std::env::set_var("AZ_LCD_CONTRAST", "77");
    std::env::set_var("AZ_LCD_PRETILE", "0");

    let config = AppConfig::create();
    assert_eq!(
        config.text_antialiasing,
        TextAntialiasing::default(),
        "AppConfig::create read the process environment"
    );
    assert_eq!(
        AppConfig::default().text_antialiasing,
        TextAntialiasing::default()
    );

    // The environment is really set: the reader that is asked for it sees it.
    let asked = TextAntialiasing::from_lookup(&|name: &str| std::env::var(name).ok());
    assert_eq!(asked.mode, TextAntialiasMode::Grayscale);
    assert_eq!(asked.lcd_contrast, 77);
    assert!(!asked.lcd_pretile);
}
