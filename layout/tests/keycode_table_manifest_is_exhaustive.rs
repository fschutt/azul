//! The four platform keycode tables must not silently lose coverage.
//!
//! Seam-audit R3. Every backend turns a platform key identifier into an
//! `azul_core::window::VirtualKeyCode` through a hand-written match table:
//!
//! | table | source | key identifier |
//! |---|---|---|
//! | macOS | `common::event::macos_keycode_to_virtual_key` | `NSEvent::keyCode` |
//! | X11 | `linux::x11::events::keysym_to_virtual_keycode` | xkb keysym |
//! | Wayland | delegates to the X11 table | xkb keysym |
//! | Win32 | `common::event::win32_vkey_to_virtual_key` | `VK_*` |
//!
//! `None` from one of these is not "unlabelled": every shortcut, every
//! `On::VirtualKeyDown` filter and every keyboard-driven widget is keyed on the
//! `VirtualKeyCode`, so a missing arm is a feature that silently does not exist
//! on that platform. This class has shipped holes repeatedly — the Wayland
//! table's catch-all was `_ => VirtualKeyCode::Escape` (every unknown key
//! dismissed menus), and the X11 table had no punctuation, no keypad and no
//! AltGr at all — and nothing compared the tables, so deleting an arm nobody
//! happened to hand-list in a spot check stayed green.
//!
//! [`MANIFEST`] is that comparison: one row per `VirtualKeyCode`, recording how
//! many source codes each table maps onto it. [`EXEMPT`] holds the codes NO
//! table maps, each with a reason. Between them they cover the enum exactly
//! once, which is checked here too — a variant that is in neither list fails.
//!
//! This file deliberately links nothing: it is `include_str!` plus string
//! matching over the four sources, so it keeps working when the Win32 and macOS
//! tables cannot even be compiled on the host running it.
//!
//! TO TURN IT RED: delete any arm from any of the tables, or add one without
//! updating the row.
//!
//! # Known holes (the zeroes in the manifest, stated rather than hidden)
//!
//! These are real per-platform gaps, not artifacts of the manifest. They are
//! recorded as `0` so they are visible in a diff the moment someone closes one:
//!
//! - **F13..F24 are macOS-and-X11 dead.** Only Win32 maps them. macOS has the
//!   keycodes (`0x69` F13, `0x6B` F14, `0x71` F15, `0x6A` F16, `0x40` F17 ...)
//!   and X11 has `XK_F13..XK_F24` (`0xFFCA..0xFFD5`); neither table lists them.
//! - **Win32 has no `NumpadEnter` and no `NumpadEquals`.** Win32 delivers the
//!   keypad Enter as `VK_RETURN` with the extended-key bit set in `lParam`, so
//!   it arrives as plain `Return` and nothing can tell the two apart.
//! - **`NumpadComma` exists only on X11.** Win32's `VK_SEPARATOR` (`0x6C`) is
//!   unmapped, and so is the JIS keypad comma on macOS (`0x5F`).
//! - **The media / browser / ACPI block is Win32-only** (`Mute`, `VolumeUp`,
//!   `VolumeDown`, `NextTrack`, `PrevTrack`, `PlayPause`, `MediaStop`,
//!   `MediaSelect`, `Mail`, `Sleep`, `Web*`, `Navigate*`). X11 exposes these as
//!   `XF86*` keysyms, which `linux/x11/defines.rs` does not declare; macOS
//!   delivers them as `NSSystemDefined` events rather than `keyDown`.
//! - **The Japanese IME keys are Win32-only** (`Convert`, `NoConvert`, `Kana`,
//!   `Kanji`). X11 has `XK_Henkan_Mode`, `XK_Muhenkan`, `XK_Kana_Shift` and
//!   `XK_Kanji`.
//! - **`OEM102` — the 102nd key on an ISO keyboard — is Win32-only, and X11
//!   actively MISREPORTS it:** the key produces `XK_less` / `XK_greater`, which
//!   the punctuation block folds onto `Comma` and `Period`. On X11 and Wayland
//!   that key is therefore indistinguishable from `,` and `.`.
//! - **macOS has no `Snapshot` / `Scroll` / `Pause` / `Insert`** — Apple
//!   keyboards carry none of those keys; the Insert position is `Help`
//!   (keycode `0x72`).

