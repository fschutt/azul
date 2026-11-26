//! What a platform's style discovery may ask of a subprocess: a command-line answer to an accessibility flag is wanted
//! only where the native query did not run; and the line `App::create` logs for each of its steps. Pure.

use azul_css::dynamic_selector::BoolCondition;

use super::*;

/// The native query answered (a `False` is an answer as much as a `True`): the command line is not asked. It is asked
/// only where the native query did not run (the library did not load, the class was not there) and left the flag at its
/// default. This is what the two `defaults read` processes of `App::create` (12 ms of its first 61 on a Mac, on the
/// window's thread) were: a fallback that ran for every user who has not turned Reduce motion or Increase contrast on,
/// and got the answer the native query had given.
///
/// Mutation: a native `False` that is asked again; a native `True` that is asked again; a native query that did not run and
/// is not asked.
#[test]
fn the_command_line_is_asked_only_where_the_native_query_did_not_answer() {
    assert!(!cli_fallback_wanted(true, BoolCondition::False), "the native query said no: no process");
    assert!(!cli_fallback_wanted(true, BoolCondition::True), "the native query said yes: no process");
    assert!(cli_fallback_wanted(false, BoolCondition::False), "the native query did not run: the command line is the only answer");
    assert!(!cli_fallback_wanted(false, BoolCondition::True), "an answer already there is kept");
}

/// The line of a step of `App::create`: its name and its time in microseconds, so that a script can read the number of
/// each step.
///
/// Mutation: the time in the wrong unit; the name missing.
#[test]
fn a_step_of_app_create_is_logged_with_its_name_and_its_microseconds() {
    assert_eq!(step_line("settings", 1_234), "[app] App::create: settings 1234 us");
    assert_eq!(step_line("system_style", 0), "[app] App::create: system_style 0 us");
}

/// The steps measure the time since the one before: the first since the start, each next since the last.
///
/// Mutation: every step measured from the start; the clock not moved on after a step.
#[test]
fn each_step_is_measured_from_the_one_before() {
    let mut steps = CreateSteps::at(1_000);
    assert_eq!(steps.lap_at("one", 1_300), "[app] App::create: one 300 us");
    assert_eq!(steps.lap_at("two", 1_350), "[app] App::create: two 50 us");
    assert_eq!(steps.lap_at("three", 1_350), "[app] App::create: three 0 us", "a clock that did not move");
    assert_eq!(steps.lap_at("four", 1_000), "[app] App::create: four 0 us", "a clock that went back is no time");
    assert_eq!(steps.total_at(2_000), 1_000, "the whole is from the start");
}

/// The style discovery of Windows starts no process: it ran `reg.exe` seven times and PowerShell once, each a process
/// started on the window's thread before the first frame (the startup timeline of 2026-11-26 measured the step).
/// A source gate in the manner of the others of this tree: the file is read as text.
///
/// Mutation: the old `run_command_with_timeout("reg", ...)`; PowerShell for the language.
#[test]
fn the_windows_style_discovery_starts_no_process() {
    let source = include_str!("../windows/system_style.rs");
    for needle in [["Command", "::new"].concat(), "run_command_with_timeout".to_owned(), "powershell".to_owned()] {
        // The doc comments may name what is gone; the code may not use it.
        let used = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .any(|line| line.contains(&needle));
        assert!(!used, "dll/src/desktop/shell2/windows/system_style.rs uses `{needle}`: a process started before the first frame");
    }
}

fn utf16le(text: &str) -> Vec<u8> {
    text.encode_utf16().chain(core::iter::once(0)).flat_map(u16::to_le_bytes).collect()
}

