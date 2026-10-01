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

// ==== Keypads ====

/// What a keypad key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Calc(Cmd),
    /// 2nd: the function keys' second meanings.
    Second,
    /// F-E: scientific notation.
    FlipFe,
    /// An empty cell.
    Blank,
}

/// One key of a keypad.
#[derive(Clone, Copy, Debug)]
pub struct KeyDef {
    pub label: &'static str,
    /// The DOM id (`key-7`, `key-plus`).
    pub id: &'static str,
    pub action: Action,
    /// Drawn as the primary button (`=`, an active toggle).
    pub primary: bool,
}

const fn key(label: &'static str, id: &'static str, cmd: Cmd) -> KeyDef {
    KeyDef {
        label,
        id,
        action: Action::Calc(cmd),
        primary: false,
    }
}

const BLANK: KeyDef = KeyDef {
    label: "",
    id: "",
    action: Action::Blank,
    primary: false,
};

const fn digit_key(label: &'static str, id: &'static str, d: u8) -> KeyDef {
    key(label, id, Cmd::Digit(d))
}

const EQUALS: KeyDef = KeyDef {
    label: "=",
    id: "key-equals",
    action: Action::Calc(Cmd::Equals),
    primary: true,
};

/// Standard: 4 columns (the plan's 2.1).
pub fn standard_keys() -> Vec<KeyDef> {
    vec![
        key("%", "key-percent", Cmd::Post(Post::Percent)),
        key("CE", "key-ce", Cmd::ClearEntry),
        key("C", "key-c", Cmd::Clear),
        key("\u{232b}", "key-back", Cmd::Backspace),
        key("1/x", "key-recip", Cmd::Func(Func::Recip)),
        key("x\u{b2}", "key-square", Cmd::Post(Post::Square)),
        key("\u{221a}x", "key-sqrt", Cmd::Func(Func::Sqrt)),
        key("\u{f7}", "key-divide", Cmd::Op(BinOp::Div)),
        digit_key("7", "key-7", 7),
        digit_key("8", "key-8", 8),
        digit_key("9", "key-9", 9),
        key("\u{d7}", "key-multiply", Cmd::Op(BinOp::Mul)),
        digit_key("4", "key-4", 4),
        digit_key("5", "key-5", 5),
        digit_key("6", "key-6", 6),
        key("\u{2212}", "key-minus", Cmd::Op(BinOp::Sub)),
        digit_key("1", "key-1", 1),
        digit_key("2", "key-2", 2),
        digit_key("3", "key-3", 3),
        key("+", "key-plus", Cmd::Op(BinOp::Add)),
        key("+/\u{2212}", "key-negate", Cmd::Negate),
        digit_key("0", "key-0", 0),
        key(".", "key-point", Cmd::Point),
        EQUALS,
    ]
}

/// The memory row of Standard and Scientific.
pub fn memory_keys() -> Vec<KeyDef> {
    vec![
        key("MC", "key-mc", Cmd::MemClear),
        key("MR", "key-mr", Cmd::MemRecall),
        key("M+", "key-mplus", Cmd::MemAdd),
        key("M\u{2212}", "key-mminus", Cmd::MemSub),
        key("MS", "key-ms", Cmd::MemStore),
    ]
}

