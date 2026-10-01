//! AzCalculator's window: azul's S9 `UtilityShell` with the app-drawn title
//! row (`NoTitle` + `Titlebar`), the mode switch (Standard, Scientific,
//! Programmer, Date, Convert), the display and keypad of each mode, the
//! History / Memory panel beside the keypad when the window is wide enough
//! (Ctrl+H shows it otherwise), and the shared settings page of azul-appkit.
//!
//! Keyboard: every key of the keypad has a key (calc::char_command and
//! calc::named_command; the US key positions give the shifted characters),
//! Enter evaluates, Escape clears, Backspace deletes, Ctrl/Cmd+C copies the
//! result, Ctrl/Cmd+V pastes an expression, Alt+1..5 switch the mode,
//! Ctrl+M / R / P / Q / L are MS / MR / M+ / M- / MC (as on Windows),
//! F3 / F4 / F5 pick DEG / RAD / GRAD (Scientific), F5..F8 HEX / DEC / OCT
//! / BIN (Programmer); Mod+, opens the settings (appkit).
//!
//! The history lives in `calculator/history.jsonl` in the user's data
//! folder, read when the window opens and written after every calculation,
//! always on an azul Thread through azul-storage (appkit::ui::spawn_file_jobs).
//!
//! On stdout, for scripts/azcalculator_e2e.py: `AZCALC_SCREEN <name>`,
//! `AZCALC_DISPLAY <expression line>\t<result line>` after every key,
//! `AZCALC_COPIED <text>`, `AZCALC_PASTED <text>`, `AZCALC_HISTORY_LOADED <n>`,
//! `AZCALC_HISTORY_SAVED <n>`, `AZCALC_CONVERT <line>`, `AZCALC_DATE <line>`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        SwitchOnToggleCallbackType, TabOnClickCallbackType, TextInputOnTextInputCallbackType,
    },
    dom::{ClipboardContent, VirtualKeyCode},
    option::OptionString,
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope, UtilityShell},
    str::String as AzString,
    vec::{StringVec, StyledTextRunVec},
    widgets::{
        ButtonType, DropDown, OnTextInputReturn, Segmented, SegmentedState, Switch, SwitchState,
        TabHeader, TabHeaderState, TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    shortcuts::Shortcut,
    ui::{self as kit, AppSection},
};

use crate::calc::{char_command, named_command, CalcMode, Calculator, Cmd, NamedKey};
use crate::datecalc::{self, Date};
use crate::expr::{AngleUnit, BinOp, Const, Func, Post};
use crate::history::{self, HistoryEntry};
use crate::num::Num;
use crate::programmer::{self, Base, WordSize};
use crate::units::{self, CATEGORIES};

// ==== The app's facts ====

/// The screens `--screen` opens.
pub const SCREENS: [&str; 6] = ["standard", "scientific", "programmer", "date", "convert", "settings"];

pub const SPEC: AppSpec = AppSpec {
    name: "AzCalculator",
    binary: "AzCalculator",
    summary: "a calculator: standard, scientific, programmer, dates, units",
    screens: &SCREENS,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzCalculator",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Standard, scientific and programmer calculations with exact decimals, date \
              calculations and a unit converter. Part of the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "calculator",
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 17] = [
    Shortcut::new("Calculator", "Enter", "Evaluate (=)"),
    Shortcut::new("Calculator", "Escape", "Clear (C)"),
    Shortcut::new("Calculator", "Delete", "Clear the entry (CE)"),
    Shortcut::new("Calculator", "Backspace", "Delete the last digit"),
    Shortcut::new("Calculator", "F9", "Change the sign (+/-)"),
    Shortcut::new("Calculator", "Mod+C", "Copy the result"),
    Shortcut::new("Calculator", "Mod+V", "Paste a number or an expression"),
    Shortcut::new("Calculator", "@  q  r", "Square root, square, reciprocal"),
    Shortcut::new("Memory", "Ctrl+M  Ctrl+R", "Memory store, recall"),
    Shortcut::new("Memory", "Ctrl+P  Ctrl+Q", "Memory add, subtract"),
    Shortcut::new("Memory", "Ctrl+L", "Memory clear"),
    Shortcut::new("Modes", "Alt+1 .. Alt+5", "Standard, Scientific, Programmer, Date, Convert"),
    Shortcut::new("Modes", "Ctrl+H", "Show or hide the history"),
    Shortcut::new("Scientific", "s o t n l p", "sin, cos, tan, ln, log, pi"),
    Shortcut::new("Scientific", "F3 F4 F5", "Degrees, radians, grads"),
    Shortcut::new("Programmer", "& | ^ ~ < > %", "AND, OR, XOR, NOT, shifts, mod"),
    Shortcut::new("Programmer", "F5 F6 F7 F8", "HEX, DEC, OCT, BIN"),
];

