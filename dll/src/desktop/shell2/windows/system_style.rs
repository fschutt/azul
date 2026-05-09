//! Native Windows system style discovery via LoadLibrary + GetProcAddress.
//!
//! This module loads `User32.dll`, `Dwmapi.dll`, and `UxTheme.dll` at runtime,
//! queries system metrics, colours, and input timing, then immediately frees
//! the library handles.
//!
//! No external crates are required — all calls go through `kernel32` functions
//! which are always available on Windows.

#![allow(non_snake_case)]

use core::ffi::c_void;

use alloc::boxed::Box;
use alloc::string::String;
use azul_css::corety::AzString;
use azul_css::css::Css;
use azul_css::dynamic_selector::{OsVersion, BoolCondition};
use azul_css::parser2::new_from_str;
use azul_css::props::basic::color::{ColorU, OptionColorU};
use azul_css::system::{defaults, InputMetrics, TextRenderingHints, SubpixelType, Platform, Theme};

// ── kernel32 functions (always linked on Windows) ────────────────────────

extern "system" {
    fn LoadLibraryA(name: *const u8) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}

// ── Win32 constants ──────────────────────────────────────────────────────

const SM_CXDOUBLECLK: i32 = 36;
const SM_CYDOUBLECLK: i32 = 37;
const SM_CXDRAG: i32 = 68;
const SM_CXVSCROLL: i32 = 2;

const SPI_GETFONTSMOOTHING: u32 = 0x004A;
const SPI_GETFONTSMOOTHINGTYPE: u32 = 0x200A;
const SPI_GETWHEELSCROLLLINES: u32 = 0x0068;
const SPI_GETMOUSEHOVERTIME: u32 = 0x0066;
const SPI_GETCLIENTAREAANIMATION: u32 = 0x1042;
const SPI_GETKEYBOARDCUES: u32 = 0x100A;
const SPI_GETBEEP: u32 = 0x0001;
const SPI_GETCARETWIDTH: u32 = 0x2006;

const FE_FONTSMOOTHINGSTANDARD: u32 = 1;
const FE_FONTSMOOTHINGCLEARTYPE: u32 = 2;

// ── Function pointer types ───────────────────────────────────────────────

type FnGetSystemMetrics = unsafe extern "system" fn(i32) -> i32;
type FnGetDoubleClickTime = unsafe extern "system" fn() -> u32;
type FnGetCaretBlinkTime = unsafe extern "system" fn() -> u32;
type FnSystemParametersInfoW = unsafe extern "system" fn(u32, u32, *mut c_void, u32) -> i32;
type FnGetSysColor = unsafe extern "system" fn(i32) -> u32;
type FnDwmGetColorizationColor = unsafe extern "system" fn(*mut u32, *mut i32) -> i32;

// ── Library wrapper ──────────────────────────────────────────────────────

struct User32 {
    GetSystemMetrics:      FnGetSystemMetrics,
    GetDoubleClickTime:    FnGetDoubleClickTime,
    GetCaretBlinkTime:     FnGetCaretBlinkTime,
    SystemParametersInfoW: FnSystemParametersInfoW,
    GetSysColor:           FnGetSysColor,
    _handle: *mut c_void,
}

impl User32 {
    fn load() -> Option<Self> {
        unsafe {
            let h = LoadLibraryA(b"User32.dll\0".as_ptr());
            if h.is_null() { return None; }

            macro_rules! sym {
                ($name:ident, $ty:ty) => {{
                    let p = GetProcAddress(h, concat!(stringify!($name), "\0").as_ptr());
                    if p.is_null() { FreeLibrary(h); return None; }
                    core::mem::transmute::<_, $ty>(p)
                }};
            }

            Some(User32 {
                GetSystemMetrics:      sym!(GetSystemMetrics, FnGetSystemMetrics),
                GetDoubleClickTime:    sym!(GetDoubleClickTime, FnGetDoubleClickTime),
                GetCaretBlinkTime:     sym!(GetCaretBlinkTime, FnGetCaretBlinkTime),
                SystemParametersInfoW: sym!(SystemParametersInfoW, FnSystemParametersInfoW),
                GetSysColor:           sym!(GetSysColor, FnGetSysColor),
                _handle: h,
            })
        }
    }
}

impl Drop for User32 {
    fn drop(&mut self) { unsafe { FreeLibrary(self._handle); } }
}

struct Dwmapi {
    DwmGetColorizationColor: FnDwmGetColorizationColor,
    _handle: *mut c_void,
}