/// `dll/src/desktop/shell2/common/event.rs`, verbatim, at compile time. Holds
/// BOTH the macOS and the Win32 table.
const COMMON_EVENT_SRC: &str = include_str!("../../dll/src/desktop/shell2/common/event.rs");

/// `dll/src/desktop/shell2/linux/x11/events.rs` — the X11 table, which Wayland
/// also runs on.
const X11_EVENTS_SRC: &str = include_str!("../../dll/src/desktop/shell2/linux/x11/events.rs");

/// `dll/src/desktop/shell2/linux/wayland/events.rs` — checked for the
/// DELEGATION, which is what makes "Wayland == X11" true in the manifest.
const WAYLAND_EVENTS_SRC: &str =
    include_str!("../../dll/src/desktop/shell2/linux/wayland/events.rs");

/// `core/src/window.rs` — the `VirtualKeyCode` enum the manifest is indexed by.
const CORE_WINDOW_SRC: &str = include_str!("../../core/src/window.rs");

/// The signature that opens each table. If one of these stops matching, the
/// tests below fail loudly rather than silently scanning an empty string.
const MACOS_FN: &str = "pub fn macos_keycode_to_virtual_key";
const X11_FN: &str = "pub fn keysym_to_virtual_keycode";
const WIN32_FN: &str = "pub fn win32_vkey_to_virtual_key";
/// The Win32 table's second half: the seven layout-dependent `VK_OEM_*` codes
/// resolve through the ACTIVE layout's character, so their `VirtualKeyCode`s
/// live in this function and not in the `VK_*` match.
const WIN32_OEM_FN: &str = "pub fn win32_oem_char_to_virtual_key";

