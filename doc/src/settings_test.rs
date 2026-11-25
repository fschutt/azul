use std::{ffi::OsString, path::PathBuf};

use azul_core::resources::{TextAntialiasMode, TextAntialiasing};
use azul_css::env::{MapEnv, NoEnv};
use azul_layout::settings::LayoutSettings;

use super::*;

/// Every variable the tool reads, with a value that tells the fields apart.
fn everything() -> MapEnv {
    MapEnv::new()
        .with("AZUL_ROOT", "/checkout")
        .with("CARGO_TARGET_DIR", "build")
        .with("HOME", "/home/u")
        .with("PATH", "/usr/bin:/bin")
        .with("TMPDIR", "/scratch")
        .with("AZUL_DOC_CACHE", "/cache/doc")
        .with("XDG_CACHE_HOME", "/xdg")
        .with("AZ_ANDROID_EXTRA_FEATURES", "azul/x")
        .with("ANDROID_NDK_HOME", "/ndk")
        .with("ANDROID_AVD_HOME", "/avd")
        .with("ANDROID_HOME", "/sdk")
        .with("ANDROID_SDK_ROOT", "/sdk-root")
        .with("JAVA_HOME", "/jdk")
        .with("CHROME", "/opt/chrome")
        .with("PROGRAMFILES", "D:\\Programs")
        .with("PROGRAMFILES(X86)", "D:\\Programs (x86)")
        .with("AZ_TEXT_AA", "grayscale")
        .with("CLAUDECODE", "1")
        .with("ANTHROPIC_API_KEY", "an-api-key-value")
        .with("AZ_DOC_TG_TOKEN", "doc-token")
        .with("TELEGRAM_BOT_TOKEN", "bot-token")
        .with("AZ_DOC_TG_CHAT_ID", "42")
        .with("TELEGRAM_CHAT_ID", "43")
}

#[test]
fn an_empty_environment_is_the_default_of_every_field() {
    let none = ToolSettings::from_lookup(&NoEnv);
    assert_eq!(none, ToolSettings::default());
    assert_eq!(none.azul_root, None);
    assert_eq!(none.cargo_target_dir, None);
    assert_eq!(none.home, None);
    assert_eq!(none.path, None);
    assert_eq!(none.temp_dir, PathBuf::from("/tmp"));
    assert_eq!(none.doc_cache, None);
    assert_eq!(none.xdg_cache_home, None);
    assert!(!none.android_extra_features);
    assert_eq!(none.android_ndk_home, None);
    assert_eq!(none.android_avd_home, None);
    assert_eq!(none.android_home, None);
    assert_eq!(none.android_sdk_root, None);
    assert_eq!(none.java_home, None);
    assert_eq!(none.chrome, None);
    assert_eq!(none.program_files, None);
    assert_eq!(none.program_files_x86, None);
    assert_eq!(none.text_aa, TextAntialiasing::default());
    assert_eq!(none.layout, LayoutSettings::default());
    assert!(!none.inside_claude_code);
    assert!(!none.anthropic_api_key_set);
    assert_eq!(none.telegram_token, None);
    assert_eq!(none.telegram_chat_id, None);
}

#[test]
fn every_variable_reaches_its_field() {
    let s = ToolSettings::from_lookup(&everything());
    assert_eq!(s.azul_root, Some(PathBuf::from("/checkout")));
    assert_eq!(s.cargo_target_dir, Some(PathBuf::from("build")));
    assert_eq!(s.home, Some(OsString::from("/home/u")));
    assert_eq!(s.path, Some(OsString::from("/usr/bin:/bin")));
    assert_eq!(s.temp_dir, PathBuf::from("/scratch"));
    assert_eq!(s.doc_cache, Some(PathBuf::from("/cache/doc")));
    assert_eq!(s.xdg_cache_home, Some(PathBuf::from("/xdg")));
    assert!(s.android_extra_features);
    assert_eq!(s.android_ndk_home, Some(PathBuf::from("/ndk")));
    assert_eq!(s.android_avd_home, Some(PathBuf::from("/avd")));
    assert_eq!(s.android_home, Some(PathBuf::from("/sdk")));
    assert_eq!(s.android_sdk_root, Some(PathBuf::from("/sdk-root")));
    assert_eq!(s.java_home, Some(PathBuf::from("/jdk")));
    assert_eq!(s.chrome.as_deref(), Some("/opt/chrome"));
    assert_eq!(s.program_files.as_deref(), Some("D:\\Programs"));
    assert_eq!(s.program_files_x86.as_deref(), Some("D:\\Programs (x86)"));
    assert_eq!(s.text_aa.mode, TextAntialiasMode::Grayscale);
    assert!(s.inside_claude_code);
    assert!(s.anthropic_api_key_set);
    assert_eq!(s.telegram_token.as_deref(), Some("doc-token"));
    assert_eq!(s.telegram_chat_id.as_deref(), Some("42"));
}