/// Scientific: 9 columns, the functions left of the digits (the plan's 2.2);
/// 2nd swaps x² / x³, √ / ∛, sin / cos / tan and their inverses, 10^x / e^x,
/// log / log2.
pub fn scientific_keys(second: bool, fe: bool) -> Vec<KeyDef> {
    let pick = |a: KeyDef, b: KeyDef| if second { b } else { a };
    vec![
        KeyDef {
            label: "2nd",
            id: "key-second",
            action: Action::Second,
            primary: second,
        },
        key("\u{3c0}", "key-pi", Cmd::Const(Const::Pi)),
        key("e", "key-e", Cmd::Const(Const::E)),
        key("C", "key-c", Cmd::Clear),
        key("\u{232b}", "key-back", Cmd::Backspace),
        key("(", "key-lparen", Cmd::LParen),
        key(")", "key-rparen", Cmd::RParen),
        key("n!", "key-factorial", Cmd::Post(Post::Factorial)),
        key("\u{f7}", "key-divide", Cmd::Op(BinOp::Div)),
        // row 2
        pick(
            key("x\u{b2}", "key-square", Cmd::Post(Post::Square)),
            key("x\u{b3}", "key-cube", Cmd::Post(Post::Cube)),
        ),
        key("x^y", "key-pow", Cmd::Op(BinOp::Pow)),
        pick(key("sin", "key-sin", Cmd::Func(Func::Sin)), key("sin\u{207b}\u{b9}", "key-asin", Cmd::Func(Func::Asin))),
        pick(key("cos", "key-cos", Cmd::Func(Func::Cos)), key("cos\u{207b}\u{b9}", "key-acos", Cmd::Func(Func::Acos))),
        pick(key("tan", "key-tan", Cmd::Func(Func::Tan)), key("tan\u{207b}\u{b9}", "key-atan", Cmd::Func(Func::Atan))),
        digit_key("7", "key-7", 7),
        digit_key("8", "key-8", 8),
        digit_key("9", "key-9", 9),
        key("\u{d7}", "key-multiply", Cmd::Op(BinOp::Mul)),
        // row 3
        pick(
            key("\u{221a}x", "key-sqrt", Cmd::Func(Func::Sqrt)),
            key("\u{221b}x", "key-cbrt", Cmd::Func(Func::Cbrt)),
        ),
        pick(key("10^x", "key-pow10", Cmd::Func(Func::Pow10)), key("e^x", "key-exp", Cmd::Func(Func::Exp))),
        pick(key("log", "key-log", Cmd::Func(Func::Log)), key("log\u{2082}", "key-log2", Cmd::Func(Func::Log2))),
        key("ln", "key-ln", Cmd::Func(Func::Ln)),
        key("Exp", "key-exponent", Cmd::Exp),
        digit_key("4", "key-4", 4),
        digit_key("5", "key-5", 5),
        digit_key("6", "key-6", 6),
        key("\u{2212}", "key-minus", Cmd::Op(BinOp::Sub)),
        // row 4
        key("|x|", "key-abs", Cmd::Func(Func::Abs)),
        key("1/x", "key-recip", Cmd::Func(Func::Recip)),
        key("mod", "key-mod", Cmd::Op(BinOp::Mod)),
        KeyDef {
            label: "F-E",
            id: "key-fe",
            action: Action::FlipFe,
            primary: fe,
        },
        key("%", "key-percent", Cmd::Post(Post::Percent)),
        digit_key("1", "key-1", 1),
        digit_key("2", "key-2", 2),
        digit_key("3", "key-3", 3),
        key("+", "key-plus", Cmd::Op(BinOp::Add)),
        // row 5
        key("MC", "key-mc", Cmd::MemClear),
        key("MR", "key-mr", Cmd::MemRecall),
        key("M+", "key-mplus", Cmd::MemAdd),
        key("MS", "key-ms", Cmd::MemStore),
        key("CE", "key-ce", Cmd::ClearEntry),
        key("+/\u{2212}", "key-negate", Cmd::Negate),
        digit_key("0", "key-0", 0),
        key(".", "key-point", Cmd::Point),
        EQUALS,
    ]
}

