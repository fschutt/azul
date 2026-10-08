//! AzCalculator's window: azul's S9 `UtilityShell` with the app-drawn title
//! row (`NoTitle` + `Titlebar`), a View menu, and a calculator that changes
//! its layout with the window:
//!
//! ```text
//!   small (the default, < 640 px wide)   wider (>= 640)          large (>= 900 x 700)
//!   +-------------+                      +---------+---------+   +------------------+------+
//!   |     display |                      | display           |   | typeset  display | tape |
//!   | MC MR M+ .. |                      +---------+---------+   | DEG RAD GRAD     |      |
//!   | %  CE C  ⌫ |                      | HEX ... | MC MR   |   | 9 x 5 scientific |      |
//!   | 7  8  9  × |                      | DEC ... | %  CE C |   +----------+-------+------+
//!   | ...         |                      | bits    | 7 8 9 × |   | y₁ = ... |  plot       |
//!   | ±  0  .  = |                      | AND OR  | ...     |   | y₂ = ... |  (graph)    |
//!   +-------------+                      +---------+---------+   +----------+-------------+
//!     Standard                             Programmer                Graphing
//! ```
//!
//! The layout callback picks the view from the window's size (recorded
//! size queries: a resize across a breakpoint rebuilds the window) unless
//! the View menu pins one; the calculator's state - the entry, the memory,
//! the word size, the graph's functions and viewport - lives in the app and
//! survives every switch (between decimals and Programmer's integers the
//! entry's numbers are rewritten, `Calculator::set_mode`). Panels that
//! appear slide in (`-azul-animation-in`). Date and Convert are the View
//! menu's other two screens.
//!
//! KEYBOARD. The characters a key TYPES come through the text input: the
//! calculator's surface holds the focus (it takes it when it mounts, and
//! back after a click on a key), so a keystroke's text is recorded against
//! it and arrives at the window's `TextInput` handler as the character the
//! user's layout produced - `*` is Shift+8 on a US keyboard but Shift++ on a
//! German one, and the key code says neither. `Calculator::type_char` reads
//! it: digits, `. ,`, `+ - * / ^ % ! ( ) =`, names letter by letter (`sqrt`,
//! `sin`, `pi`, `x`; `and`, `xor`, `0x` in Programmer mode). The key-down
//! handler keeps the keys that type nothing: Enter (=), Backspace, Escape
//! (C), Delete (CE), F3-F9, the chords (Ctrl/Cmd+C copies, Ctrl/Cmd+V pastes
//! through the engine's Paste event, Ctrl+M/R/P/Q/L the memory, Ctrl+H the
//! history, Alt+0..5 the View menu). The window's body is focusable too, so
//! a click anywhere keeps the keyboard in the window; only when NOTHING holds
//! it (no text could be recorded) does a key type without its text, and then
//! only a keypad key, whose character no layout changes. A focused
//! calculator waits for the text, which Windows sends in a pass of its own
//! after the key (`WM_CHAR`).
//!
//! The history lives in `calculator/history.jsonl` in the user's data
//! folder, read when the window opens and written after every calculation,
//! always on an azul Thread through azul-storage (appkit::ui::spawn_file_jobs).
//!
//! On stdout, for scripts/azcalculator_e2e.py: `AZCALC_MODE <micro |
//! programmer | graph | date | convert>` when the view changes,
//! `AZCALC_SCREEN <pin>` when the View menu changes, `AZCALC_DISPLAY
//! <expression line>\t<result line>` after every key, `AZCALC_PLOT <n> | y1 =
//! ...` when the graph's functions change, `AZCALC_GRAPH x .. y ..` when its
//! viewport does, `AZCALC_COPIED <text>`, `AZCALC_PASTED <text>`,
//! `AZCALC_HISTORY_LOADED <n>`, `AZCALC_HISTORY_SAVED <n>`, `AZCALC_CONVERT
//! <line>`, `AZCALC_DATE <line>`.

use std::path::PathBuf;

use azul::{
    callbacks::{
        ButtonOnClickCallbackType, DropDownOnChoiceChangeCallbackType, SegmentedOnChangeCallbackType,
        SwitchOnToggleCallbackType, TabOnClickCallbackType, TextInputOnTextInputCallbackType,
    },
    css::DarkLightMode,
    dom::{AccessibilityInfo, AccessibilityRole, ClipboardContent, DomId, FocusTarget, TabIndex, VirtualKeyCode},
    option::OptionString,
    prelude::*,
    shells::{ShellThemeAccent, ShellThemeScope, UtilityShell},
    str::String as AzString,
    svg::{CssPath, CssPathSelector},
    vec::{StringVec, StyledTextRunVec},
    widgets::{
        DropDown, OnTextInputReturn, Segmented, SegmentedState, Switch, SwitchState, TabHeader, TabHeaderState,
        TextInputState, TextInputValid,
    },
};
use azul_appkit::{
    about::AboutInfo,
    args::{AppArgs, AppSpec},
    files::{FileJob, FileOutcome},
    settings::AppSettings,
    shortcuts::Shortcut,
    ui::{self as kit, AppSection},
};

use crate::calc::{named_command, CalcMode, Calculator, Cmd, NamedKey};
use crate::datecalc::{self, Date};
use crate::expr::AngleUnit;
use crate::graphview::{self, GraphState};
use crate::history::{self, HistoryEntry};
use crate::ids;
use crate::keypad::{self, Action, KeyHooks, KeyRef};
use crate::look::{self, Look};
use crate::mathview::{self, M};
use crate::num::Num;
use crate::programmer::{self, Base, WordSize};
use crate::units::{self, CATEGORIES};

// ==== The app's facts ====

/// The screens `--screen` opens: the View menu's choices (`scientific` is
/// the graphing view's old name), and the settings page.
pub const SCREENS: [&str; 8] = [
    "auto",
    "standard",
    "programmer",
    "graphing",
    "scientific",
    "date",
    "convert",
    "settings",
];

pub const SPEC: AppSpec = AppSpec {
    name: "AzCalculator",
    binary: "AzCalculator",
    summary: "a calculator that grows with its window: standard, programmer, graphing; dates, units",
    screens: &SCREENS,
    files_help: "",
};

pub const ABOUT: AboutInfo = AboutInfo {
    name: "AzCalculator",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Standard, programmer and graphing calculations with exact decimals - the window \
              picks the calculator by its size - date calculations and a unit converter. Part of \
              the Azlin apps, built with azul.",
    license: "MIT",
    app_folder: "calculator",
};

/// The keyboard shortcuts the settings page lists.
pub const SHORTCUTS: [Shortcut; 19] = [
    Shortcut::new("Calculator", "Enter", "Evaluate (=); a function of x goes to the graph"),
    Shortcut::new("Calculator", "Escape", "Clear (C)"),
    Shortcut::new("Calculator", "Delete", "Clear the entry (CE)"),
    Shortcut::new("Calculator", "Backspace", "Delete the last digit or letter"),
    Shortcut::new("Calculator", "F9", "Change the sign (+/-)"),
    Shortcut::new("Calculator", "Mod+C", "Copy the result"),
    Shortcut::new("Calculator", "Mod+V", "Paste a number or an expression"),
    Shortcut::new("Typing", "+ - * / ^ % ! ( ) =", "Operators, as your keyboard types them"),
    Shortcut::new("Typing", "sqrt sin cos tan ln log abs", "Functions: type the name"),
    Shortcut::new("Typing", "pi  e  x", "Constants, and the graph's variable"),
    Shortcut::new("Typing", "y =", "Start a function for the graph"),
    Shortcut::new("Memory", "Ctrl+M  Ctrl+R", "Memory store, recall"),
    Shortcut::new("Memory", "Ctrl+P  Ctrl+Q", "Memory add, subtract"),
    Shortcut::new("Memory", "Ctrl+L", "Memory clear"),
    Shortcut::new("View", "Alt+0 .. Alt+5", "Automatic, Standard, Programmer, Graphing, Date, Convert"),
    Shortcut::new("View", "Ctrl+H", "Show or hide the history"),
    Shortcut::new("Graphing", "F3 F4 F5", "Degrees, radians, grads"),
    Shortcut::new("Programmer", "& | ^ ~ < > %  and xor not", "AND, OR, XOR, NOT, shifts, mod"),
    Shortcut::new("Programmer", "F5 F6 F7 F8  0x 0b 0o", "HEX, DEC, OCT, BIN"),
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

/// The window's default size: the small calculator.
const DEFAULT_SIZE: (f32, f32) = (340.0, 560.0);
/// The smallest window the small calculator still fits.
const MIN_SIZE: (f32, f32) = (300.0, 460.0);

/// From this width on the window is the programmer calculator.
pub const PROGRAMMER_MIN_WIDTH: f32 = 640.0;
/// From this width AND [`GRAPH_MIN_HEIGHT`] on it is the graphing one.
pub const GRAPH_MIN_WIDTH: f32 = 900.0;
pub const GRAPH_MIN_HEIGHT: f32 = 700.0;

// ==== Views ====

/// What the window shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    /// The small standard calculator.
    Micro,
    Programmer,
    /// Scientific keys over the graph.
    Graph,
    Date,
    Convert,
}