#[test]
fn the_tool_s_own_telegram_names_win_and_the_standard_ones_are_the_fallback() {
    let standard = MapEnv::new()
        .with("TELEGRAM_BOT_TOKEN", "bot-token")
        .with("TELEGRAM_CHAT_ID", "43");
    let s = ToolSettings::from_lookup(&standard);
    assert_eq!(s.telegram_token.as_deref(), Some("bot-token"));
    assert_eq!(s.telegram_chat_id.as_deref(), Some("43"));

    // an empty value is a value, as `std::env::var(..).or_else(..)` took it
    let empty = MapEnv::new()
        .with("AZ_DOC_TG_TOKEN", "")
        .with("TELEGRAM_BOT_TOKEN", "bot-token");
    let s = ToolSettings::from_lookup(&empty);
    assert_eq!(s.telegram_token.as_deref(), Some(""));
}

#[test]
fn the_text_anti_aliasing_is_the_engine_s_reading_of_the_lookup() {
    let tuned = MapEnv::new()
        .with("AZ_TEXT_AA", "legacy")
        .with("AZ_LCD_CONTRAST", "9");
    let s = ToolSettings::from_lookup(&tuned);
    assert_eq!(s.text_aa.mode, TextAntialiasMode::LcdLegacy);
    assert_eq!(s.text_aa.lcd_contrast, 9);
    // the reftest pins the mode and keeps what else the environment tuned
    let pinned = TextAntialiasing {
        mode: TextAntialiasMode::LcdLegacy,
        ..ToolSettings::from_lookup(&MapEnv::new().with("AZ_LCD_CONTRAST", "9")).text_aa
    };
    assert_eq!(pinned, s.text_aa);
}

/// The engine's switches are not named in the doc tool: the whole lookup goes to the engine's own
/// constructor, so whatever its groups read (now or when a cluster adds a group) reaches the
/// `LayoutWindow`s the tool makes.
#[test]
fn the_layout_settings_are_what_the_engine_reads_from_the_same_lookup() {
    for env in [
        MapEnv::new(),
        MapEnv::new().with("AZ_NO_DL_PATCH", "1"),
        MapEnv::new().with("AZ_PATCH_VERIFY", "1").with("AZ_TEXT_HINTING", "0"),
        everything(),
    ] {
        assert_eq!(
            ToolSettings::from_lookup(&env).layout,
            LayoutSettings::from_lookup(&env)
        );
    }
}

#[test]
fn home_and_path_read_as_text_like_the_calls_they_replace() {
    let s = ToolSettings::from_lookup(&everything());
    assert_eq!(s.home_dir(), Some(PathBuf::from("/home/u")));
    assert_eq!(s.home_utf8(), Some("/home/u"));
    assert_eq!(s.path_utf8(), "/usr/bin:/bin");
    let none = ToolSettings::default();
    assert_eq!(none.home_dir(), None);
    assert_eq!(none.home_utf8(), None);
    assert_eq!(none.path_utf8(), "");
}

/// `std::env::var(..).is_ok()` is false for a value that is not UTF-8 and `var_os(..).is_some()`
/// is true: the flags keep each caller's question.
#[cfg(unix)]
#[test]
fn a_value_that_is_not_utf8_is_set_for_var_os_questions_and_unset_for_var_questions() {
    use std::os::unix::ffi::OsStringExt;
    let raw = || OsString::from_vec(vec![0x66, 0x6f, 0x80]);
    let env = MapEnv::new()
        .with_os("CLAUDECODE", raw())
        .with_os("ANTHROPIC_API_KEY", raw())
        .with_os("AZ_ANDROID_EXTRA_FEATURES", raw())
        .with_os("HOME", raw())
        .with_os("PATH", raw());
    let s = ToolSettings::from_lookup(&env);
    assert!(!s.inside_claude_code, "var(..).is_ok() is false");
    assert!(!s.anthropic_api_key_set, "var(..).is_ok() is false");
    assert!(s.android_extra_features, "var_os(..).is_some() is true");
    assert_eq!(s.home, Some(raw()), "a path keeps its bytes");
    assert_eq!(s.home_utf8(), None, "var(\"HOME\").ok() is None");
    assert_eq!(s.path_utf8(), "", "var(\"PATH\").unwrap_or_default() is empty");
}

#[test]
fn the_debug_output_hides_the_bot_token() {
    let shown = format!("{:?}", ToolSettings::from_lookup(&everything()));
    assert!(!shown.contains("doc-token"), "{shown}");
    assert!(!shown.contains("an-api-key-value"), "{shown}");
    assert!(shown.contains("<set>"), "{shown}");
}