/// Programmer: 11 columns, the operators left of the hex and decimal digits
/// (the plan's 2.3).
pub fn programmer_keys() -> Vec<KeyDef> {
    vec![
        key("AND", "key-and", Cmd::Op(BinOp::And)),
        key("OR", "key-or", Cmd::Op(BinOp::Or)),
        key("XOR", "key-xor", Cmd::Op(BinOp::Xor)),
        key("NOT", "key-not", Cmd::Not),
        key("<<", "key-shl", Cmd::Op(BinOp::Shl)),
        key(">>", "key-shr", Cmd::Op(BinOp::Shr)),
        digit_key("A", "key-a", 10),
        digit_key("B", "key-b", 11),
        digit_key("7", "key-7", 7),
        digit_key("8", "key-8", 8),
        digit_key("9", "key-9", 9),
        // row 2
        key("ROL", "key-rol", Cmd::Op(BinOp::Rol)),
        key("ROR", "key-ror", Cmd::Op(BinOp::Ror)),
        key("mod", "key-mod", Cmd::Op(BinOp::Mod)),
        key("(", "key-lparen", Cmd::LParen),
        key(")", "key-rparen", Cmd::RParen),
        key("\u{f7}", "key-divide", Cmd::Op(BinOp::Div)),
        digit_key("C", "key-hex-c", 12),
        digit_key("D", "key-d", 13),
        digit_key("4", "key-4", 4),
        digit_key("5", "key-5", 5),
        digit_key("6", "key-6", 6),
        // row 3
        key("CE", "key-ce", Cmd::ClearEntry),
        key("C", "key-c", Cmd::Clear),
        key("\u{232b}", "key-back", Cmd::Backspace),
        key("+/\u{2212}", "key-negate", Cmd::Negate),
        key("\u{d7}", "key-multiply", Cmd::Op(BinOp::Mul)),
        key("\u{2212}", "key-minus", Cmd::Op(BinOp::Sub)),
        digit_key("E", "key-hex-e", 14),
        digit_key("F", "key-f", 15),
        digit_key("1", "key-1", 1),
        digit_key("2", "key-2", 2),
        digit_key("3", "key-3", 3),
        // row 4
        key("NAND", "key-nand", Cmd::Op(BinOp::Nand)),
        key("NOR", "key-nor", Cmd::Op(BinOp::Nor)),
        key("x\u{b2}", "key-square", Cmd::Post(Post::Square)),
        key("x^y", "key-pow", Cmd::Op(BinOp::Pow)),
        key("+", "key-plus", Cmd::Op(BinOp::Add)),
        EQUALS,
        BLANK,
        BLANK,
        BLANK,
        digit_key("0", "key-0", 0),
        BLANK,
    ]
}

/// The data of a key's click: the app and what the key does.
struct KeyRef {
    app: RefAny,
    action: Action,
}

fn grid(columns: usize, cells: Vec<Dom>, id: &str) -> Dom {
    let template = vec!["1fr"; columns].join(" ");
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: grid; grid-template-columns: {template}; gap: 4px; padding: 4px 8px 8px 8px;"
        ))
        .with_children(DomVec::from_vec(cells))
}

/// A keypad of `keys` in `columns` columns. A digit the base does not take
/// is drawn dimmed and does nothing.
fn keypad(app: &RefAny, keys: &[KeyDef], columns: usize, base: Option<Base>, id: &str) -> Dom {
    let cells: Vec<Dom> = keys
        .iter()
        .map(|k| {
            let enabled = match (k.action, base) {
                (Action::Blank, _) => false,
                (Action::Calc(Cmd::Digit(d)), Some(b)) => b.accepts(d),
                _ => true,
            };
            if !enabled {
                return Dom::create_div()
                    .with_css("display: flex; align-items: center; justify-content: center; opacity: 0.35; font-size: 14px;")
                    .with_child(Dom::create_span_with_text(k.label));
            }
            let button = Button::create(k.label).with_on_click(
                RefAny::new(KeyRef {
                    app: app.clone(),
                    action: k.action,
                }),
                on_key_button as ButtonOnClickCallbackType,
            );
            let button = if k.primary {
                button.with_button_type(ButtonType::Primary)
            } else {
                button
            };
            button.dom().with_id(k.id).with_css("min-height: 34px;")
        })
        .collect();
    grid(columns, cells, id)
}

// ==== The display ====

fn line(id: &str, text: &str, css: &str) -> Dom {
    let div = Dom::create_div()
        .with_css(format!("text-align: right; {css}"))
        .with_child(Dom::create_span_with_text(text));
    if id.is_empty() {
        div
    } else {
        div.with_id(id)
    }
}

fn display(s: &CalcApp) -> Dom {
    let big = if s.screen == Screen::Programmer { 26 } else { 34 };
    let mut expression = s.calc.expression_line();
    let open = s.calc.open_parens();
    if open > 0 && !s.calc.just_evaluated {
        expression.push_str(&format!("   ({open} open)"));
    }
    let mut column = Dom::create_div()
        .with_id("calc-display")
        .with_css("display: flex; flex-direction: column; padding: 8px 12px 4px 12px; flex-shrink: 0;")
        .with_child(line("calc-expression", &expression, "font-size: 14px; opacity: 0.7; min-height: 20px;"))
        .with_child(line(
            "calc-result",
            &s.calc.result_line(),
            &format!("font-size: {big}px; font-weight: 600; min-height: {}px;", big + 8),
        ));
    if !s.notice.is_empty() {
        column.add_child(line("calc-notice", &s.notice, "font-size: 12px; opacity: 0.8;"));
    }
    column
}