impl Dwmapi {
    fn load() -> Option<Self> {
        unsafe {
            let h = LoadLibraryA(b"Dwmapi.dll\0".as_ptr());
            if h.is_null() { return None; }
            let p = GetProcAddress(h, b"DwmGetColorizationColor\0".as_ptr());
            if p.is_null() { FreeLibrary(h); return None; }
            Some(Dwmapi {
                DwmGetColorizationColor: core::mem::transmute(p),
                _handle: h,
            })
        }
    }
}

impl Drop for Dwmapi {
    fn drop(&mut self) { unsafe { FreeLibrary(self._handle); } }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn color_from_sys(u32: &FnGetSysColor, index: i32) -> ColorU {
    let c = unsafe { u32(index) };
    // GetSysColor returns 0x00BBGGRR
    let r = (c & 0xFF) as u8;
    let g = ((c >> 8) & 0xFF) as u8;
    let b = ((c >> 16) & 0xFF) as u8;
    ColorU::new_rgb(r, g, b)
}

// ── Public entry point ───────────────────────────────────────────────────

/// Discover Windows system style via LoadLibrary.
///
/// Falls back to `defaults::windows_11_light()` if any DLL fails to load.
pub(crate) fn discover() -> azul_css::system::SystemStyle {
    let u32_lib = match User32::load() {
        Some(l) => l,
        None => return defaults::windows_11_light(),
    };

    let mut style = defaults::windows_11_light();

    unsafe {
        // ── Input metrics ────────────────────────────────────────────
        style.input = InputMetrics {
            double_click_time_ms:    (u32_lib.GetDoubleClickTime)(),
            double_click_distance_px: (u32_lib.GetSystemMetrics)(SM_CXDOUBLECLK) as f32,
            drag_threshold_px:       (u32_lib.GetSystemMetrics)(SM_CXDRAG) as f32,
            caret_blink_rate_ms:     (u32_lib.GetCaretBlinkTime)(),
            caret_width_px: {
                let mut w: u32 = 1;
                (u32_lib.SystemParametersInfoW)(
                    SPI_GETCARETWIDTH, 0,
                    &mut w as *mut u32 as *mut c_void, 0,
                );
                w as f32
            },
            wheel_scroll_lines: {
                let mut lines: u32 = 3;
                (u32_lib.SystemParametersInfoW)(
                    SPI_GETWHEELSCROLLLINES, 0,
                    &mut lines as *mut u32 as *mut c_void, 0,
                );
                lines
            },
            hover_time_ms: {
                let mut hover: u32 = 400;
                (u32_lib.SystemParametersInfoW)(
                    SPI_GETMOUSEHOVERTIME, 0,
                    &mut hover as *mut u32 as *mut c_void, 0,
                );
                hover
            },
        };

        // ── System colours (classic GetSysColor) ─────────────────────
        // COLOR_WINDOW = 5, COLOR_WINDOWTEXT = 8, COLOR_HIGHLIGHT = 13,
        // COLOR_HIGHLIGHTTEXT = 14, COLOR_BTNFACE = 15, COLOR_BTNTEXT = 18,
        // COLOR_GRAYTEXT = 17
        style.colors.window_background = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 5));
        style.colors.text              = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 8));
        style.colors.selection_background = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 13));
        style.colors.selection_text    = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 14));
        style.colors.button_face       = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 15));
        style.colors.button_text       = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 18));
        style.colors.disabled_text     = OptionColorU::Some(color_from_sys(&u32_lib.GetSysColor, 17));

        // ── Text rendering hints ─────────────────────────────────────
        {
            let mut smoothing: i32 = 0;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETFONTSMOOTHING, 0,
                &mut smoothing as *mut i32 as *mut c_void, 0,
            );
            let mut smooth_type: u32 = 0;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETFONTSMOOTHINGTYPE, 0,
                &mut smooth_type as *mut u32 as *mut c_void, 0,
            );
            style.text_rendering = TextRenderingHints {
                font_smoothing_enabled: smoothing != 0,
                subpixel_type: if smooth_type == FE_FONTSMOOTHINGCLEARTYPE {
                    SubpixelType::Rgb // ClearType defaults to horizontal RGB
                } else {
                    SubpixelType::None
                },
                font_smoothing_gamma: 1000,
                increased_contrast: false,
            };
        }

        // ── DWM accent colour ────────────────────────────────────────
        if let Some(dwm) = Dwmapi::load() {
            let mut colorization: u32 = 0;
            let mut opaque_blend: i32 = 0;
            let hr = (dwm.DwmGetColorizationColor)(&mut colorization, &mut opaque_blend);
            if hr >= 0 {
                // DwmGetColorizationColor returns 0xAARRGGBB
                let a = ((colorization >> 24) & 0xFF) as u8;
                let r = ((colorization >> 16) & 0xFF) as u8;
                let g = ((colorization >> 8)  & 0xFF) as u8;
                let b = ( colorization        & 0xFF) as u8;
                style.colors.accent = OptionColorU::Some(ColorU::new(r, g, b, a));
            }
        }

        // ── Dark mode detection (registry-based, same as old `io` path)
        // We keep this simple: check HKCU\...\Personalize\AppsUseLightTheme
        // via the already-loaded SystemParametersInfoW path is not possible,
        // so we rely on the GetSysColor heuristic: if window background
        // luminance < 128, assume dark.
        if let Some(ref bg) = style.colors.window_background.as_option() {
            let luma = (bg.r as u16 + bg.g as u16 + bg.b as u16) / 3;
            if luma < 128 {
                style.theme = azul_css::system::Theme::Dark;
            }
        }

        // ── Animation metrics ────────────────────────────────────────
        {
            let mut anim: i32 = 1;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETCLIENTAREAANIMATION, 0,
                &mut anim as *mut i32 as *mut c_void, 0,
            );
            let mut kb_cues: i32 = 0;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETKEYBOARDCUES, 0,
                &mut kb_cues as *mut i32 as *mut c_void, 0,
            );
            style.animation = azul_css::system::AnimationMetrics {
                animations_enabled: anim != 0,
                animation_duration_factor: 1.0,
                focus_indicator_behavior: if kb_cues != 0 {
                    azul_css::system::FocusBehavior::AlwaysVisible
                } else {
                    azul_css::system::FocusBehavior::KeyboardOnly
                },
            };
            if anim == 0 {
                style.prefers_reduced_motion =
                    azul_css::dynamic_selector::BoolCondition::True;
                style.accessibility.prefers_reduced_motion = true;
            }
        }

        // ── Audio metrics ────────────────────────────────────────────
        {
            let mut beep: i32 = 1;
            (u32_lib.SystemParametersInfoW)(
                SPI_GETBEEP, 0,
                &mut beep as *mut i32 as *mut c_void, 0,
            );
            style.audio = azul_css::system::AudioMetrics {
                event_sounds_enabled: beep != 0,
                input_feedback_sounds_enabled: false,
            };
        }
    }

    style.platform = Platform::Windows;

    // ── CLI fallback discovery ───────────────────────────────────────
    discover_windows_cli_extras(&mut style);
    style.os_version = detect_windows_version();
    style.language = detect_language_windows();

    let rm = detect_windows_reduced_motion();
    if rm == BoolCondition::True {
        style.prefers_reduced_motion = BoolCondition::True;
        style.accessibility.prefers_reduced_motion = true;
    }
    let hc = detect_windows_high_contrast();
    if hc == BoolCondition::True {
        style.prefers_high_contrast = BoolCondition::True;
        style.accessibility.prefers_high_contrast = true;
    }

    if let Some(sheet) = load_app_specific_stylesheet() {
        style.app_specific_stylesheet = Some(Box::new(sheet));
    }

    discover_windows_riced_style(&mut style);

    style
}