/// The settings page's own category.
const APP_CATEGORIES: [&str; 1] = ["Calculator"];

/// The history file's key.
fn history_key() -> String {
    azul_appkit::data::app_key(ABOUT.app_folder, history::HISTORY_FILE)
}

/// Write-back tags.
const TAG_LOAD: u64 = 1;
const TAG_SAVE: u64 = 2;

/// The window is wide enough for the History / Memory panel beside the keypad.
const PANEL_MIN_WIDTH: f32 = 620.0;

// ==== State ====

/// The five screens of the mode switch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Standard,
    Scientific,
    Programmer,
    Date,
    Convert,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Standard,
        Screen::Scientific,
        Screen::Programmer,
        Screen::Date,
        Screen::Convert,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Screen::Standard => "Standard",
            Screen::Scientific => "Scientific",
            Screen::Programmer => "Programmer",
            Screen::Date => "Date",
            Screen::Convert => "Convert",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Screen::Standard => "standard",
            Screen::Scientific => "scientific",
            Screen::Programmer => "programmer",
            Screen::Date => "date",
            Screen::Convert => "convert",
        }
    }

    pub fn by_key(key: &str) -> Option<Screen> {
        Screen::ALL.into_iter().find(|s| s.key() == key)
    }

    pub fn index(self) -> usize {
        Screen::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// The keypad mode a calculator screen uses.
    pub fn calc_mode(self) -> Option<CalcMode> {
        match self {
            Screen::Standard => Some(CalcMode::Standard),
            Screen::Scientific => Some(CalcMode::Scientific),
            Screen::Programmer => Some(CalcMode::Programmer),
            Screen::Date | Screen::Convert => None,
        }
    }
}

/// The panel beside the keypad.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    History,
    Memory,
}

/// Which converter field the user typed into last.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    From,
    To,
}

/// The unit converter.
#[derive(Clone, Debug)]
pub struct ConvertState {
    pub category: usize,
    pub from: usize,
    pub to: usize,
    /// The two fields' texts.
    pub from_text: String,
    pub to_text: String,
    /// The field typed into last (the other one is computed).
    pub source: Side,
    /// "42.195 km = 26.2188 mi", newest first.
    pub recent: Vec<String>,
}

/// The date calculation.
#[derive(Clone, Debug)]
pub struct DateState {
    /// 0 = difference between dates, 1 = add or subtract.
    pub kind: usize,
    pub from: String,
    pub to: String,
    pub years: String,
    pub months: String,
    pub days: String,
    pub subtract: bool,
}