impl View {
    /// The name on stdout (`AZCALC_MODE micro`).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            View::Micro => "micro",
            View::Programmer => "programmer",
            View::Graph => "graph",
            View::Date => "date",
            View::Convert => "convert",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            View::Micro => "Standard",
            View::Programmer => "Programmer",
            View::Graph => "Graphing",
            View::Date => "Date",
            View::Convert => "Convert",
        }
    }

    /// The keypad mode of a calculator view.
    #[must_use]
    pub fn calc_mode(self) -> Option<CalcMode> {
        match self {
            View::Micro => Some(CalcMode::Standard),
            View::Programmer => Some(CalcMode::Programmer),
            View::Graph => Some(CalcMode::Scientific),
            View::Date | View::Convert => None,
        }
    }
}

/// The view a window of `width` x `height` shows by itself.
#[must_use]
pub fn view_for_size(width: f32, height: f32) -> View {
    if width >= GRAPH_MIN_WIDTH && height >= GRAPH_MIN_HEIGHT {
        View::Graph
    } else if width >= PROGRAMMER_MIN_WIDTH {
        View::Programmer
    } else {
        View::Micro
    }
}

/// [`view_for_size`] through the layout callback's RECORDED size queries:
/// a resize that flips one of them rebuilds the window (all three are
/// asked every time, so all three are recorded).
fn view_of_window(info: &LayoutCallbackInfo) -> View {
    let narrow = info.window_width_less_than(PROGRAMMER_MIN_WIDTH);
    let below_graph = info.window_width_less_than(GRAPH_MIN_WIDTH);
    let short = info.window_height_less_than(GRAPH_MIN_HEIGHT);
    if !below_graph && !short {
        View::Graph
    } else if !narrow {
        View::Programmer
    } else {
        View::Micro
    }
}

/// The View menu: by the window's size, or one view pinned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pin {
    Auto,
    Standard,
    Programmer,
    Graphing,
    Date,
    Convert,
}

impl Pin {
    pub const ALL: [Pin; 6] = [Pin::Auto, Pin::Standard, Pin::Programmer, Pin::Graphing, Pin::Date, Pin::Convert];

    /// The name in the settings file and of `--screen`.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Pin::Auto => "auto",
            Pin::Standard => "standard",
            Pin::Programmer => "programmer",
            Pin::Graphing => "graphing",
            Pin::Date => "date",
            Pin::Convert => "convert",
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Pin::Auto => "Automatic",
            Pin::Standard => "Standard",
            Pin::Programmer => "Programmer",
            Pin::Graphing => "Graphing",
            Pin::Date => "Date",
            Pin::Convert => "Convert",
        }
    }

    #[must_use]
    pub fn by_key(key: &str) -> Option<Pin> {
        if key == "scientific" {
            return Some(Pin::Graphing);
        }
        Pin::ALL.into_iter().find(|p| p.key() == key)
    }

    #[must_use]
    pub fn index(self) -> usize {
        Pin::ALL.iter().position(|p| *p == self).unwrap_or(0)
    }

    /// The pinned view (`None`: the window's size decides).
    #[must_use]
    pub fn view(self) -> Option<View> {
        match self {
            Pin::Auto => None,
            Pin::Standard => Some(View::Micro),
            Pin::Programmer => Some(View::Programmer),
            Pin::Graphing => Some(View::Graph),
            Pin::Date => Some(View::Date),
            Pin::Convert => Some(View::Convert),
        }
    }
}

// ==== State ====