struct BaseRef {
    app: RefAny,
    base: Base,
}

struct BitRef {
    app: RefAny,
    bit: u32,
}

/// Programmer: the four bases (click one to type in it), the word size and the bit field.
fn programmer_panel(s: &CalcApp, app: &RefAny) -> Dom {
    let mut rows = Dom::create_div().with_id("calc-bases").with_css("display: flex; flex-direction: column; padding: 0px 12px;");
    for (base, text) in s.calc.programmer_lines() {
        let selected = base == s.calc.base;
        rows.add_child(
            Dom::create_div()
                .with_id(format!("base-{}", base.label().to_lowercase()))
                .with_css(format!(
                    "display: flex; flex-direction: row; padding: 2px 4px; cursor: pointer; font-size: 13px; {}",
                    if selected { "font-weight: 700;" } else { "opacity: 0.8;" }
                ))
                .with_child(Dom::create_div().with_css("width: 48px;").with_child(Dom::create_span_with_text(base.label())))
                .with_child(Dom::create_div().with_child(Dom::create_span_with_text(text)))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    RefAny::new(BaseRef {
                        app: app.clone(),
                        base,
                    }),
                    on_base,
                ),
        );
    }
    let words: Vec<&str> = WordSize::ALL.iter().map(|w| w.label()).collect();
    let word = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 12px;")
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(
            DropDown::create(strs(&words))
                .with_selected(s.calc.word.index())
                .with_accessibility_name("Word size")
                .with_on_choice_change(app.clone(), on_word as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id("calc-word"),
        );
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-shrink: 0;")
        .with_child(word)
        .with_child(rows)
        .with_child(bit_field(s, app))
}

/// 64 bits in four rows of 16 (63..48 at the top), groups of four; bits
/// beyond the word size are dimmed and do nothing.
fn bit_field(s: &CalcApp, app: &RefAny) -> Dom {
    let value = match s.calc.current_value() {
        Some(crate::calc::Value::Int(v)) => v,
        _ => 0,
    };
    let word = s.calc.word;
    let mut field = Dom::create_div()
        .with_id("calc-bits")
        .with_css("display: flex; flex-direction: column; padding: 4px 12px; font-size: 12px;");
    for row in 0..4u32 {
        let top = 63 - row * 16;
        let mut line = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center;")
            .with_child(
                Dom::create_div()
                    .with_css("width: 24px; opacity: 0.6;")
                    .with_child(Dom::create_span_with_text(top.to_string())),
            );
        for i in 0..16u32 {
            let bit = top - i;
            let set = programmer::bit(value, bit, word);
            let inside = bit < word.bits();
            let mut cell = Dom::create_div()
                .with_id(format!("bit-{bit}"))
                .with_css(format!(
                    "width: 14px; text-align: center; {}{}",
                    if inside { "cursor: pointer;" } else { "opacity: 0.3;" },
                    if i % 4 == 3 { " margin-right: 8px;" } else { "" }
                ))
                .with_child(Dom::create_span_with_text(if set { "1" } else { "0" }));
            if inside {
                cell = cell.with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    RefAny::new(BitRef { app: app.clone(), bit }),
                    on_bit,
                );
            }
            line.add_child(cell);
        }
        field.add_child(line);
    }
    field
}