/// The app's state.
pub struct CalcApp {
    pub kit: RefAny,
    pub calc: Calculator,
    pub screen: Screen,
    pub panel: Panel,
    /// Ctrl+H / the History button: `None` = by the window's width.
    pub panel_shown: Option<bool>,
    pub convert: ConvertState,
    pub date: DateState,
    /// The data root (from the kit).
    pub data_root: PathBuf,
    pub sample: bool,
    pub history_loaded: bool,
    pub saving: bool,
    pub save_pending: bool,
    /// The last notice (a failed save, a paste that could not be read).
    pub notice: String,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Today in the local calendar is not needed to the second: UTC days.
fn today() -> Date {
    Date::from_days((now_secs() / 86_400) as i64)
}

/// The sample history (`--sample` with an empty history), from the plan.
fn sample_history() -> Vec<HistoryEntry> {
    let entry = |expr: &str, result: &str| HistoryEntry {
        expr: expr.to_string(),
        result: result.to_string(),
        mode: CalcMode::Standard.key().to_string(),
        at: now_secs(),
    };
    vec![
        entry("1,280 \u{d7} 0.19", "243.2"),
        entry("243.2 + 18.5", "261.7"),
        entry("\u{221a}(2)", "1.4142135623730950488016887242097"),
    ]
}

impl CalcApp {
    fn new(kit_ref: RefAny, args: &AppArgs) -> CalcApp {
        let mut k = kit_ref.clone();
        let (grouping, angle, last_screen, data_root) = match k.downcast_ref::<kit::Kit>() {
            Some(kit) => (
                kit.settings.get_bool("grouping", true),
                kit.settings.get("angle").map(str::to_string),
                kit.settings.get("screen").map(str::to_string),
                kit.data_root.clone(),
            ),
            None => (true, None, None, PathBuf::from(".")),
        };
        let mut calc = Calculator::new();
        calc.grouping = grouping;
        calc.angle = match angle.as_deref() {
            Some("rad") => AngleUnit::Rad,
            Some("grad") => AngleUnit::Grad,
            _ => AngleUnit::Deg,
        };
        let screen = args
            .screen
            .as_deref()
            .and_then(Screen::by_key)
            .or_else(|| last_screen.as_deref().and_then(Screen::by_key))
            .unwrap_or(Screen::Standard);
        if let Some(mode) = screen.calc_mode() {
            calc.set_mode(mode);
        }
        let length = &CATEGORIES[0];
        let mut convert = ConvertState {
            category: 0,
            from: length.default_pair.0,
            to: length.default_pair.1,
            from_text: "42.195".to_string(),
            to_text: String::new(),
            source: Side::From,
            recent: Vec::new(),
        };
        if args.sample {
            convert.recent = vec!["5.9166 ft = 1.80339 m".to_string(), "100 \u{b0}F = 37.7778 \u{b0}C".to_string()];
        }
        let t = today();
        CalcApp {
            kit: kit_ref,
            calc,
            screen,
            panel: Panel::History,
            panel_shown: None,
            convert,
            date: DateState {
                kind: 0,
                from: t.to_string(),
                to: t.add_days(100).unwrap_or(t).to_string(),
                years: "0".to_string(),
                months: "0".to_string(),
                days: "100".to_string(),
                subtract: false,
            },
            data_root,
            sample: args.sample,
            history_loaded: false,
            saving: false,
            save_pending: false,
            notice: String::new(),
        }
    }

    /// Prints the display lines for scripts.
    fn announce_display(&self) {
        println!(
            "AZCALC_DISPLAY {}\t{}",
            self.calc.expression_line(),
            self.calc.result_line()
        );
    }
}

/// The app's start: switches, the kit (settings, data root), the window.
pub fn start() {
    let args = match AppArgs::from_env(&SPEC) {
        Ok(a) => a,
        Err(message) => {
            println!("{message}");
            std::process::exit(if message.contains("USAGE") { 0 } else { 2 });
        }
    };
    let kit_ref = kit::create_kit(SPEC, ABOUT, &SHORTCUTS, &APP_CATEGORIES, args.clone());
    if args.screen.as_deref() == Some("settings") {
        kit::open_settings(&kit_ref, None);
    }
    let app = CalcApp::new(kit_ref.clone(), &args);
    println!("AZCALC_SCREEN {}", app.screen.key());
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(&kit_ref, layout, (680.0, 620.0), (320.0, 480.0), on_window_created);
    App::create(RefAny::new(app), config).run(window);
}