/// How many source codes each table maps onto a given `VirtualKeyCode`.
///
/// Columns: `(variant, macOS, X11 + Wayland, Win32)`. A count, not a flag,
/// because several codes legitimately share one variant — X11 folds a key's
/// shifted keysym onto its unshifted code so a press and its release resolve
/// the same way (`XK_1 | XK_exclam => Key1`), Win32 maps both the generic and
/// the left-hand modifier (`VK_SHIFT` and `VK_LSHIFT` => `LShift`), and X11
/// lands four keysyms on `RAlt` (`Alt_R`, `Meta_R`, `ISO_Level3_Shift`,
/// `Mode_switch`). A flag would stay green when one of a pair is deleted, and
/// deleting exactly one of those pairs is how `ctrl_down()` was never true on
/// Windows and how X11 keys got stuck in `pressed_virtual_keycodes`.
///
/// `0` = that platform does not map the key at all. Those are listed and
/// justified under "Known holes" in this file's header.
#[rustfmt::skip]
const MANIFEST: &[(&str, u8, u8, u8)] = &[
    // variant           mac  x11  win
    ("Key1",               1,   2,   1),
    ("Key2",               1,   2,   1),
    ("Key3",               1,   2,   1),
    ("Key4",               1,   2,   1),
    ("Key5",               1,   2,   1),
    ("Key6",               1,   2,   1),
    ("Key7",               1,   2,   1),
    ("Key8",               1,   2,   1),
    ("Key9",               1,   2,   1),
    ("Key0",               1,   2,   1),
    ("A",                  1,   2,   1),
    ("B",                  1,   2,   1),
    ("C",                  1,   2,   1),
    ("D",                  1,   2,   1),
    ("E",                  1,   2,   1),
    ("F",                  1,   2,   1),
    ("G",                  1,   2,   1),
    ("H",                  1,   2,   1),
    ("I",                  1,   2,   1),
    ("J",                  1,   2,   1),
    ("K",                  1,   2,   1),
    ("L",                  1,   2,   1),
    ("M",                  1,   2,   1),
    ("N",                  1,   2,   1),
    ("O",                  1,   2,   1),
    ("P",                  1,   2,   1),
    ("Q",                  1,   2,   1),
    ("R",                  1,   2,   1),
    ("S",                  1,   2,   1),
    ("T",                  1,   2,   1),
    ("U",                  1,   2,   1),
    ("V",                  1,   2,   1),
    ("W",                  1,   2,   1),
    ("X",                  1,   2,   1),
    ("Y",                  1,   2,   1),
    ("Z",                  1,   2,   1),
    ("Escape",             1,   1,   1),
    ("F1",                 1,   1,   1),
    ("F2",                 1,   1,   1),
    ("F3",                 1,   1,   1),
    ("F4",                 1,   1,   1),
    ("F5",                 1,   1,   1),
    ("F6",                 1,   1,   1),
    ("F7",                 1,   1,   1),
    ("F8",                 1,   1,   1),
    ("F9",                 1,   1,   1),
    ("F10",                1,   1,   1),
    ("F11",                1,   1,   1),
    ("F12",                1,   1,   1),
    ("F13",                0,   0,   1),
    ("F14",                0,   0,   1),
    ("F15",                0,   0,   1),
    ("F16",                0,   0,   1),
    ("F17",                0,   0,   1),
    ("F18",                0,   0,   1),
    ("F19",                0,   0,   1),
    ("F20",                0,   0,   1),
    ("F21",                0,   0,   1),
    ("F22",                0,   0,   1),
    ("F23",                0,   0,   1),
    ("F24",                0,   0,   1),
    ("Snapshot",           0,   1,   1),
    ("Scroll",             0,   1,   1),
    ("Pause",              0,   1,   1),
    ("Insert",             0,   1,   1),
    ("Home",               1,   1,   1),
    ("Delete",             1,   1,   1),
    ("End",                1,   1,   1),
    ("PageDown",           1,   1,   1),
    ("PageUp",             1,   1,   1),
    ("Left",               1,   1,   1),
    ("Up",                 1,   1,   1),
    ("Right",              1,   1,   1),
    ("Down",               1,   1,   1),
    ("Back",               1,   1,   1),
    ("Return",             1,   1,   1),
    ("Space",              1,   2,   1),
    ("Numlock",            1,   1,   1),
    ("Numpad0",            1,   2,   1),
    ("Numpad1",            1,   2,   1),
    ("Numpad2",            1,   2,   1),
    ("Numpad3",            1,   2,   1),
    ("Numpad4",            1,   2,   1),
    ("Numpad5",            1,   2,   1),
    ("Numpad6",            1,   2,   1),
    ("Numpad7",            1,   2,   1),
    ("Numpad8",            1,   2,   1),
    ("Numpad9",            1,   2,   1),
    ("NumpadAdd",          1,   1,   1),
    ("NumpadDivide",       1,   1,   1),
    ("NumpadDecimal",      1,   2,   1),
    ("NumpadComma",        0,   1,   0),
    ("NumpadEnter",        1,   1,   0),
    ("NumpadEquals",       1,   1,   0),
    ("NumpadMultiply",     1,   1,   1),
    ("NumpadSubtract",     1,   1,   1),
    ("Apostrophe",         1,   2,   1),
    ("Apps",               1,   1,   1),
    ("Backslash",          1,   2,   1),
    ("Capital",            1,   2,   1),
    ("Comma",              1,   2,   1),
    ("Convert",            0,   0,   1),
    ("Equals",             1,   2,   1),
    ("Grave",              1,   2,   1),
    ("Kana",               0,   0,   1),
    ("Kanji",              0,   0,   1),
    ("LAlt",               1,   2,   2),
    ("LBracket",           1,   2,   1),
    ("LControl",           1,   1,   2),
    ("LShift",             1,   1,   2),
    ("LWin",               1,   2,   1),
    ("Mail",               0,   1,   1),
    ("MediaSelect",        0,   1,   1),
    ("MediaStop",          0,   1,   1),
    ("Minus",              1,   2,   1),
    ("Mute",               0,   1,   1),
    ("NavigateForward",    0,   0,   1),
    ("NavigateBackward",   0,   0,   1),
    ("NextTrack",          0,   1,   1),
    ("NoConvert",          0,   0,   1),
    ("OEM102",             0,   0,   1),
    ("Period",             1,   2,   1),
    ("PlayPause",          0,   2,   1),
    ("PrevTrack",          0,   1,   1),
    ("RAlt",               1,   4,   1),
    ("RBracket",           1,   2,   1),
    ("RControl",           1,   1,   1),
    ("RShift",             1,   1,   1),
    ("RWin",               1,   2,   1),
    ("Semicolon",          1,   2,   1),
    ("Slash",              1,   2,   1),
    ("Sleep",              0,   1,   1),
    ("Sysrq",              0,   1,   0),
    ("Tab",                1,   2,   1),
    ("VolumeDown",         0,   1,   1),
    ("VolumeUp",           0,   1,   1),
    ("WebFavorites",       0,   1,   1),
    ("WebHome",            0,   1,   1),
    ("WebRefresh",         0,   1,   1),
    ("WebSearch",          0,   1,   1),
    ("WebStop",            0,   1,   1),
    // ADDED by 13e. 9h-i mapped these on X11 and the manifest was never
    // updated - only the e2e target checks it, and that target had not
    // compiled since 8f, so the drift sat unseen for the whole arc.
    // Win32 is 0: `VK_BROWSER_BACK` maps to `NavigateBackward`, which is
    // what the old exemption text said and what this row must record.
    ("WebBack",            0,   1,   0),
    ("WebForward",         0,   1,   0),
    ("MyComputer",         0,   2,   0),
    ("Wake",               0,   1,   0),
    ("Power",              0,   1,   0),
];