/// Scientific: the angle unit beside the display.
fn angle_row(s: &CalcApp, app: &RefAny) -> Dom {
    let labels: Vec<&str> = AngleUnit::ALL.iter().map(|a| a.label()).collect();
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; padding: 0px 8px 4px 8px;")
        .with_child(
            Segmented::create(strs(&labels))
                .with_selected_index(s.calc.angle.index())
                .with_on_change(app.clone(), on_angle as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("calc-angle"),
        )
}

// ==== History and memory ====

struct HistoryRef {
    app: RefAny,
    index: usize,
}

fn history_list(s: &CalcApp, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id("calc-history")
        .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; padding: 4px 8px;");
    if s.calc.history.is_empty() {
        list.add_child(
            Dom::create_div()
                .with_css("padding: 12px; opacity: 0.7; font-size: 13px;")
                .with_child(Dom::create_span_with_text("There's no history yet.")),
        );
    }
    for (index, e) in s.calc.history.iter().enumerate().rev() {
        list.add_child(
            Dom::create_div()
                .with_class("calc-history-entry")
                .with_css("display: flex; flex-direction: column; padding: 6px 8px; cursor: pointer;")
                .with_child(line("", &format!("{} =", e.expr), "font-size: 12px; opacity: 0.7;"))
                .with_child(line("", &e.result, "font-size: 18px; font-weight: 600;"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    RefAny::new(HistoryRef { app: app.clone(), index }),
                    on_history_entry,
                ),
        );
    }
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(list);
    if !s.calc.history.is_empty() {
        column.add_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; justify-content: flex-end; padding: 6px 8px;")
                .with_child(
                    Button::create("Clear history")
                        .with_icon("delete")
                        .with_on_click(app.clone(), on_clear_history as ButtonOnClickCallbackType)
                        .dom()
                        .with_id("calc-clear-history"),
                ),
        );
    }
    column
}

fn memory_list(s: &CalcApp, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id("calc-memory")
        .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; padding: 4px 8px;");
    if s.calc.memory.items.is_empty() {
        list.add_child(
            Dom::create_div()
                .with_css("padding: 12px; opacity: 0.7; font-size: 13px;")
                .with_child(Dom::create_span_with_text("There's nothing saved in memory.")),
        );
    }
    for (index, item) in s.calc.memory.items.iter().enumerate() {
        let shown = Num::parse(item)
            .map(|n| n.format(&crate::num::Format { grouping: s.calc.grouping, ..crate::num::Format::default() }))
            .unwrap_or_else(|_| item.clone());
        list.add_child(
            Dom::create_div()
                .with_class("calc-memory-entry")
                .with_css("padding: 6px 8px; cursor: pointer;")
                .with_child(line("", &shown, "font-size: 18px; font-weight: 600;"))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    RefAny::new(HistoryRef { app: app.clone(), index }),
                    on_memory_entry,
                ),
        );
    }
    list
}

fn side_panel(s: &CalcApp, app: &RefAny) -> Dom {
    let active = match s.panel {
        Panel::History => 0,
        Panel::Memory => 1,
    };
    let body = match s.panel {
        Panel::History => history_list(s, app),
        Panel::Memory => memory_list(s, app),
    };
    Dom::create_aside()
        .with_id("calc-panel")
        .with_accessibility_name("History and memory")
        .with_css("display: flex; flex-direction: column; width: 260px; flex-shrink: 0; min-height: 0px;")
        .with_child(
            TabHeader::create(strs(&["History", "Memory"]))
                .with_active_tab(active)
                .with_on_click(app.clone(), on_panel_tab as TabOnClickCallbackType)
                .dom()
                .with_id("calc-panel-tabs"),
        )
        .with_child(body)
}

// ==== The screens ====

fn calculator_view(s: &CalcApp, app: &RefAny, wide: bool) -> Dom {
    let panel_visible = s.panel_shown.unwrap_or(wide);
    if panel_visible && !wide {
        // A narrow window: the panel takes the keypad's place.
        return side_panel(s, app);
    }
    let mut column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px;")
        .with_child(display(s));
    match s.screen {
        Screen::Standard => {
            column.add_child(keypad(app, &memory_keys(), 5, None, "calc-memory-row"));
            column.add_child(keypad(app, &standard_keys(), 4, None, "calc-keypad"));
        }
        Screen::Scientific => {
            column.add_child(angle_row(s, app));
            column.add_child(keypad(app, &scientific_keys(s.calc.second, s.calc.fe), 9, None, "calc-keypad"));
        }
        _ => {
            column.add_child(programmer_panel(s, app));
            column.add_child(keypad(app, &programmer_keys(), 11, Some(s.calc.base), "calc-keypad"));
        }
    }
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
        .with_child(column);
    if panel_visible {
        row.add_child(side_panel(s, app));
    }
    row
}