// ── CLI fallback helpers ────────────────────────────────────────────────

/// Spawn a subprocess and capture its stdout, returning `Err(())` on
/// timeout or any other failure.
fn run_command_with_timeout(
    program: &str,
    args: &[&str],
    timeout_ms: u64,
) -> Result<String, ()> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .map_err(|_| ())?;

    let deadline = Instant::now() + Duration::from_millis(timeout_ms);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                if !status.success() {
                    return Err(());
                }
                let output = child.wait_with_output().map_err(|_| ())?;
                return String::from_utf8(output.stdout).map_err(|_| ());
            }
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return Err(());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(_) => return Err(()),
        }
    }
}

/// Enhance an existing `SystemStyle` with registry-based dark-mode and
/// accent-colour information obtained via the `reg` CLI.
fn discover_windows_cli_extras(style: &mut azul_css::system::SystemStyle) {
    // ── Dark mode detection via registry ────────────────────────────
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            "/v",
            "AppsUseLightTheme",
        ],
        2000,
    ) {
        if output.contains("0x0") {
            style.theme = Theme::Dark;
        } else if output.contains("0x1") {
            style.theme = Theme::Light;
        }
    }

    // ── Accent colour from DWM registry key ─────────────────────────
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Software\Microsoft\Windows\DWM",
            "/v",
            "AccentColor",
        ],
        2000,
    ) {
        // Output line looks like: "    AccentColor    REG_DWORD    0xffb16300"
        // The value is in ABGR format.
        if let Some(hex_start) = output.find("0x") {
            let hex_str = &output[hex_start + 2..];
            let hex_str = hex_str.trim();
            // Take only the hex digits (up to 8 characters)
            let hex_digits: String = hex_str.chars().take(8).filter(|c| c.is_ascii_hexdigit()).collect();
            if let Ok(val) = u32::from_str_radix(&hex_digits, 16) {
                let a = ((val >> 24) & 0xFF) as u8;
                let b = ((val >> 16) & 0xFF) as u8;
                let g = ((val >> 8) & 0xFF) as u8;
                let r = (val & 0xFF) as u8;
                let accent = ColorU::new(r, g, b, a);
                style.colors.accent = OptionColorU::Some(accent);
                style.colors.selection_background = OptionColorU::Some(accent);
            }
        }
    }
}