/// The panel of the history tape.
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
    /// The View menu's choice.
    pub pin: Pin,
    /// The view the last layout built (`None` before the first).
    pub view: Option<View>,
    pub panel: Panel,
    /// The small and the programmer views: the history over the keys
    /// (Ctrl+H / the History button). The graphing view always shows it.
    pub panel_shown: bool,
    pub convert: ConvertState,
    pub date: DateState,
    pub graph: GraphState,
    /// The graph's functions as last announced on stdout.
    pub plots_seen: Vec<String>,
    /// A character a key typed by its US position because nothing held the
    /// keyboard: on Windows its text still follows (`WM_CHAR`, a pass after
    /// the key), and must not type it twice.
    pub fallback: Option<char>,
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
        calc.angle = angle_of(angle.as_deref());
        let pin = args
            .screen
            .as_deref()
            .filter(|s| *s != "settings")
            .and_then(Pin::by_key)
            .or_else(|| last_screen.as_deref().and_then(Pin::by_key))
            .unwrap_or(Pin::Auto);
        if let Some(mode) = pin.view().and_then(View::calc_mode) {
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
            pin,
            view: None,
            panel: Panel::History,
            panel_shown: false,
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
            graph: GraphState::default(),
            plots_seen: Vec::new(),
            fallback: None,
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

    /// Prints the graph's functions for scripts, when they changed.
    pub(crate) fn announce_plots(&mut self) {
        let texts: Vec<String> = self
            .calc
            .plots
            .iter()
            .enumerate()
            .map(|(i, p)| format!("y{} = {}", i + 1, p.text))
            .collect();
        if texts == self.plots_seen {
            return;
        }
        let list: String = texts.iter().map(|t| format!(" | {t}")).collect();
        println!("AZCALC_PLOT {}{list}", texts.len());
        self.plots_seen = texts;
    }

    /// The layout built `view`: the calculator follows it into its keypad
    /// mode (the entry carried over), and scripts hear of it.
    fn enter_view(&mut self, view: View) {
        if self.view == Some(view) {
            return;
        }
        if let Some(mode) = view.calc_mode() {
            self.calc.set_mode(mode);
        }
        self.view = Some(view);
        self.graph.drag = None;
        println!("AZCALC_MODE {}", view.key());
        self.announce_display();
    }

    /// The view callbacks act on: the last one built, else the window's.
    fn current_view(&self, info: &CallbackInfo) -> View {
        self.view.unwrap_or_else(|| {
            let size = info.get_current_window_state().size.dimensions;
            self.pin.view().unwrap_or_else(|| view_for_size(size.width, size.height))
        })
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
    println!("AZCALC_SCREEN {}", app.pin.key());
    let config = kit::app_config(&kit_ref);
    let window = kit::window_options(&kit_ref, layout, DEFAULT_SIZE, MIN_SIZE, on_window_created);
    App::create(RefAny::new(app), config).run(window);
}

// ==== The display ====

/// The result line's size: as big as the view allows, smaller for long numbers.
fn result_px(text: &str, view: View) -> usize {
    let base: usize = match view {
        View::Programmer => 32,
        View::Graph => 40,
        _ => 38,
    };
    let n = text.chars().count().max(1);
    if n <= 11 {
        base
    } else {
        (base * 11 / n).max(14)
    }
}

/// The last `max` characters of `text`, an ellipsis before them: a long
/// expression shows where it is being typed (an overflowing line is cut at
/// its END, right-aligned or not).
fn tail(text: &str, max: usize) -> String {
    let n = text.chars().count();
    if n <= max || max < 2 {
        return text.to_string();
    }
    let kept: String = text.chars().skip(n - (max - 1)).collect();
    format!("\u{2026}{kept}")
}

/// The expression typeset (the graphing view): the evaluated expression and
/// `=` after it, `y =` before a function, the entry with its holes.
fn typeset(c: &Calculator) -> Option<M> {
    let y_equals = |m: M| M::Row(vec![M::Var("y".to_string()), M::Op("=".to_string()), m]);
    if c.just_evaluated {
        if c.shown_tokens.is_empty() {
            return None;
        }
        let m = mathview::entry(&c.shown_tokens, "", c.grouping)?;
        return Some(if c.plotted.is_some() {
            y_equals(m)
        } else {
            M::Row(vec![m, M::Op("=".to_string())])
        });
    }
    if c.tokens.is_empty() && c.letters.is_empty() && !c.defining {
        return None;
    }
    let m = mathview::entry(&c.tokens, &c.letters, c.grouping)?;
    Some(if c.defining { y_equals(m) } else { m })
}

fn display(s: &CalcApp, view: View) -> Dom {
    let c = &s.calc;
    let result = c.result_line();
    let mut column = Dom::create_div()
        .with_id(ids::DISPLAY)
        .with_css(look::DISPLAY)
        .with_accessibility_info(AccessibilityInfo::named(
            format!("{} {}", c.expression_line(), result),
            AccessibilityRole::StatusBar,
        ));
    if view == View::Graph {
        let math = match typeset(c) {
            Some(m) => mathview::to_dom(&m),
            None => Dom::create_div(),
        };
        column.add_child(Dom::create_div().with_id(ids::MATH).with_css(look::MATH).with_child(math));
    } else {
        let mut expression = c.expression_line();
        let open = c.open_parens();
        if open > 0 && !c.just_evaluated {
            expression.push_str(&format!("   ({open} open)"));
        }
        let fits = if view == View::Programmer { 64 } else { 40 };
        column.add_child(
            Dom::create_div_with_text(AzString::from(tail(&expression, fits)))
                .with_id(ids::EXPRESSION)
                .with_css(look::EXPR_LINE),
        );
    }
    let px = result_px(&result, view);
    column.add_child(
        Dom::create_div_with_text(AzString::from(result))
            .with_id(ids::RESULT)
            .with_css(format!(
                "{} font-size: {px}px; min-height: {}px; line-height: 1.2;",
                look::RESULT_LINE,
                px + 10
            )),
    );
    let note = if s.notice.is_empty() {
        c.hint.clone().unwrap_or_default()
    } else {
        s.notice.clone()
    };
    if !note.is_empty() {
        column.add_child(
            Dom::create_div_with_text(AzString::from(note))
                .with_id(ids::NOTICE)
                .with_css(look::NOTICE),
        );
    }
    column
}

// ==== The programmer panel ====

struct BaseRef {
    app: RefAny,
    base: Base,
}

struct BitRef {
    app: RefAny,
    bit: u32,
}

/// HEX / DEC / OCT / BIN (click one to type in it), the word size, the bits,
/// the bitwise keys and the hex digits.
fn programmer_panel(s: &CalcApp, app: &RefAny) -> Dom {
    let words: Vec<&str> = WordSize::ALL.iter().map(|w| w.label()).collect();
    let title = Dom::create_div()
        .with_css(look::PANEL_TITLE)
        .with_child(Dom::create_div_with_text("Programmer").with_css("flex-grow: 1;"))
        .with_child(
            DropDown::create(strs(&words))
                .with_selected(s.calc.word.index())
                .with_accessibility_name("Word size")
                .with_on_choice_change(app.clone(), on_word as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id(ids::WORD)
                .with_css("text-transform: none; letter-spacing: 0px;"),
        );
    let mut rows = Dom::create_div()
        .with_id(ids::BASES)
        .with_css("display: flex; flex-direction: column; padding: 2px 0px 4px 0px; flex-shrink: 0;");
    for (base, text) in s.calc.programmer_lines() {
        let selected = base == s.calc.base;
        rows.add_child(
            Dom::create_div()
                .with_id(ids::named(&format!("base-{}", base.label().to_lowercase())))
                .with_css(format!(
                    "{} {}",
                    look::BASE_ROW,
                    if selected { look::BASE_ROW_SELECTED } else { "" }
                ))
                .with_accessibility_info(AccessibilityInfo::named(
                    format!("{} {}", base.label(), text),
                    AccessibilityRole::PushButton,
                ))
                .with_tab_index(TabIndex::Auto)
                .with_child(Dom::create_div_with_text(base.label()).with_css("width: 40px; flex-shrink: 0;"))
                .with_child(
                    Dom::create_div_with_text(AzString::from(text))
                        .with_css("flex-grow: 1; min-width: 0px; overflow: hidden;"),
                )
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(BaseRef {
                        app: app.clone(),
                        base,
                    }),
                    on_base,
                ),
        );
    }
    Dom::create_div()
        .with_id(ids::PROGRAMMER)
        .with_css(format!(
            "{} {} flex-grow: 1; flex-basis: 0px; margin-right: 8px;",
            look::PANEL,
            look::ENTER_SIDE
        ))
        .with_child(title)
        .with_child(rows)
        .with_child(bit_field(s, app))
        .with_child(keypad::grid(
            app,
            &keypad::programmer_keys(),
            6,
            Some(s.calc.base),
            &[],
            ids::PROGPAD,
            "padding: 6px 8px 8px 8px; flex-grow: 1; min-height: 100px;",
            &hooks(),
        ))
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
        .with_id(ids::BITS)
        .with_css("display: flex; flex-direction: column; padding: 2px 10px 4px 10px; flex-shrink: 0;");
    for row in 0..4u32 {
        let top = 63 - row * 16;
        let mut line = Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center; height: 17px;")
            .with_child(Dom::create_div_with_text(AzString::from(top.to_string())).with_css(look::BIT_LABEL));
        for i in 0..16u32 {
            let bit = top - i;
            let set = programmer::bit(value, bit, word);
            let inside = bit < word.bits();
            let mut cell = Dom::create_div_with_text(if set { "1" } else { "0" })
                .with_id(ids::named(&format!("bit-{bit}")))
                .with_css(format!(
                    "{} {} {} {}",
                    look::BIT,
                    if set { look::BIT_ON } else { "" },
                    if inside { "" } else { look::BIT_OUT },
                    if i % 4 == 3 && i < 15 { "margin-right: 7px;" } else { "" }
                ));
            if inside {
                cell = cell.with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
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

/// Graphing: the angle unit over the scientific keys.
fn angle_row(s: &CalcApp, app: &RefAny) -> Dom {
    let labels: Vec<&str> = AngleUnit::ALL.iter().map(|a| a.label()).collect();
    Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 0px 0px 6px 0px; flex-shrink: 0;")
        .with_child(
            Segmented::create(strs(&labels))
                .with_selected_index(s.calc.angle.index())
                .with_on_change(app.clone(), on_angle as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::ANGLE),
        )
}

// ==== The history tape and the memory ====

struct HistoryRef {
    app: RefAny,
    index: usize,
}

fn history_list(s: &CalcApp, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::HISTORY)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;");
    if s.calc.history.is_empty() {
        list.add_child(Dom::create_div_with_text("There's no history yet.").with_css(look::EMPTY_NOTE));
    }
    for (index, e) in s.calc.history.iter().enumerate().rev() {
        list.add_child(
            Dom::create_div()
                .with_class(ids::HISTORY_ENTRY)
                .with_css(look::TAPE_ENTRY)
                .with_accessibility_info(AccessibilityInfo::named(
                    format!("{} = {}", e.expr, e.result),
                    AccessibilityRole::ListItem,
                ))
                .with_child(Dom::create_div_with_text(AzString::from(format!("{} =", e.expr))).with_css(look::TAPE_EXPR))
                .with_child(Dom::create_div_with_text(AzString::from(e.result.as_str())).with_css(look::TAPE_RESULT))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
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
                .with_css("display: flex; flex-direction: row; justify-content: flex-end; padding: 6px 8px; flex-shrink: 0;")
                .with_child(
                    Button::create("Clear history")
                        .with_icon("delete")
                        .with_on_click(app.clone(), on_clear_history as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::CLEAR_HISTORY),
                ),
        );
    }
    column
}

fn memory_list(s: &CalcApp, app: &RefAny) -> Dom {
    let mut list = Dom::create_div()
        .with_id(ids::MEMORY)
        .with_css("display: flex; flex-direction: column; flex-grow: 1; overflow-y: auto; min-height: 0px;");
    if s.calc.memory.items.is_empty() {
        list.add_child(Dom::create_div_with_text("There's nothing saved in memory.").with_css(look::EMPTY_NOTE));
    }
    for (index, item) in s.calc.memory.items.iter().enumerate() {
        let shown = Num::parse(item)
            .map(|n| n.format(&crate::num::Format { grouping: s.calc.grouping, ..crate::num::Format::default() }))
            .unwrap_or_else(|_| item.clone());
        list.add_child(
            Dom::create_div()
                .with_class(ids::MEMORY_ENTRY)
                .with_css(look::TAPE_ENTRY)
                .with_child(Dom::create_div_with_text(AzString::from(shown)).with_css(look::TAPE_RESULT))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::Click),
                    RefAny::new(HistoryRef { app: app.clone(), index }),
                    on_memory_entry,
                ),
        );
    }
    list
}

/// The history tape (and the memory, its second tab).
fn side_panel(s: &CalcApp, app: &RefAny, css: &str) -> Dom {
    let active = match s.panel {
        Panel::History => 0,
        Panel::Memory => 1,
    };
    let body = match s.panel {
        Panel::History => history_list(s, app),
        Panel::Memory => memory_list(s, app),
    };
    Dom::create_aside()
        .with_id(ids::PANEL)
        .with_accessibility_name("History and memory")
        .with_css(format!("{} {css}", look::PANEL))
        .with_child(
            TabHeader::create(strs(&["History", "Memory"]))
                .with_active_tab(active)
                .with_on_click(app.clone(), on_panel_tab as TabOnClickCallbackType)
                .dom()
                .with_id(ids::PANEL_TABS),
        )
        .with_child(body)
}

// ==== The calculator views ====

/// The callbacks every key carries.
fn hooks() -> KeyHooks {
    KeyHooks {
        click: on_key_click,
        paste: on_paste,
        copy: on_copy,
    }
}

/// The calculator's surface: it holds the keyboard focus, so a keystroke's
/// TEXT arrives (recorded against the focused node, heard by the window's
/// `TextInput` handler).
fn surface(app: &RefAny) -> Dom {
    Dom::create_div()
        .with_id(ids::CALC)
        .with_css(look::SURFACE)
        .with_tab_index(TabIndex::NoKeyboardFocus)
        .with_accessibility_info(AccessibilityInfo::named("Calculator", AccessibilityRole::Grouping))
        .with_callback(EventFilter::Component(ComponentEventFilter::AfterMount), app.clone(), on_surface_mounted)
        .with_callback(EventFilter::Focus(FocusEventFilter::Paste), app.clone(), on_paste)
        .with_callback(EventFilter::Focus(FocusEventFilter::Copy), app.clone(), on_copy)
}

/// The memory row over the standard keys (`programmer`: its variant, the
/// digits the base does not take dimmed).
fn standard_pad(s: &CalcApp, app: &RefAny, programmer: bool) -> Dom {
    let base = programmer.then_some(s.calc.base);
    Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; flex-basis: 0px; min-height: 0px; min-width: 0px;")
        .with_child(keypad::grid(
            app,
            &keypad::memory_keys(),
            5,
            None,
            &[],
            ids::MEMORY_ROW,
            "height: 28px; flex-shrink: 0; margin-bottom: 5px;",
            &hooks(),
        ))
        .with_child(keypad::grid(
            app,
            &keypad::standard_keys(programmer),
            4,
            base,
            &[],
            ids::KEYPAD,
            "flex-grow: 1;",
            &hooks(),
        ))
}

/// The small window: the display over the standard keys (or the history,
/// Ctrl+H).
fn micro_view(s: &CalcApp, app: &RefAny) -> Dom {
    let below = if s.panel_shown {
        side_panel(s, app, &format!("flex-grow: 1; {}", look::ENTER_FADE))
    } else {
        standard_pad(s, app, false)
    };
    surface(app).with_child(display(s, View::Micro)).with_child(below)
}

/// Wider: the programmer panel beside the standard keys.
fn programmer_view(s: &CalcApp, app: &RefAny) -> Dom {
    let right = if s.panel_shown {
        side_panel(s, app, &format!("flex-grow: 1; flex-basis: 0px; {}", look::ENTER_FADE))
    } else {
        standard_pad(s, app, true)
    };
    surface(app).with_child(display(s, View::Programmer)).with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; flex-grow: 1; min-height: 0px;")
            .with_child(programmer_panel(s, app))
            .with_child(right),
    )
}

/// Large: the scientific keys and the history tape over the graph.
fn graphing_view(s: &CalcApp, app: &RefAny, look: Look) -> Dom {
    let mut on: Vec<&str> = Vec::new();
    if s.calc.second {
        on.push("key-second");
    }
    if s.calc.fe {
        on.push("key-fe");
    }
    let keys = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-width: 0px; margin-right: 8px;")
        .with_child(display(s, View::Graph))
        .with_child(angle_row(s, app))
        .with_child(keypad::grid(
            app,
            &keypad::scientific_keys(s.calc.second),
            9,
            None,
            &on,
            ids::KEYPAD,
            "height: 216px; flex-shrink: 0;",
            &hooks(),
        ));
    surface(app)
        .with_child(
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; flex-shrink: 0;")
                .with_child(keys)
                // The tape fills the height the keys give the row and scrolls in it,
                // out of the flow: a row is as tall as its tallest item, and a long
                // tape made the row taller than the window and pushed the graph out.
                .with_child(
                    Dom::create_div()
                        .with_css("position: relative; width: 290px; flex-shrink: 0;")
                        .with_child(side_panel(
                            s,
                            app,
                            &format!(
                                "position: absolute; top: 0px; right: 0px; bottom: 0px; \
                                 left: 0px; {}",
                                look::ENTER_SIDE
                            ),
                        )),
                ),
        )
        .with_child(graphview::panel(s, app, look))
}

fn calculator(s: &CalcApp, app: &RefAny, view: View, look: Look) -> Dom {
    match view {
        View::Programmer => programmer_view(s, app),
        View::Graph => graphing_view(s, app, look),
        _ => micro_view(s, app),
    }
}

// ==== Date and Convert ====

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

fn field_input(app: &RefAny, field: Field, text: &str, name: &str, id: AzString) -> Dom {
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
        .with_id(ids::DATE_VIEW)
        .with_css("display: flex; flex-direction: column; padding: 12px 16px; flex-grow: 1;")
        .with_child(
            Segmented::create(strs(&["Difference between dates", "Add or subtract days"]))
                .with_selected_index(d.kind)
                .with_on_change(app.clone(), on_date_kind as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::DATE_KIND),
        )
        .with_child(labelled("From", field_input(app, Field::DateFrom, &d.from, "From date", ids::DATE_FROM)));
    if d.kind == 0 {
        column.add_child(labelled("To", field_input(app, Field::DateTo, &d.to, "To date", ids::DATE_TO)));
    } else {
        column.add_child(labelled(
            "",
            Segmented::create(strs(&["Add", "Subtract"]))
                .with_selected_index(usize::from(d.subtract))
                .with_on_change(app.clone(), on_date_sign as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::DATE_SIGN),
        ));
        column.add_child(labelled("Years", field_input(app, Field::Years, &d.years, "Years", ids::DATE_YEARS)));
        column.add_child(labelled("Months", field_input(app, Field::Months, &d.months, "Months", ids::DATE_MONTHS)));
        column.add_child(labelled("Days", field_input(app, Field::Days, &d.days, "Days", ids::DATE_DAYS)));
    }
    column.add_child(
        Dom::create_div().with_css("padding: 4px 0px 8px 110px;").with_child(
            Button::create("Today")
                .with_icon("today")
                .with_on_click(app.clone(), on_date_today as ButtonOnClickCallbackType)
                .dom()
                .with_id(ids::DATE_TODAY),
        ),
    );
    let mut result = Dom::create_div()
        .with_id(ids::DATE_RESULT)
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
    let unit_drop = |selected: usize, id: AzString, name: &str, cb: DropDownOnChoiceChangeCallbackType| {
        DropDown::create(strs(&unit_strs))
            .with_selected(selected)
            .with_accessibility_name(name)
            .with_on_choice_change(app.clone(), cb)
            .dom()
            .with_id(id)
    };
    let rate = units::rate_line(&category.units[c.from], &category.units[c.to]).unwrap_or_default();
    let mut recent = Dom::create_div()
        .with_id(ids::CONV_RECENT)
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
        .with_id(ids::CONVERT_VIEW)
        .with_css("display: flex; flex-direction: column; padding: 12px 16px; flex-grow: 1;")
        .with_child(labelled(
            "Category",
            DropDown::create(strs(&names))
                .with_selected(c.category)
                .with_accessibility_name("Category")
                .with_on_choice_change(app.clone(), on_convert_category as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id(ids::CONV_CATEGORY),
        ))
        .with_child(labelled(
            "From",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(field_input(app, Field::ConvertFrom, &from_text, "Value to convert", ids::CONV_FROM_VALUE))
                .with_child(unit_drop(c.from, ids::CONV_FROM_UNIT, "From unit", on_convert_from_unit)),
        ))
        .with_child(
            Dom::create_div().with_css("padding: 2px 0px 2px 110px;").with_child(
                Button::create("Swap")
                    .with_icon("swap_vert")
                    .with_on_click(app.clone(), on_convert_swap as ButtonOnClickCallbackType)
                    .dom()
                    .with_id(ids::CONV_SWAP),
            ),
        )
        .with_child(labelled(
            "To",
            Dom::create_div()
                .with_css("display: flex; flex-direction: row; align-items: center;")
                .with_child(field_input(app, Field::ConvertTo, &to_text, "Converted value", ids::CONV_TO_VALUE))
                .with_child(unit_drop(c.to, ids::CONV_TO_UNIT, "To unit", on_convert_to_unit)),
        ))
        .with_child(
            Dom::create_div()
                .with_id(ids::CONV_RATE)
                .with_css("padding: 8px 0px 0px 110px; font-size: 13px; opacity: 0.8;")
                .with_child(Dom::create_span_with_text(rate)),
        )
        .with_child(recent)
}

fn strs(items: &[&str]) -> StringVec {
    StringVec::from_vec(items.iter().map(|s| AzString::from(*s)).collect())
}

/// An icon-only command of the mode row, named for a screen reader.
fn icon_button(app: &RefAny, icon: &str, name: &str, id: AzString, cb: ButtonOnClickCallbackType) -> Dom {
    Button::create("")
        .with_icon(icon)
        .with_on_click(app.clone(), cb)
        .dom()
        .with_id(id)
        .with_accessibility_assign(AccessibilityInfo::named(name, AccessibilityRole::PushButton))
}

/// The View menu, the name of the view the size picked, History, Settings.
fn modes_row(s: &CalcApp, app: &RefAny, view: View) -> Dom {
    let labels: Vec<&str> = Pin::ALL.iter().map(|p| p.label()).collect();
    let mut row = Dom::create_div()
        .with_css("display: flex; flex-direction: row; align-items: center; padding: 4px 8px; min-width: 0px;")
        .with_child(
            DropDown::create(strs(&labels))
                .with_selected(s.pin.index())
                .with_accessibility_name("View")
                .with_on_choice_change(app.clone(), on_view_pick as DropDownOnChoiceChangeCallbackType)
                .dom()
                .with_id(ids::VIEW),
        );
    if s.pin == Pin::Auto {
        row.add_child(Dom::create_div_with_text(view.label()).with_css(look::VIEW_BADGE));
    }
    row.add_child(Dom::create_div().with_css("flex-grow: 1;"));
    if matches!(view, View::Micro | View::Programmer) {
        row.add_child(icon_button(app, "history", "History", ids::TOGGLE_PANEL, on_toggle_panel));
    }
    row.with_child(icon_button(app, "settings", "Settings", ids::SETTINGS, on_open_settings))
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
                        .with_id(ids::SET_GROUPING),
                ))
                .with_child(kit::row(
                    "Angle unit",
                    Segmented::create(strs(&angles))
                        .with_selected_index(s.calc.angle.index())
                        .with_on_change(app.clone(), on_angle as SegmentedOnChangeCallbackType)
                        .dom()
                        .with_id(ids::SET_ANGLE),
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
                        .with_id(ids::SET_KEEP_HISTORY),
                ))
                .with_child(kit::row(
                    "",
                    Button::create("Clear history")
                        .with_on_click(app.clone(), on_clear_history as ButtonOnClickCallbackType)
                        .dom()
                        .with_id(ids::SET_CLEAR_HISTORY),
                ))
                .with_child(kit::note(&format!(
                    "{} calculations, kept in {}.",
                    s.calc.history.len(),
                    azul_appkit::data::local_path(&s.data_root, &history_key()).display()
                ))),
        },
    ]
}