/// The fields of the date and converter screens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    DateFrom,
    DateTo,
    Years,
    Months,
    Days,
    ConvertFrom,
    ConvertTo,
}

struct FieldRef {
    app: RefAny,
    field: Field,
}

fn field_input(app: &RefAny, field: Field, text: &str, name: &str, id: &str) -> Dom {
    TextInput::create()
        .with_text(text)
        .with_accessibility_name(name)
        .with_on_text_input(
            RefAny::new(FieldRef {
                app: app.clone(),
                field,
            }),
            on_field_text as TextInputOnTextInputCallbackType,
        )
        .dom()
        .with_id(id)
}

fn labelled(label: &str, control: Dom) -> Dom {
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 0px;")
        .with_child(
            Dom::create_div()
                .with_css("width: 110px; flex-shrink: 0; font-size: 13px;")
                .with_child(Dom::create_span_with_text(label)),
        )
        .with_child(control)
}

/// The date screen's result lines, or why there is none.
pub fn date_result(d: &DateState) -> Vec<String> {
    let Some(from) = datecalc::parse_date(&d.from) else {
        return vec!["Enter the first date as YYYY-MM-DD.".to_string()];
    };
    if d.kind == 0 {
        let Some(to) = datecalc::parse_date(&d.to) else {
            return vec!["Enter the second date as YYYY-MM-DD.".to_string()];
        };
        let diff = datecalc::difference(from, to);
        if diff.total_days == 0 {
            return vec![diff.describe()];
        }
        vec![diff.describe(), diff.in_weeks(), diff.in_days()]
    } else {
        let number = |t: &str| t.trim().parse::<i64>().ok().filter(|n| (0..=100_000).contains(n));
        let (Some(y), Some(m), Some(n)) = (number(&d.years), number(&d.months), number(&d.days)) else {
            return vec!["Years, months and days are whole numbers.".to_string()];
        };
        match datecalc::add(from, y, m, n, d.subtract) {
            Some(date) => vec![format!("{} ({})", date, date.weekday_name())],
            None => vec!["The date is outside the years 1 to 9999.".to_string()],
        }
    }
}