/// Detect the Windows version by reading `CurrentBuildNumber` from the
/// registry.  Returns a best-effort `OsVersion`; defaults to `WIN_10` when
/// the build number cannot be determined.
fn detect_windows_version() -> OsVersion {
    let build_str = match run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion",
            "/v",
            "CurrentBuildNumber",
        ],
        2000,
    ) {
        Ok(s) => s,
        Err(_) => return OsVersion::WIN_10,
    };

    // Parse the numeric build number from the reg output
    let build: u32 = build_str
        .split_whitespace()
        .rev()
        .find_map(|tok| tok.parse::<u32>().ok())
        .unwrap_or(0);

    match build {
        22631..=u32::MAX => OsVersion::WIN_11_24H2,
        22621..=22630    => OsVersion::WIN_11_23H2,
        22500..=22620    => OsVersion::WIN_11_22H2,
        22000..=22499    => OsVersion::WIN_11_21H2,
        19045            => OsVersion::WIN_10_22H2,
        19044            => OsVersion::WIN_10_21H2,
        19043            => OsVersion::WIN_10_21H1,
        19042            => OsVersion::WIN_10_20H2,
        19041            => OsVersion::WIN_10_2004,
        18363            => OsVersion::WIN_10_1909,
        18362            => OsVersion::WIN_10_1903,
        17763            => OsVersion::WIN_10_1809,
        17134            => OsVersion::WIN_10_1803,
        16299            => OsVersion::WIN_10_1709,
        15063            => OsVersion::WIN_10_1703,
        14393            => OsVersion::WIN_10_1607,
        10586            => OsVersion::WIN_10_1511,
        10240            => OsVersion::WIN_10_1507,
        9600             => OsVersion::WIN_8_1,
        9200             => OsVersion::WIN_8,
        7601             => OsVersion::WIN_7,
        6002             => OsVersion::WIN_VISTA,
        2600             => OsVersion::WIN_XP,
        _                => OsVersion::WIN_10,
    }
}

/// Check the `MinAnimate` registry key to detect reduced-motion preference.
fn detect_windows_reduced_motion() -> BoolCondition {
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Desktop\WindowMetrics",
            "/v",
            "MinAnimate",
        ],
        2000,
    ) {
        // MinAnimate = "0" means animations are disabled
        if output.contains("\"0\"") || output.contains("    0") {
            return BoolCondition::True;
        }
    }
    BoolCondition::False
}

/// Check the `HighContrast` Flags registry value (bit 0 = HCF_HIGHCONTRASTON).
fn detect_windows_high_contrast() -> BoolCondition {
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\Accessibility\HighContrast",
            "/v",
            "Flags",
        ],
        2000,
    ) {
        // Flags is a REG_SZ decimal string.  Bit 0 set = high contrast on.
        if let Some(val) = output.split_whitespace().rev().find_map(|tok| tok.parse::<u32>().ok()) {
            if val & 1 != 0 {
                return BoolCondition::True;
            }
        }
    }
    BoolCondition::False
}

/// Detect the Windows display language.  Tries PowerShell `(Get-Culture).Name`
/// first, then falls back to the registry `LocaleName` value.
fn detect_language_windows() -> AzString {
    // Fast path: PowerShell
    if let Ok(output) = run_command_with_timeout(
        "powershell",
        &["-NoProfile", "-Command", "(Get-Culture).Name"],
        3000,
    ) {
        let lang = output.trim();
        if !lang.is_empty() {
            return AzString::from(lang.to_string());
        }
    }

    // Fallback: registry
    if let Ok(output) = run_command_with_timeout(
        "reg",
        &[
            "query",
            r"HKCU\Control Panel\International",
            "/v",
            "LocaleName",
        ],
        2000,
    ) {
        // Last whitespace-delimited token is the locale string
        if let Some(locale) = output.split_whitespace().last() {
            if !locale.is_empty() {
                return AzString::from(locale.to_string());
            }
        }
    }

    AzString::from_const_str("en-US")
}