/// `VirtualKeyCode`s no table maps, with the reason each one is unreachable.
///
/// An entry here is a claim that NO platform can currently produce the code.
/// `the_exemption_list_has_no_stale_entries` re-checks the claim, so closing a
/// gap forces the entry out of this list and into [`MANIFEST`].
const EXEMPT: &[(&str, &str)] = &[
    (
        "Compose",
        "dead-key/compose sequences are resolved by xkb_compose and by the macOS input \
         manager before a key event exists; no backend sees a 'Compose' key",
    ),
    (
        "Caret",
        "winit legacy name for '^'; a shifted digit or a dead key on every layout, never a \
         physical key of its own",
    ),
    (
        "AbntC1",
        "Brazilian ABNT2 extra key; Win32 has the undocumented VK_ABNT_C1 (0xC1), X11 \
         reports it as XK_slash, macOS has no keycode",
    ),
    (
        "AbntC2",
        "Brazilian ABNT2 keypad '.' ; Win32 VK_ABNT_C2 (0xC2), X11 reports XK_KP_Decimal",
    ),
    (
        "Asterisk",
        "shifted form of Key8 on every layout; X11 deliberately folds XK_asterisk onto Key8 \
         so a press and its release resolve to the same code",
    ),
    (
        "At",
        "shifted form of Key2 (US); folded onto Key2 by the X11 digit block for press/release \
         symmetry",
    ),
    (
        "Colon",
        "shifted form of Semicolon; folded onto Semicolon by the X11 punctuation block",
    ),
    (
        "Plus",
        "shifted form of Equals; folded onto Equals by the X11 punctuation block. The keypad \
         '+' is NumpadAdd, which is mapped",
    ),
    (
        "Underline",
        "shifted form of Minus; folded onto Minus by the X11 punctuation block",
    ),
    (
        "Ax",
        "winit's name for the Japanese AX keyboard's AX key; no keysym and no VK code exists",
    ),
    (
        "Yen",
        "JIS yen key; macOS has keycode 0x5D and X11 has XK_yen, but neither table lists it \
         and Win32 has no VK code at all",
    ),
    (
        "Calculator",
        "ACPI/vendor hotkey: Win32 delivers it as VK_LAUNCH_APP2 (deliberately unmapped, see \
         the commented block in the Win32 table), X11 as XF86Calculator, macOS not at all",
    ),
    (
        "Stop",
        "the browser Stop key is WebStop (mapped on Win32); this variant is winit's duplicate \
         and nothing produces it",
    ),
    (
        "Unlabeled",
        "winit's placeholder for a key with no label; by definition no code maps to it",
    ),
    (
        "Copy",
        "the dedicated Copy key exists only as XF86Copy / a Sun keyboard key; the engine's \
         clipboard path is the Ctrl/Cmd+C shortcut, keyed on VirtualKeyCode::C",
    ),
    (
        "Paste",
        "the dedicated Paste key exists only as XF86Paste; the engine's clipboard path is \
         Ctrl/Cmd+V",
    ),
    (
        "Cut",
        "the dedicated Cut key exists only as XF86Cut; the engine's clipboard path is \
         Ctrl/Cmd+X",
    ),
];