/// The window: the shell, the theme scope, the window-level key, text,
/// copy and paste handlers. The view comes from the window's size (or the
/// View menu); the calculator follows it into its keypad mode here.
extern "C" fn layout(mut data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let dark = matches!(info.get_mode(), DarkLightMode::Dark);
    let flora = info.get_theme().as_str().starts_with("flora");
    let look = Look { flora, dark };
    let by_size = view_of_window(&info);
    let app = data.clone();
    let Some(mut guard) = data.downcast_mut::<CalcApp>() else {
        return Dom::create_body();
    };
    let s = &mut *guard;
    let view = s.pin.view().unwrap_or(by_size);
    s.enter_view(view);
    let settings = kit::settings_open(&s.kit);
    let content = if settings {
        kit::settings_page_with_reload(&s.kit, settings_sections(s, &app), &app, reload_settings)
    } else {
        match view {
            View::Date => date_view(s, &app),
            View::Convert => convert_view(s, &app),
            _ => calculator(s, &app, view, look),
        }
    };
    let mut shell = UtilityShell::create(content)
        .with_title_row(kit::title_row(SPEC.name))
        .with_label(SPEC.name)
        .with_min_size(MIN_SIZE.0, MIN_SIZE.1);
    if !settings {
        shell = shell.with_modes(modes_row(s, &app, view));
    }
    let column = Dom::create_div()
        .with_css("display: flex; flex-direction: column; flex-grow: 1; min-height: 0px;")
        .with_child(shell.dom());
    // The scope as the window's body: no UA margin, the full window height.
    // Focusable (not a Tab stop): a click on the title row or the mode row's
    // background focuses at least the body, so the keyboard's TEXT is always
    // recorded somewhere the window's handler hears it.
    ShellThemeScope::create(column)
        .with_accent(ShellThemeAccent::Slate)
        .body()
        .with_tab_index(TabIndex::NoKeyboardFocus)
        .with_component_css(Css::from_string(look::KEYFRAMES))
        .with_callback(EventFilter::Window(WindowEventFilter::VirtualKeyDown), app.clone(), on_key)
        .with_callback(EventFilter::Window(WindowEventFilter::TextInput), app.clone(), on_text)
        .with_callback(EventFilter::Focus(FocusEventFilter::Paste), app.clone(), on_paste)
        .with_callback(EventFilter::Focus(FocusEventFilter::Copy), app, on_copy)
}