fn date_view(s: &CalcApp, app: &RefAny) -> Dom {
    let d = &s.date;
    let mut column = Dom::create_div()
        .with_id("date-view")
        .with_css("display: flex; flex-direction: column; padding: 12px 16px; flex-grow: 1;")
        .with_child(
            Segmented::create(strs(&["Difference between dates", "Add or subtract days"]))
                .with_selected_index(d.kind)
                .with_on_change(app.clone(), on_date_kind as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("date-kind"),
        )
        .with_child(labelled("From", field_input(app, Field::DateFrom, &d.from, "From date", "date-from")));
    if d.kind == 0 {
        column.add_child(labelled("To", field_input(app, Field::DateTo, &d.to, "To date", "date-to")));
    } else {
        column.add_child(labelled(
            "",
            Segmented::create(strs(&["Add", "Subtract"]))
                .with_selected_index(usize::from(d.subtract))
                .with_on_change(app.clone(), on_date_sign as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("date-sign"),
        ));
        column.add_child(labelled("Years", field_input(app, Field::Years, &d.years, "Years", "date-years")));
        column.add_child(labelled("Months", field_input(app, Field::Months, &d.months, "Months", "date-months")));
        column.add_child(labelled("Days", field_input(app, Field::Days, &d.days, "Days", "date-days")));
    }
    column.add_child(
        Dom::create_div().with_css("padding: 4px 0px 8px 110px;").with_child(
            Button::create("Today")
                .with_icon("today")
                .with_on_click(app.clone(), on_date_today as ButtonOnClickCallbackType)
                .dom()
                .with_id("date-today"),
        ),
    );
    let mut result = Dom::create_div()
        .with_id("date-result")
        .with_css("display: flex; flex-direction: column; padding-top: 8px;");
    for (i, text) in date_result(d).into_iter().enumerate() {
        let css = if i == 0 { "font-size: 22px; font-weight: 600;" } else { "font-size: 14px; opacity: 0.8;" };
        result.add_child(
            Dom::create_div()
                .with_css(css)
                .with_child(Dom::create_span_with_text(text)),
        );
    }
    column.with_child(result)
}

/// The converter's two values: the source field as typed, the other computed.
pub fn convert_values(c: &ConvertState) -> (String, String) {
    let category = &CATEGORIES[c.category.min(CATEGORIES.len() - 1)];
    let (from, to) = (&category.units[c.from], &category.units[c.to]);
    let compute = |text: &str, a: &units::Unit, b: &units::Unit| -> String {
        if text.trim().is_empty() {
            return String::new();
        }
        match Num::parse(text).and_then(|v| units::convert(&v, a, b)) {
            Ok(v) => units::show(&v, 10),
            Err(_) => "\u{2014}".to_string(),
        }
    };
    match c.source {
        Side::From => (c.from_text.clone(), compute(&c.from_text, from, to)),
        Side::To => (compute(&c.to_text, to, from), c.to_text.clone()),
    }
}

fn convert_view(s: &CalcApp, app: &RefAny) -> Dom {
    let c = &s.convert;
    let category = &CATEGORIES[c.category.min(CATEGORIES.len() - 1)];
    let names: Vec<&str> = CATEGORIES.iter().map(|c| c.name).collect();
    let unit_names: Vec<String> = category.units.iter().map(|u| format!("{} ({})", u.name, u.symbol)).collect();
    let unit_strs: Vec<&str> = unit_names.iter().map(String::as_str).collect();
    let (from_text, to_text) = convert_values(c);
    let unit_drop = |selected: usize, id: &str, name: &str, cb: DropDownOnChoiceChangeCallbackType| {
        DropDown::create(strs(&unit_strs))
            .with_selected(selected)
            .with_accessibility_name(name)
            .with_on_choice_change(app.clone(), cb)
            .dom()
            .with_id(id)
    };
    let rate = units::rate_line(&category.units[c.from], &category.units[c.to]).unwrap_or_default();
    let mut recent = Dom::create_div()
        .with_id("conv-recent")
        .with_css("display: flex; flex-direction: column; padding-top: 12px; font-size: 13px;");
    if !c.recent.is_empty() {
        recent.add_child(
            Dom::create_div()
                .with_css("font-weight: 600; padding-bottom: 4px;")
                .with_child(Dom::create_span_with_text("Recent")),
        );
    }
    for r in &c.recent {
        recent.add_child(Dom::create_div().with_child(Dom::create_span_with_text(r.as_str())));
    }
    Dom::create_div()
        .with_id("convert-view")
        .with_css("display: flex; flex-direction: column; padding: 12px 16px; flex-grow: 1;")
        .with_child(labelled(
            "Category",
            DropDown::create(strs(&names))
                .with_selected(c.category)
                .with_accessibility_name("Category")
                .with_on_choice_change(app.clone(), on_convert_category as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id("conv-category"),
        ))
        .with_child(labelled(
            "From",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(field_input(app, Field::ConvertFrom, &from_text, "Value to convert", "conv-from-value"))
                .with_child(unit_drop(c.from, "conv-from-unit", "From unit", on_convert_from_unit)),
        ))
        .with_child(
            Dom::create_div().with_css("padding: 2px 0px 2px 110px;").with_child(
                Button::create("Swap")
                    .with_icon("swap_vert")
                    .with_on_click(app.clone(), on_convert_swap as ButtonOnClickCallbackType)
                    .dom()
                    .with_id("conv-swap"),
            ),
        )
        .with_child(labelled(
            "To",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(field_input(app, Field::ConvertTo, &to_text, "Converted value", "conv-to-value"))
                .with_child(unit_drop(c.to, "conv-to-unit", "To unit", on_convert_to_unit)),
        ))
        .with_child(
            Dom::create_div()
                .with_id("conv-rate")
                .with_css("padding: 8px 0px 0px 110px; font-size: 13px; opacity: 0.8;")
                .with_child(Dom::create_span_with_text(rate)),
        )
        .with_child(recent)
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

/// The mode switch and the History / Settings buttons.
fn modes_row(s: &CalcApp, app: &RefAny) -> Dom {
    let labels: Vec<&str> = Screen::ALL.iter().map(|m| m.label()).collect();
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 8px;")
        .with_child(
            Segmented::create(strs(&labels))
                .with_selected_index(s.screen.index())
                .with_on_change(app.clone(), on_screen as SegmentedOnChangeCallbackType)
                .dom()
                .with_id("calc-modes"),
        )
        .with_child(Dom::create_div().with_css("flex-grow: 1;"))
        .with_child(
            Button::create("History")
                .with_icon("history")
                .with_on_click(app.clone(), on_toggle_panel as ButtonOnClickCallbackType)
                .dom()
                .with_id("calc-toggle-panel"),
        )
        .with_child(
            Button::create("Settings")
                .with_icon("settings")
                .with_on_click(app.clone(), on_open_settings as ButtonOnClickCallbackType)
                .dom()
                .with_id("calc-settings"),
        )
}

/// The calculator's own settings sections.
fn settings_sections(s: &CalcApp, app: &RefAny) -> Vec<AppSection> {
    let mut k = s.kit.clone();
    let keep = k
        .downcast_ref::<kit::Kit>()
        .map_or(true, |k| k.settings.get_bool("history", true));
    let angles: Vec<&str> = AngleUnit::ALL.iter().map(|a| a.label()).collect();
    vec![
        AppSection {
            category: 0,
            title: "Display".to_string(),
            content: Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(kit::row(
                    "Group thousands",
                    Switch::create(s.calc.grouping)
                        .with_accessibility_name("Group thousands")
                        .with_on_toggle(app.clone(), on_grouping as SwitchOnToggleCallbackType)
                        .dom()
                        .with_id("set-grouping"),
                ))
                .with_child(kit::row(
                    "Angle unit",
                    Segmented::create(strs(&angles))
                        .with_selected_index(s.calc.angle.index())
                        .with_on_change(app.clone(), on_angle as SegmentedOnChangeCallbackType)
                        .dom()
                        .with_id("set-angle"),
                )),
        },
        AppSection {
            category: 0,
            title: "History".to_string(),
            content: Dom::create_div()
                .with_css("display: flex; flex-direction: column;")
                .with_child(kit::row(
                    "Keep the history",
                    Switch::create(keep)
                        .with_accessibility_name("Keep the history")
                        .with_on_toggle(app.clone(), on_keep_history as SwitchOnToggleCallbackType)
                        .dom()
                        .with_id("set-keep-history"),
                ))
                .with_child(kit::row(
                    "",
                    Button::create("Clear history")
                        .with_on_click(app.clone(), on_clear_history as ButtonOnClickCallbackType)
                        .dom()
                        .with_id("set-clear-history"),
                ))
                .with_child(kit::note(&format!(
                    "{} calculations, kept in {}.",
                    s.calc.history.len(),
                    azul_appkit::data::local_path(&s.data_root, &history_key()).display()
                ))),
        },
    ]
}

/// The window: the shell, the theme scope, the window-level key, copy and paste handlers.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    let wide = info.window_width_greater_than(PANEL_MIN_WIDTH);
    let app = data.clone();
    let Some(guard) = data.downcast_ref::<CalcApp>() else {
        return Dom::create_body();
    };
    let s = &*guard;
    let settings = kit::settings_open(&s.kit);
    let content = if settings {
        kit::settings_page(&s.kit, settings_sections(s, &app))
    } else {
        match s.screen {
            Screen::Date => date_view(s, &app),
            Screen::Convert => convert_view(s, &app),
            _ => calculator_view(s, &app, wide),
        }
    };
    let mut shell = UtilityShell::create(content)
        .with_title_row(kit::title_row(SPEC.name))
        .with_label(SPEC.name)
        .with_min_size(320.0, 480.0);
    if !settings {
        shell = shell.with_modes(modes_row(s, &app));
    }
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell.dom());
    Dom::create_body()
        .with_css("display: flex; flex-direction: column;")
        .with_child(ShellThemeScope::create(column).with_accent(ShellThemeAccent::Slate).dom())
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app.clone(), on_key)
        .with_callback(EventFilter::Focus(FocusEventFilter::Paste), app.clone(), on_paste)
        .with_callback(EventFilter::Focus(FocusEventFilter::Copy), app, on_copy)
}