// ---------------------------------------------------------------------------
// Source parsing
// ---------------------------------------------------------------------------

/// Rust source with `//` and `/* */` comments blanked out.
///
/// Both tables carry commented-out arms naming types that do not exist
/// (`VirtualKeyCode::Lbutton`, `VirtualKeyCode::Launch_app1`), so a parser that
/// reads comments would "find" coverage that no build contains.
fn strip_comments(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == '*' && bytes[i + 1] == '/') {
                if bytes[i] == '\n' {
                    out.push('\n');
                }
                i += 1;
            }
            i = (i + 2).min(bytes.len());
        } else if bytes[i] == '/' && i + 1 < bytes.len() && bytes[i + 1] == '/' {
            while i < bytes.len() && bytes[i] != '\n' {
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    out
}

/// The body of the function whose signature starts with `sig`, braces balanced.
fn fn_body(src: &str, sig: &str, file: &str) -> String {
    let start = src.find(sig).unwrap_or_else(|| {
        panic!(
            "could not find `{sig}` in {file} — it was renamed or moved. Update the constant in \
             this test rather than deleting the test: a table nothing compares is exactly the \
             hole this file exists to close."
        )
    });
    let rest = &src[start..];
    let open = rest
        .find('{')
        .expect("a function signature is followed by a body");
    let mut depth = 0usize;
    for (offset, ch) in rest[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return strip_comments(&rest[open..=open + offset]);
                }
            }
            _ => {}
        }
    }
    panic!("unbalanced braces while extracting `{sig}` from {file}");
}

/// Every `VirtualKeyCode` a table body produces, and how many source codes
/// resolve to it.
///
/// Every arm in all four tables is one line of the form
/// `<pattern> => Some(VirtualKeyCode::<Variant>),`, where the pattern is either
/// a single code or `|`-separated alternates. The alternates are the count: an
/// arm mapping two keysyms onto one variant contributes 2.
fn mapped_codes(body: &str) -> Vec<(String, u32)> {
    let mut counts: Vec<(String, u32)> = Vec::new();
    for line in body.lines() {
        let Some((pattern, result)) = line.split_once("=>") else {
            continue;
        };
        let Some(rest) = result.split("VirtualKeyCode::").nth(1) else {
            continue;
        };
        let variant: String = rest
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if variant.is_empty() {
            continue;
        }
        let alternates = u32::try_from(pattern.matches('|').count()).unwrap() + 1;
        match counts.iter_mut().find(|(name, _)| *name == variant) {
            Some((_, n)) => *n += alternates,
            None => counts.push((variant, alternates)),
        }
    }
    counts
}