// ==== Callbacks ====

/// Runs `f` on the app's state; the window is rebuilt afterwards.
pub(crate) fn with_app(
    app: &mut RefAny,
    info: &mut CallbackInfo,
    f: impl FnOnce(&mut CalcApp, &mut CallbackInfo, &RefAny),
) -> Update {
    let handle = app.clone();
    let Some(mut guard) = app.downcast_mut::<CalcApp>() else {
        return Update::DoNothing;
    };
    f(&mut guard, info, &handle);
    Update::RefreshDom
}

/// The angle unit a setting names (degrees unless it says `rad` or `grad`).
fn angle_of(setting: Option<&str>) -> AngleUnit {
    match setting {
        Some("rad") => AngleUnit::Rad,
        Some("grad") => AngleUnit::Grad,
        _ => AngleUnit::Deg,
    }
}

/// Cancel on the settings page put the settings back: the calculator's copies of them (the
/// grouping, the angle unit) follow.
fn reload_settings(app: &mut RefAny, _info: &mut CallbackInfo, settings: &AppSettings) {
    if let Some(mut s) = app.downcast_mut::<CalcApp>() {
        s.calc.grouping = settings.get_bool("grouping", true);
        s.calc.angle = angle_of(settings.get("angle"));
    };
}

fn keep_history(s: &CalcApp) -> bool {
    let mut k = s.kit.clone();
    k.downcast_ref::<kit::Kit>()
        .map_or(true, |k| k.settings.get_bool("history", true))
}

/// Writes the history file (one write at a time; a newer state waits).
fn save_history(s: &mut CalcApp, info: &mut CallbackInfo, app: &RefAny) {
    if !s.calc.history_dirty {
        return;
    }
    if !keep_history(s) {
        s.calc.history_dirty = false;
        return;
    }
    if s.saving {
        s.save_pending = true;
        return;
    }
    s.calc.history_dirty = false;
    s.saving = true;
    kit::spawn_file_jobs(
        info,
        &s.data_root,
        vec![FileJob::Put {
            key: history_key(),
            bytes: history::to_jsonl(&s.calc.history).into_bytes(),
        }],
        app.clone(),
        TAG_SAVE,
        on_files_done,
    );
}

/// After a key: the display lines and the graph's functions for scripts,
/// the history to disk.
fn after_calc(s: &mut CalcApp, info: &mut CallbackInfo, app: &RefAny) {
    s.announce_display();
    s.announce_plots();
    save_history(s, info, app);
}

fn run_action(s: &mut CalcApp, action: Action) {
    s.notice.clear();
    match action {
        Action::Calc(cmd) => s.calc.apply(cmd, now_secs()),
        Action::Second => s.calc.second = !s.calc.second,
        Action::FlipFe => s.calc.fe = !s.calc.fe,
        Action::Blank => {}
    }
}

/// The View menu's choice (the next layout builds the view).
fn set_pin(s: &mut CalcApp, info: &mut CallbackInfo, pin: Pin) {
    if s.pin == pin {
        return;
    }
    remember_conversion(s);
    s.pin = pin;
    println!("AZCALC_SCREEN {}", pin.key());
    kit::set_value(&s.kit, info, "screen", pin.key());
}

fn copy_result(s: &CalcApp, info: &mut CallbackInfo) {
    let text = s.calc.copy_text();
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text.as_str()),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
    println!("AZCALC_COPIED {text}");
}

/// The keys go to the calculator's surface again (after a click on a key,
/// or when a rebuild took the focused node away).
fn focus_surface(info: &mut CallbackInfo) {
    info.set_focus_to_path(
        DomId { inner: 0 },
        CssPath {
            selectors: vec![CssPathSelector::Id(ids::CALC)].into(),
        },
    );
}

/// The surface mounted (the window opened, the settings page closed, Date
/// gave way): it takes the keyboard when nothing else has it.
extern "C" fn on_surface_mounted(_data: RefAny, mut info: CallbackInfo) -> Update {
    if info.get_focused_node().into_option().is_none() {
        let node = info.get_hit_node();
        info.set_focus(FocusTarget::Id(node));
    }
    Update::DoNothing
}