/// Load an application-specific stylesheet from
/// `%APPDATA%\azul\styles\<exe_name>.css`.
fn load_app_specific_stylesheet() -> Option<Css> {
    use std::env;
    use std::path::PathBuf;

    if env::var("AZ_DISABLE_RICING").is_ok() {
        return None;
    }

    let appdata = env::var("APPDATA").ok()?;
    let exe_path = env::current_exe().ok()?;
    let exe_stem = exe_path.file_stem()?.to_str()?;

    let mut css_path = PathBuf::from(&appdata);
    css_path.push("azul");
    css_path.push("styles");
    css_path.push(format!("{}.css", exe_stem));

    let css_text = std::fs::read_to_string(&css_path).ok()?;
    let (css, _warnings) = new_from_str(&css_text);
    if css.is_empty() { None } else { Some(css) }
}

/// Check for "riced" style overrides from popular Windows customisation tools.
///
/// Currently inspects:
/// - Windows Terminal `settings.json` for color scheme colours
/// - pywal `colors.json` (available on Windows via WSL or native install)
fn discover_windows_riced_style(style: &mut azul_css::system::SystemStyle) {
    use std::env;
    use std::path::PathBuf;

    // ── Windows Terminal colour scheme ──────────────────────────────
    if let Ok(localappdata) = env::var("LOCALAPPDATA") {
        let mut wt_path = PathBuf::from(&localappdata);
        wt_path.push("Packages");
        // Microsoft.WindowsTerminal_8wekyb3d8bbwe is the default store path
        wt_path.push("Microsoft.WindowsTerminal_8wekyb3d8bbwe");
        wt_path.push("LocalState");
        wt_path.push("settings.json");

        if let Ok(text) = std::fs::read_to_string(&wt_path) {
            parse_windows_terminal_colors(&text, style);
        }
    }

    // ── pywal colours (native Windows or WSL) ───────────────────────
    if let Ok(home) = env::var("USERPROFILE") {
        let mut pywal_path = PathBuf::from(&home);
        pywal_path.push(".cache");
        pywal_path.push("wal");
        pywal_path.push("colors.json");

        if let Ok(text) = std::fs::read_to_string(&pywal_path) {
            parse_pywal_colors(&text, style);
        }
    }
}

/// Minimal JSON-free parser for Windows Terminal settings.json.
/// Looks for `"background"` and `"foreground"` keys in scheme blocks.
fn parse_windows_terminal_colors(
    text: &str,
    style: &mut azul_css::system::SystemStyle,
) {
    // Very simple: find "background": "#rrggbb" and "foreground": "#rrggbb"
    if let Some(bg) = extract_json_color(text, "background") {
        style.colors.window_background = OptionColorU::Some(bg);
    }
    if let Some(fg) = extract_json_color(text, "foreground") {
        style.colors.text = OptionColorU::Some(fg);
    }
}

/// Minimal JSON-free parser for pywal colors.json.
/// Looks for `"background"` and `"foreground"` keys.
fn parse_pywal_colors(
    text: &str,
    style: &mut azul_css::system::SystemStyle,
) {
    if let Some(bg) = extract_json_color(text, "background") {
        style.colors.window_background = OptionColorU::Some(bg);
    }
    if let Some(fg) = extract_json_color(text, "foreground") {
        style.colors.text = OptionColorU::Some(fg);
    }
}

/// Extract a `"key": "#rrggbb"` colour value from a JSON-like string
/// without pulling in a JSON parser.
fn extract_json_color(text: &str, key: &str) -> Option<ColorU> {
    let needle = format!("\"{}\"", key);
    let idx = text.find(&needle)?;
    let after_key = &text[idx + needle.len()..];
    // Skip past the colon and whitespace to find the '#'
    let hash_idx = after_key.find('#')?;
    let hex_start = hash_idx + 1;
    if after_key.len() < hex_start + 6 {
        return None;
    }
    let hex = &after_key[hex_start..hex_start + 6];
    let val = u32::from_str_radix(hex, 16).ok()?;
    let r = ((val >> 16) & 0xFF) as u8;
    let g = ((val >> 8) & 0xFF) as u8;
    let b = (val & 0xFF) as u8;
    Some(ColorU::new_rgb(r, g, b))
}