/// `(label, source codes it maps)` for each platform table, in manifest column
/// order.
fn platform_tables() -> Vec<(&'static str, Vec<(String, u32)>)> {
    let mut win32 = mapped_codes(&fn_body(COMMON_EVENT_SRC, WIN32_FN, "common/event.rs"));
    for (variant, count) in
        mapped_codes(&fn_body(COMMON_EVENT_SRC, WIN32_OEM_FN, "common/event.rs"))
    {
        match win32.iter_mut().find(|(name, _)| *name == variant) {
            Some((_, n)) => *n += count,
            None => win32.push((variant, count)),
        }
    }
    vec![
        (
            "macOS (common/event.rs::macos_keycode_to_virtual_key)",
            mapped_codes(&fn_body(COMMON_EVENT_SRC, MACOS_FN, "common/event.rs")),
        ),
        (
            "X11 + Wayland (linux/x11/events.rs::keysym_to_virtual_keycode)",
            mapped_codes(&fn_body(X11_EVENTS_SRC, X11_FN, "linux/x11/events.rs")),
        ),
        ("Win32 (common/event.rs::win32_vkey_to_virtual_key)", win32),
    ]
}

/// Every variant of `azul_core::window::VirtualKeyCode`, in declaration order.
fn declared_virtual_key_codes() -> Vec<&'static str> {
    let start = CORE_WINDOW_SRC
        .find("pub enum VirtualKeyCode {")
        .expect("core/src/window.rs must declare `pub enum VirtualKeyCode`");
    let rest = &CORE_WINDOW_SRC[start..];
    let end = rest.find("\n}").expect("the enum must be closed");
    rest[..end]
        .lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            let name = line.strip_suffix(',')?;
            name.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
                .then_some(name)
        })
        .collect()
}