extern "C" fn on_window_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let Some(s) = data.downcast_ref::<CalcApp>() else {
        return Update::DoNothing;
    };
    kit::on_window_created(&s.kit, &mut info);
    kit::spawn_file_jobs(
        &mut info,
        &s.data_root,
        vec![FileJob::Get { key: history_key() }],
        app.clone(),
        TAG_LOAD,
        on_files_done,
    );
    s.announce_display();
    // The focus a create callback asks for waits for the first layout.
    focus_surface(&mut info);
    Update::DoNothing
}

extern "C" fn on_files_done(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(reply) = kit::take_reply(&mut msg) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, info, handle| match reply.tag {
        TAG_LOAD => {
            s.history_loaded = true;
            let mut loaded = Vec::new();
            for outcome in reply.outcomes {
                match outcome {
                    FileOutcome::Got {
                        result: Ok(Some(bytes)),
                        ..
                    } => {
                        let (entries, skipped) = history::parse_jsonl(&String::from_utf8_lossy(&bytes));
                        if skipped > 0 {
                            eprintln!("[azcalculator] {skipped} history line(s) could not be read");
                        }
                        loaded = entries;
                    }
                    FileOutcome::Got { result: Err(e), .. } => {
                        s.notice = format!("The history could not be read: {e}");
                    }
                    _ => {}
                }
            }
            if loaded.is_empty() && s.sample {
                loaded = sample_history();
                s.calc.history_dirty = true;
            }
            // Calculations made before the file arrived come after it.
            let session = std::mem::take(&mut s.calc.history);
            if !session.is_empty() {
                s.calc.history_dirty = true;
            }
            loaded.extend(session);
            history::trim(&mut loaded);
            s.calc.history = loaded;
            println!("AZCALC_HISTORY_LOADED {}", s.calc.history.len());
            save_history(s, info, handle);
        }
        _ => {
            s.saving = false;
            match reply.outcomes.iter().find_map(FileOutcome::error) {
                Some(e) => s.notice = format!("The history could not be saved: {e}"),
                None => println!("AZCALC_HISTORY_SAVED {}", s.calc.history.len()),
            }
            if s.save_pending {
                s.save_pending = false;
                s.calc.history_dirty = true;
                save_history(s, info, handle);
            }
        }
    })
}

/// A key of a keypad, clicked (or activated with Space on a key tabbed to).
extern "C" fn on_key_click(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, action)) = data.downcast_ref::<KeyRef>().map(|k| (k.app.clone(), k.action)) else {
        return Update::DoNothing;
    };
    // A pointer click hands the keyboard back to the surface (a key the
    // user tabbed to keeps it).
    let by_keyboard = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
        .is_some();
    if !by_keyboard {
        focus_surface(&mut info);
    }
    with_app(&mut app, &mut info, |s, info, handle| {
        run_action(s, action);
        after_calc(s, info, handle);
    })
}

extern "C" fn on_base(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, base)) = data.downcast_ref::<BaseRef>().map(|b| (b.app.clone(), b.base)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        s.calc.set_base(base);
        s.announce_display();
    })
}

extern "C" fn on_bit(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, bit)) = data.downcast_ref::<BitRef>().map(|b| (b.app.clone(), b.bit)) else {
        return Update::DoNothing;
    };
    with_app(&mut app, &mut info, |s, _info, _| {
        s.calc.toggle_bit(bit);
        s.announce_display();
    })
}

extern "C" fn on_word(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.calc.set_word(WordSize::ALL[index.min(WordSize::ALL.len() - 1)]);
        s.announce_display();
    })
}

extern "C" fn on_angle(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        let angle = AngleUnit::ALL[state.selected_index.min(AngleUnit::ALL.len() - 1)];
        s.calc.angle = angle;
        kit::set_value(&s.kit, info, "angle", &angle.label().to_lowercase());
        s.announce_display();
    })
}

extern "C" fn on_history_entry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<HistoryRef>().map(|h| (h.app.clone(), h.index)) else {
        return Update::DoNothing;
    };
    focus_surface(&mut info);
    with_app(&mut app, &mut info, |s, _info, _| {
        let before = s.calc.clone();
        s.calc.use_history(index);
        if s.calc == before {
            s.notice = "That calculation belongs to another mode.".to_string();
        }
        s.announce_display();
    })
}

extern "C" fn on_memory_entry(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, index)) = data.downcast_ref::<HistoryRef>().map(|h| (h.app.clone(), h.index)) else {
        return Update::DoNothing;
    };
    focus_surface(&mut info);
    with_app(&mut app, &mut info, |s, _info, _| {
        if index < s.calc.memory.items.len() {
            let item = s.calc.memory.items.remove(index);
            s.calc.memory.items.insert(0, item);
            s.calc.apply(Cmd::MemRecall, now_secs());
            s.announce_display();
        }
    })
}

extern "C" fn on_clear_history(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        s.calc.clear_history();
        save_history(s, info, handle);
    })
}

extern "C" fn on_panel_tab(mut data: RefAny, mut info: CallbackInfo, state: TabHeaderState) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.panel = if state.active_tab == 1 { Panel::Memory } else { Panel::History };
    })
}

extern "C" fn on_toggle_panel(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.panel_shown = !s.panel_shown;
    })
}

extern "C" fn on_open_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| kit::open_settings(&s.kit, None))
}

extern "C" fn on_view_pick(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        set_pin(s, info, Pin::ALL[index.min(Pin::ALL.len() - 1)]);
    })
}

extern "C" fn on_grouping(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    with_app(&mut data, &mut info, |s, info, _| {
        s.calc.grouping = state.checked;
        kit::set_value(&s.kit, info, "grouping", if state.checked { "true" } else { "false" });
    })
}

extern "C" fn on_keep_history(mut data: RefAny, mut info: CallbackInfo, state: SwitchState) -> Update {
    with_app(&mut data, &mut info, |s, info, handle| {
        kit::set_value(&s.kit, info, "history", if state.checked { "true" } else { "false" });
        if state.checked {
            s.calc.history_dirty = true;
            save_history(s, info, handle);
        }
    })
}

// ==== Date and converter ====

fn announce_date(s: &CalcApp) {
    println!("AZCALC_DATE {}", date_result(&s.date).join(" | "));
}

fn announce_conversion(s: &CalcApp) {
    let (from, to) = convert_values(&s.convert);
    let category = &CATEGORIES[s.convert.category.min(CATEGORIES.len() - 1)];
    println!(
        "AZCALC_CONVERT {} {} = {} {}",
        from,
        category.units[s.convert.from].symbol,
        to,
        category.units[s.convert.to].symbol
    );
}

/// Puts the conversion on screen into the recent list (newest first, five kept).
fn remember_conversion(s: &mut CalcApp) {
    let c = &s.convert;
    let category = &CATEGORIES[c.category.min(CATEGORIES.len() - 1)];
    let (from_text, _) = convert_values(c);
    let Ok(value) = Num::parse(&from_text) else {
        return;
    };
    let Ok(line) = units::recent_line(&value, &category.units[c.from], &category.units[c.to]) else {
        return;
    };
    let recent = &mut s.convert.recent;
    recent.retain(|r| *r != line);
    recent.insert(0, line);
    recent.truncate(5);
}

extern "C" fn on_field_text(mut data: RefAny, mut info: CallbackInfo, state: TextInputState) -> OnTextInputReturn {
    let keep = OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    };
    let Some((mut app, field)) = data.downcast_ref::<FieldRef>().map(|f| (f.app.clone(), f.field)) else {
        return keep;
    };
    let text = state.get_text().as_str().to_string();
    let update = with_app(&mut app, &mut info, |s, _info, _| {
        match field {
            Field::DateFrom => s.date.from = text,
            Field::DateTo => s.date.to = text,
            Field::Years => s.date.years = text,
            Field::Months => s.date.months = text,
            Field::Days => s.date.days = text,
            Field::ConvertFrom => {
                s.convert.from_text = text;
                s.convert.source = Side::From;
            }
            Field::ConvertTo => {
                s.convert.to_text = text;
                s.convert.source = Side::To;
            }
        }
        match field {
            Field::ConvertFrom | Field::ConvertTo => announce_conversion(s),
            _ => announce_date(s),
        }
    });
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_date_kind(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.date.kind = state.selected_index.min(1);
        announce_date(s);
    })
}

extern "C" fn on_date_sign(mut data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.date.subtract = state.selected_index == 1;
        announce_date(s);
    })
}

extern "C" fn on_date_today(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        s.date.from = today().to_string();
        announce_date(s);
    })
}

/// The converter's source value as the new "from" text (before units change).
fn settle_source(s: &mut CalcApp) {
    let (from, _) = convert_values(&s.convert);
    s.convert.from_text = from;
    s.convert.source = Side::From;
}