/// A DWORD, a string and binary data come out as the lines `reg query` prints.
///
/// Mutation: a DWORD in decimal; the string kept with its NUL; binary in lowercase; the line ending dropped.
#[test]
fn a_registry_value_is_the_line_reg_query_prints() {
    assert_eq!(
        reg_value_text("AccentColorMenu", 4, &0xffd4_7800_u32.to_le_bytes()).as_deref(),
        Some("    AccentColorMenu    REG_DWORD    0xffd47800\r\n")
    );
    assert_eq!(reg_value_text("AppsUseLightTheme", 4, &0_u32.to_le_bytes()).as_deref(), Some("    AppsUseLightTheme    REG_DWORD    0x0\r\n"));
    assert_eq!(
        reg_value_text("CurrentBuildNumber", 1, &utf16le("22631")).as_deref(),
        Some("    CurrentBuildNumber    REG_SZ    22631\r\n")
    );
    assert_eq!(reg_value_text("Path", 2, &utf16le("%SystemRoot%")).as_deref(), Some("    Path    REG_EXPAND_SZ    %SystemRoot%\r\n"));
    assert_eq!(reg_value_text("AccentPalette", 3, &[0x99, 0xeb, 0xff, 0x00]).as_deref(), Some("    AccentPalette    REG_BINARY    99EBFF00\r\n"));
}

/// What the registry holds that the discovery does not read, and data that does not fit its type, are no line.
///
/// Mutation: a short DWORD read as 0; a type 11 (QWORD) formatted; an odd string length accepted.
#[test]
fn a_registry_value_that_does_not_fit_is_no_line() {
    assert_eq!(reg_value_text("x", 4, &[1, 2]), None, "a DWORD has four bytes");
    assert_eq!(reg_value_text("x", 11, &[0; 8]), None, "a QWORD is not read");
    assert_eq!(reg_value_text("x", 0, &[]), None, "REG_NONE is not read");
    assert_eq!(reg_value_text("x", 1, &[b'a', 0, b'b']), None, "UTF-16 has an even length");
}

/// The text the native reader makes is read by the parsers that read `reg query`: the accent from the Explorer
/// key (the DWORD of the menu accent, and the palette when only that is there), the dark mode, the build number.
/// This is the contract of the Windows discovery's move from `reg.exe` to the registry API.
///
/// Mutation: the accent's bytes in the wrong order; the palette hex in lowercase with a separator.
#[test]
fn the_parsers_of_reg_query_read_what_the_native_reader_writes() {
    use azul_css::{props::basic::color::ColorU, system::windows_accent};

    let menu = reg_value_text("AccentColorMenu", 4, &0xffd4_7800_u32.to_le_bytes()).unwrap();
    assert_eq!(windows_accent::accent_from_reg_query(&menu), Some(ColorU::new_rgb(0x00, 0x78, 0xd4)));
    let dwm = reg_value_text("AccentColor", 4, &0xffd4_7800_u32.to_le_bytes()).unwrap();
    assert_eq!(windows_accent::accent_from_reg_query(&dwm), Some(ColorU::new_rgb(0x00, 0x78, 0xd4)));

    // The palette of Windows 11's blue: seven RGBA entries and the unused eighth; the base accent is the fourth.
    let palette: Vec<u8> = [
        [0x99, 0xeb, 0xff, 0x00],
        [0x60, 0xcd, 0xff, 0x00],
        [0x00, 0x93, 0xf9, 0x00],
        [0x00, 0x78, 0xd4, 0x00],
        [0x00, 0x5f, 0xb8, 0x00],
        [0x00, 0x42, 0x92, 0x00],
        [0x00, 0x23, 0x68, 0x00],
        [0x4c, 0xc2, 0xff, 0x00],
    ]
    .concat();
    let only_palette = reg_value_text("AccentPalette", 3, &palette).unwrap();
    assert_eq!(windows_accent::accent_from_reg_query(&only_palette), Some(ColorU::new_rgb(0x00, 0x78, 0xd4)));

    let light = reg_value_text("AppsUseLightTheme", 4, &1_u32.to_le_bytes()).unwrap();
    assert!(light.contains("0x1") && !light.contains("0x0"), "{light}");
    let build = reg_value_text("CurrentBuildNumber", 1, &utf16le("26100")).unwrap();
    assert_eq!(build.split_whitespace().rev().find_map(|t| t.parse::<u32>().ok()), Some(26100));
}