fn manifest_count(row: &(&str, u8, u8, u8), column: usize) -> u32 {
    u32::from(match column {
        0 => row.1,
        1 => row.2,
        _ => row.3,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// A zero is not a measurement: every test below would pass vacuously against
/// an empty parse, which is the exact failure mode they exist to prevent.
#[test]
fn the_parser_actually_found_the_tables() {
    let variants = declared_virtual_key_codes();
    assert!(
        variants.len() >= 150,
        "parsed only {} VirtualKeyCode variants out of core/src/window.rs — the enum parser \
         broke, and a short list would make the coverage comparison pass while proving nothing",
        variants.len()
    );

    for (label, table) in platform_tables() {
        let arms: u32 = table.iter().map(|(_, n)| n).sum();
        assert!(
            table.len() >= 90 && arms >= 90,
            "{label}: parsed {} variants across {arms} source codes — far too few for a keycode \
             table. The arm parser broke; fix it rather than lowering this floor",
            table.len()
        );
        for (variant, _) in &table {
            assert!(
                variants.contains(&variant.as_str()),
                "{label} maps `VirtualKeyCode::{variant}`, which is not a variant of the enum in \
                 core/src/window.rs — the parser is reading something that is not a match arm"
            );
        }
    }
}

#[test]
fn every_virtual_key_code_is_in_the_manifest_or_exempt() {
    let variants = declared_virtual_key_codes();

    let unlisted: Vec<&str> = variants
        .iter()
        .copied()
        .filter(|v| {
            !MANIFEST.iter().any(|row| row.0 == *v) && !EXEMPT.iter().any(|(name, _)| name == v)
        })
        .collect();
    assert!(
        unlisted.is_empty(),
        "VirtualKeyCode variant(s) in neither MANIFEST nor EXEMPT: {unlisted:?}\n\nA new variant \
         that nothing lists is invisible to the coverage comparison — add a MANIFEST row saying \
         which platforms map it, or an EXEMPT entry saying why none can."
    );

    let stale: Vec<&str> = MANIFEST
        .iter()
        .map(|row| row.0)
        .chain(EXEMPT.iter().map(|(name, _)| *name))
        .filter(|name| !variants.contains(name))
        .collect();
    assert!(
        stale.is_empty(),
        "manifest/exemption entries naming variants that no longer exist: {stale:?}"
    );

    let both: Vec<&str> = MANIFEST
        .iter()
        .map(|row| row.0)
        .filter(|name| EXEMPT.iter().any(|(exempt, _)| exempt == name))
        .collect();
    assert!(
        both.is_empty(),
        "listed in BOTH MANIFEST and EXEMPT: {both:?}"
    );
}

#[test]
fn every_platform_table_matches_the_manifest() {
    let tables = platform_tables();
    let mut drift: Vec<String> = Vec::new();

    for (column, (label, table)) in tables.iter().enumerate() {
        for row in MANIFEST {
            let expected = manifest_count(row, column);
            let actual = table
                .iter()
                .find(|(name, _)| name == row.0)
                .map_or(0, |(_, n)| *n);
            if expected != actual {
                drift.push(format!(
                    "  {label}: VirtualKeyCode::{} — manifest says {expected} source code(s), the \
                     table has {actual}",
                    row.0
                ));
            }
        }
        for (variant, actual) in table {
            if !MANIFEST.iter().any(|row| row.0 == variant.as_str()) {
                drift.push(format!(
                    "  {label}: VirtualKeyCode::{variant} is mapped by {actual} source code(s) \
                     but has no MANIFEST row"
                ));
            }
        }
    }

    assert!(
        drift.is_empty(),
        "the platform keycode tables drifted from MANIFEST:\n{}\n\nA count that DROPPED is an arm \
         somebody deleted — that key is now dead on that platform. A count that ROSE is coverage \
         somebody added; record it in the manifest so the next deletion is caught too.",
        drift.join("\n")
    );
}

#[test]
fn the_exemption_list_has_no_stale_entries() {
    let tables = platform_tables();
    for (name, reason) in EXEMPT {
        assert!(
            !reason.trim().is_empty(),
            "`{name}` is exempted with an empty reason — an exemption without a reason is a hole \
             with paperwork"
        );
        for (label, table) in &tables {
            assert!(
                !table.iter().any(|(variant, _)| variant == name),
                "`{name}` is on the exemption list but {label} now maps it — the exemption is \
                 stale and is masking a real row. Move it into MANIFEST (reason on file: {reason})"
            );
        }
    }
}

/// Wayland must keep DELEGATING, or the manifest's "X11 + Wayland" column is a
/// lie.
///
/// Its own table used to end in `_ => VirtualKeyCode::Escape`, so every key it
/// did not know pressed and released Escape — dismissing menus and firing
/// Escape default actions on, among others, every punctuation key.
#[test]
fn wayland_still_delegates_to_the_x11_table() {
    let body = fn_body(
        WAYLAND_EVENTS_SRC,
        "pub(super) fn keysym_to_virtual_keycode",
        "linux/wayland/events.rs",
    );

    assert!(
        body.contains("x11::events::keysym_to_virtual_keycode"),
        "the Wayland keysym translation no longer delegates to the X11 table. If it grew a table \
         of its own it needs its own MANIFEST column — the current one claims the two backends \
         share a table:\n{body}"
    );
    assert!(
        mapped_codes(&body).is_empty(),
        "the Wayland keysym translation grew match arms of its own ({:?}); it must delegate, or \
         the manifest's shared X11/Wayland column stops being true",
        mapped_codes(&body)
    );
    assert!(
        !body.contains("VirtualKeyCode::Escape"),
        "the Wayland catch-all is answering Escape again. An unknown keysym must resolve to \
         `None`, not to a key that dismisses menus:\n{body}"
    );
}