extern "C" fn on_convert_category(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        remember_conversion(s);
        settle_source(s);
        let old = &CATEGORIES[s.convert.category.min(CATEGORIES.len() - 1)];
        let (old_from, old_to) = (old.units[s.convert.from], old.units[s.convert.to]);
        let new = index.min(CATEGORIES.len() - 1);
        let (from, to) = units::units_after_category_change(&old_from, &old_to, &CATEGORIES[new]);
        s.convert.category = new;
        s.convert.from = from;
        s.convert.to = to;
        announce_conversion(s);
    })
}

extern "C" fn on_convert_from_unit(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        remember_conversion(s);
        settle_source(s);
        let units = CATEGORIES[s.convert.category.min(CATEGORIES.len() - 1)].units.len();
        s.convert.from = index.min(units - 1);
        announce_conversion(s);
    })
}

extern "C" fn on_convert_to_unit(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        remember_conversion(s);
        settle_source(s);
        let units = CATEGORIES[s.convert.category.min(CATEGORIES.len() - 1)].units.len();
        s.convert.to = index.min(units - 1);
        announce_conversion(s);
    })
}

extern "C" fn on_convert_swap(mut data: RefAny, mut info: CallbackInfo) -> Update {
    with_app(&mut data, &mut info, |s, _info, _| {
        remember_conversion(s);
        let (_, to) = convert_values(&s.convert);
        let c = &mut s.convert;
        std::mem::swap(&mut c.from, &mut c.to);
        c.from_text = to.replace(',', "");
        c.source = Side::From;
        announce_conversion(s);
    })
}

// ==== Keyboard, copy and paste ====

/// The character a KEYPAD key stands for, whatever the layout: the one
/// fallback for a key no text could be recorded for (nothing held the
/// keyboard). Every other key's character depends on the layout - `*` is
/// Shift+8 only on a US keyboard, Shift++ on a German one, and a digit is
/// shifted on a French one - so only its text says it ([`on_text`]); a
/// guess from the key's position typed `*` for a German `(`.
fn keypad_char(vk: VirtualKeyCode) -> Option<char> {
    use VirtualKeyCode as K;
    let pad = [
        K::Numpad0, K::Numpad1, K::Numpad2, K::Numpad3, K::Numpad4, K::Numpad5, K::Numpad6, K::Numpad7,
        K::Numpad8, K::Numpad9,
    ];
    if let Some(d) = pad.iter().position(|k| *k == vk) {
        return Some(char::from(b'0' + d as u8));
    }
    Some(match vk {
        K::NumpadAdd => '+',
        K::NumpadSubtract => '-',
        K::NumpadMultiply => '*',
        K::NumpadDivide => '/',
        K::NumpadDecimal | K::NumpadComma => '.',
        K::NumpadEquals => '=',
        _ => return None,
    })
}

/// Alt+0..5: the View menu.
fn pin_for_key(vk: VirtualKeyCode) -> Option<Pin> {
    let keys = [
        VirtualKeyCode::Key0,
        VirtualKeyCode::Key1,
        VirtualKeyCode::Key2,
        VirtualKeyCode::Key3,
        VirtualKeyCode::Key4,
        VirtualKeyCode::Key5,
    ];
    keys.iter().position(|k| *k == vk).map(|i| Pin::ALL[i])
}

/// The text the key being handled typed, if any (recorded against the
/// focused node before the key's pass; on Windows in a pass of its own
/// after it), without control characters (Windows' WM_CHAR sends Backspace
/// and Enter as text too).
fn typed_text(info: &CallbackInfo) -> Option<String> {
    info.get_text_changeset()
        .into_option()
        .map(|c| c.inserted_text.as_str().chars().filter(|c| !c.is_control()).collect::<String>())
        .filter(|t| !t.is_empty())
}

/// The keys that type nothing, and the chords. A key that typed a character
/// is the text handler's ([`on_text`]); one that could not (nothing held
/// the keyboard, so no text was recorded) types only if it is a keypad key
/// ([`keypad_char`]).
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_mut::<CalcApp>().map(|mut s| {
        // A fallback character's text no longer follows once another key comes.
        s.fallback = None;
        s.kit.clone()
    }) else {
        return Update::DoNothing;
    };
    if let Some(update) = kit::handle_key(&kit_ref, &mut info) {
        return update;
    }
    if kit::settings_open(&kit_ref) {
        return Update::DoNothing;
    }
    let Some(vk) = info.get_current_keyboard_state().current_virtual_keycode.into_option() else {
        return Update::DoNothing;
    };
    let m = info.get_key_modifiers();
    let command = m.primary_down();
    let Some(view) = data.downcast_ref::<CalcApp>().map(|s| s.current_view(&info)) else {
        return Update::DoNothing;
    };
    // Alt+0..5: the View menu.
    if m.alt && !command {
        if let Some(pin) = pin_for_key(vk) {
            info.prevent_default();
            return with_app(&mut data, &mut info, |s, info, _| set_pin(s, info, pin));
        }
        return Update::DoNothing;
    }
    let Some(mode) = view.calc_mode() else {
        // Date and Convert: the fields take the keys.
        return Update::DoNothing;
    };
    let typed = typed_text(&info);
    let focused = info.get_focused_node().into_option().is_some();
    if !focused {
        // Nothing holds the keyboard (a rebuild took the focused key away):
        // the surface takes it, so the next key's text arrives.
        focus_surface(&mut info);
    }
    if command {
        let cmd = match vk {
            VirtualKeyCode::C => {
                info.prevent_default();
                if let Some(s) = data.downcast_ref::<CalcApp>() {
                    copy_result(&s, &mut info);
                }
                return Update::DoNothing;
            }
            VirtualKeyCode::H => {
                info.prevent_default();
                return with_app(&mut data, &mut info, |s, _info, _| {
                    s.panel_shown = !s.panel_shown;
                });
            }
            VirtualKeyCode::M => Cmd::MemStore,
            VirtualKeyCode::R => Cmd::MemRecall,
            VirtualKeyCode::P => Cmd::MemAdd,
            VirtualKeyCode::Q => Cmd::MemSub,
            VirtualKeyCode::L => Cmd::MemClear,
            _ => return Update::DoNothing,
        };
        info.prevent_default();
        return with_app(&mut data, &mut info, |s, info, handle| {
            run_action(s, Action::Calc(cmd));
            after_calc(s, info, handle);
        });
    }
    // The angle unit (graphing) and the base (Programmer) by function key.
    let setting = match (mode, vk) {
        (CalcMode::Scientific, VirtualKeyCode::F3) => Some((Some(AngleUnit::Deg), None)),
        (CalcMode::Scientific, VirtualKeyCode::F4) => Some((Some(AngleUnit::Rad), None)),
        (CalcMode::Scientific, VirtualKeyCode::F5) => Some((Some(AngleUnit::Grad), None)),
        (CalcMode::Programmer, VirtualKeyCode::F5) => Some((None, Some(Base::Hex))),
        (CalcMode::Programmer, VirtualKeyCode::F6) => Some((None, Some(Base::Dec))),
        (CalcMode::Programmer, VirtualKeyCode::F7) => Some((None, Some(Base::Oct))),
        (CalcMode::Programmer, VirtualKeyCode::F8) => Some((None, Some(Base::Bin))),
        _ => None,
    };
    if let Some((angle, base)) = setting {
        info.prevent_default();
        return with_app(&mut data, &mut info, |s, _info, _| {
            if let Some(a) = angle {
                s.calc.angle = a;
            }
            if let Some(b) = base {
                s.calc.set_base(b);
            }
            s.announce_display();
        });
    }
    let named = match vk {
        VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter => Some(NamedKey::Enter),
        VirtualKeyCode::Back => Some(NamedKey::Backspace),
        VirtualKeyCode::Escape => Some(NamedKey::Escape),
        VirtualKeyCode::Delete => Some(NamedKey::Delete),
        VirtualKeyCode::F9 => Some(NamedKey::F9),
        _ => None,
    };
    if let Some(n) = named {
        // Enter must not also click the focused key.
        info.prevent_default();
        let cmd = named_command(n);
        return with_app(&mut data, &mut info, |s, info, handle| {
            run_action(s, Action::Calc(cmd));
            after_calc(s, info, handle);
        });
    }
    if typed.is_some() || focused {
        // The text handler reads the character this key typed - recorded
        // with the key (macOS, X11, Wayland, the debug server) or right after
        // it (Windows' WM_CHAR).
        return Update::DoNothing;
    }
    let Some(c) = keypad_char(vk) else {
        return Update::DoNothing;
    };
    info.prevent_default();
    with_app(&mut data, &mut info, |s, info, handle| {
        s.notice.clear();
        s.fallback = Some(c);
        s.calc.type_char(c, now_secs());
        after_calc(s, info, handle);
    })
}

/// The characters a key typed - whatever key position made them on the
/// user's layout: `*` from Shift+8 (US), Shift++ (German) or the keypad,
/// letters for names, a dead key's composed character.
extern "C" fn on_text(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some(text) = typed_text(&info) else {
        return Update::DoNothing;
    };
    let Some((kit_ref, view, naming)) = data
        .downcast_ref::<CalcApp>()
        .map(|s| (s.kit.clone(), s.current_view(&info), !s.calc.letters.is_empty()))
    else {
        return Update::DoNothing;
    };
    if kit::settings_open(&kit_ref) || view.calc_mode().is_none() {
        // The settings page's and Date's / Convert's fields take their text.
        return Update::DoNothing;
    }
    if text.trim().is_empty() && !naming {
        // A space means nothing here (unless it ends a name being typed), and
        // it stays the Space that activates a key the user tabbed to.
        return Update::DoNothing;
    }
    let m = info.get_key_modifiers();
    if m.primary_down() {
        return Update::DoNothing;
    }
    let vk = info.get_current_keyboard_state().current_virtual_keycode.into_option();
    if m.alt && vk.and_then(pin_for_key).is_some() {
        // Alt+0..5 chose a view; on a Mac Option+digit types a character too.
        info.prevent_default();
        return Update::DoNothing;
    }
    info.prevent_default();
    let already = data
        .downcast_mut::<CalcApp>()
        .and_then(|mut s| s.fallback.take())
        .is_some_and(|c| text == c.to_string());
    if already {
        // Its key typed it by its position (nothing held the keyboard then);
        // Windows sends the text after the key.
        return Update::DoNothing;
    }
    with_app(&mut data, &mut info, |s, info, handle| {
        s.notice.clear();
        s.calc.type_text(&text, now_secs());
        after_calc(s, info, handle);
    })
}

/// Ctrl/Cmd+V (the engine reads the clipboard for the focused node that
/// listens for paste): the clipboard's text as an expression.
extern "C" fn on_paste(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((kit_ref, view)) = data.downcast_ref::<CalcApp>().map(|s| (s.kit.clone(), s.current_view(&info))) else {
        return Update::DoNothing;
    };
    if kit::settings_open(&kit_ref) || view.calc_mode().is_none() {
        return Update::DoNothing;
    }
    let Some(content) = info.get_clipboard_content().into_option() else {
        return Update::DoNothing;
    };
    let text = content.plain_text.as_str().to_string();
    info.prevent_default();
    with_app(&mut data, &mut info, |s, info, handle| {
        match s.calc.paste(&text) {
            Ok(()) => {
                println!("AZCALC_PASTED {text}");
                s.notice.clear();
            }
            Err(e) => s.notice = format!("\"{}\" could not be pasted: {e}", text.trim()),
        }
        after_calc(s, info, handle);
    })
}

/// The Copy event (Edit > Copy on the focused key or surface): the result,
/// not a selection.
extern "C" fn on_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((kit_ref, view)) = data.downcast_ref::<CalcApp>().map(|s| (s.kit.clone(), s.current_view(&info))) else {
        return Update::DoNothing;
    };
    if kit::settings_open(&kit_ref) || view.calc_mode().is_none() {
        return Update::DoNothing;
    }
    info.prevent_default();
    if let Some(s) = data.downcast_ref::<CalcApp>() {
        copy_result(&s, &mut info);
    }
    Update::DoNothing
}

#[cfg(test)]
mod tests {
    //! The parts of the window that are plain data.

    use super::*;

    #[test]
    fn the_window_picks_its_calculator_by_its_size() {
        assert_eq!(view_for_size(DEFAULT_SIZE.0, DEFAULT_SIZE.1), View::Micro, "the default is small");
        assert_eq!(view_for_size(MIN_SIZE.0, MIN_SIZE.1), View::Micro);
        assert_eq!(view_for_size(639.0, 900.0), View::Micro, "tall but narrow");
        assert_eq!(view_for_size(640.0, 560.0), View::Programmer, "widened to the right");
        assert_eq!(view_for_size(1200.0, 699.0), View::Programmer, "wide but not tall");
        assert_eq!(view_for_size(900.0, 700.0), View::Graph);
        assert_eq!(view_for_size(1920.0, 1080.0), View::Graph, "maximized");
    }

    #[test]
    fn the_view_menu_pins_a_view() {
        assert_eq!(Pin::by_key("auto"), Some(Pin::Auto));
        assert_eq!(Pin::Auto.view(), None);
        assert_eq!(Pin::by_key("scientific"), Some(Pin::Graphing), "the old screen name");
        assert_eq!(Pin::Graphing.view(), Some(View::Graph));
        assert_eq!(Pin::Graphing.view().and_then(View::calc_mode), Some(CalcMode::Scientific));
        assert_eq!(View::Date.calc_mode(), None);
        for p in Pin::ALL {
            assert!(SCREENS.contains(&p.key()), "{}", p.key());
            assert_eq!(Pin::ALL[p.index()], p);
        }
        assert_eq!(pin_for_key(VirtualKeyCode::Key2), Some(Pin::Programmer));
        assert_eq!(pin_for_key(VirtualKeyCode::Key9), None);
    }

    #[test]
    fn a_long_expression_shows_its_end() {
        assert_eq!(tail("12 \u{d7} 3", 40), "12 \u{d7} 3");
        let long = "1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10 + 11 + 12";
        let shown = tail(long, 20);
        assert_eq!(shown.chars().count(), 20);
        assert!(shown.starts_with('\u{2026}') && shown.ends_with("11 + 12"), "{shown}");
    }

    #[test]
    fn long_results_get_smaller() {
        assert_eq!(result_px("42", View::Micro), 38);
        assert!(result_px("1.4142135623730950488016887242097", View::Micro) < 20);
        assert!(result_px("Cannot divide by zero", View::Micro) >= 14);
    }

    #[test]
    fn only_the_keypad_types_without_its_text() {
        assert_eq!(keypad_char(VirtualKeyCode::Numpad7), Some('7'));
        assert_eq!(keypad_char(VirtualKeyCode::NumpadMultiply), Some('*'));
        assert_eq!(keypad_char(VirtualKeyCode::NumpadComma), Some('.'), "the German keypad's comma");
        assert_eq!(keypad_char(VirtualKeyCode::NumpadEnter), None, "Enter is a named key");
        // The key positions say nothing about the character: Shift+8 is `*`
        // on a US keyboard and `(` on a German one, whose `*` is Shift and
        // the key right of Ü (the US `]` position). Only the TEXT says it.
        assert_eq!(keypad_char(VirtualKeyCode::Key8), None);
        assert_eq!(keypad_char(VirtualKeyCode::RBracket), None);
        assert_eq!(keypad_char(VirtualKeyCode::A), None);
    }

    #[test]
    fn the_typeset_display_wraps_results_and_functions() {
        let mut c = Calculator::new();
        assert!(typeset(&c).is_none(), "nothing typed, nothing set");
        c.type_text("1/4=", 1);
        assert_eq!(typeset(&c).unwrap().text(), "(1)/(4) = ");
        c.type_text("y=x^2", 1);
        assert_eq!(typeset(&c).unwrap().text(), "y = x^(2)");
        c.type_char('=', 1);
        assert_eq!(typeset(&c).unwrap().text(), "y = x^(2)");
    }

    #[test]
    fn the_date_screen_explains_a_bad_date() {
        let mut d = DateState {
            kind: 0,
            from: "2025-08-01".into(),
            to: "2026-10-04".into(),
            years: "0".into(),
            months: "0".into(),
            days: "0".into(),
            subtract: false,
        };
        assert_eq!(date_result(&d), vec!["1 year, 2 months, 3 days", "61 weeks, 2 days", "429 days"]);
        d.to = "soon".into();
        assert_eq!(date_result(&d), vec!["Enter the second date as YYYY-MM-DD."]);
        d.kind = 1;
        d.days = "100".into();
        d.from = "2026-10-01".into();
        assert_eq!(date_result(&d), vec!["2027-01-09 (Saturday)"]);
        d.days = "x".into();
        assert_eq!(date_result(&d), vec!["Years, months and days are whole numbers."]);
    }

    #[test]
    fn the_converter_computes_the_field_not_typed_into() {
        let mut c = ConvertState {
            category: 0,
            from: 3,
            to: 7,
            from_text: "42.195".into(),
            to_text: String::new(),
            source: Side::From,
            recent: Vec::new(),
        };
        assert_eq!(convert_values(&c), ("42.195".to_string(), "26.21875746".to_string()));
        c.source = Side::To;
        c.to_text = "1".into();
        assert_eq!(convert_values(&c), ("1.609344".to_string(), "1".to_string()));
        c.to_text = "abc".into();
        assert_eq!(convert_values(&c).0, "\u{2014}");
    }
}
