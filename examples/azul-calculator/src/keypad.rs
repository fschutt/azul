//! The keypads: the standard 4 x 6 (the small window, and beside the
//! programmer panel with mod and parentheses in place of %, 1/x and √x),
//! the programmer panel's bitwise operators and hex digits, the memory row,
//! and the scientific 9 x 5 of the graphing view - each key a themed box
//! ([`crate::look`]) the grid stretches, a Tab stop with a spoken name, its
//! label a bare text node.

use azul::{
    callbacks::CallbackType,
    dom::{AccessibilityInfo, AccessibilityRole, TabIndex},
    prelude::*,
    str::String as AzString,
};

use crate::calc::Cmd;
use crate::expr::{BinOp, Const, Func, Post};
use crate::ids;
use crate::look;
use crate::programmer::Base;

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

/// How a key looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Digits, the point, +/-: the brightest face.
    Digit,
    /// + − × ÷.
    Op,
    /// Functions, clearing keys, parentheses.
    Func,
    /// `=`: the accent.
    Equals,
    /// The memory row: quiet.
    Mem,
}

/// One key of a keypad.
#[derive(Clone, Copy, Debug)]
pub struct KeyDef {
    pub label: &'static str,
    /// The DOM id's suffix (`key-7`, `key-plus`).
    pub id: &'static str,
    pub action: Action,
    pub kind: Kind,
}

const fn key(label: &'static str, id: &'static str, cmd: Cmd, kind: Kind) -> KeyDef {
    KeyDef {
        label,
        id,
        action: Action::Calc(cmd),
        kind,
    }
}

const fn digit(label: &'static str, id: &'static str, d: u8) -> KeyDef {
    key(label, id, Cmd::Digit(d), Kind::Digit)
}

const fn op(label: &'static str, id: &'static str, o: BinOp) -> KeyDef {
    key(label, id, Cmd::Op(o), Kind::Op)
}

const fn func(label: &'static str, id: &'static str, cmd: Cmd) -> KeyDef {
    key(label, id, cmd, Kind::Func)
}

pub const BLANK: KeyDef = KeyDef {
    label: "",
    id: "",
    action: Action::Blank,
    kind: Kind::Func,
};

const EQUALS: KeyDef = key("=", "key-equals", Cmd::Equals, Kind::Equals);
const NEGATE: KeyDef = key("+/\u{2212}", "key-negate", Cmd::Negate, Kind::Digit);
const POINT: KeyDef = key(".", "key-point", Cmd::Point, Kind::Digit);

/// The standard keypad, 4 columns x 6 rows (Windows' Standard calculator).
/// In Programmer mode `%` is mod and 1/x, √x give way to the parentheses.
#[must_use]
pub fn standard_keys(programmer: bool) -> Vec<KeyDef> {
    let (first, row2a, row2c) = if programmer {
        (
            op("mod", "key-mod", BinOp::Mod),
            func("(", "key-lparen", Cmd::LParen),
            func(")", "key-rparen", Cmd::RParen),
        )
    } else {
        (
            func("%", "key-percent", Cmd::Post(Post::Percent)),
            func("1/x", "key-recip", Cmd::Func(Func::Recip)),
            func("\u{221a}x", "key-sqrt", Cmd::Func(Func::Sqrt)),
        )
    };
    vec![
        first,
        func("CE", "key-ce", Cmd::ClearEntry),
        func("C", "key-c", Cmd::Clear),
        func("\u{232b}", "key-back", Cmd::Backspace),
        row2a,
        func("x\u{b2}", "key-square", Cmd::Post(Post::Square)),
        row2c,
        op("\u{f7}", "key-divide", BinOp::Div),
        digit("7", "key-7", 7),
        digit("8", "key-8", 8),
        digit("9", "key-9", 9),
        op("\u{d7}", "key-multiply", BinOp::Mul),
        digit("4", "key-4", 4),
        digit("5", "key-5", 5),
        digit("6", "key-6", 6),
        op("\u{2212}", "key-minus", BinOp::Sub),
        digit("1", "key-1", 1),
        digit("2", "key-2", 2),
        digit("3", "key-3", 3),
        op("+", "key-plus", BinOp::Add),
        NEGATE,
        digit("0", "key-0", 0),
        POINT,
        EQUALS,
    ]
}

/// The memory row.
#[must_use]
pub fn memory_keys() -> Vec<KeyDef> {
    vec![
        key("MC", "key-mc", Cmd::MemClear, Kind::Mem),
        key("MR", "key-mr", Cmd::MemRecall, Kind::Mem),
        key("M+", "key-mplus", Cmd::MemAdd, Kind::Mem),
        key("M\u{2212}", "key-mminus", Cmd::MemSub, Kind::Mem),
        key("MS", "key-ms", Cmd::MemStore, Kind::Mem),
    ]
}

/// The programmer panel's keys, 6 columns x 3 rows: the bitwise operators,
/// the shifts and rotates, and the hex digits.
#[must_use]
pub fn programmer_keys() -> Vec<KeyDef> {
    vec![
        op("AND", "key-and", BinOp::And),
        op("OR", "key-or", BinOp::Or),
        op("XOR", "key-xor", BinOp::Xor),
        func("NOT", "key-not", Cmd::Not),
        op("<<", "key-shl", BinOp::Shl),
        op(">>", "key-shr", BinOp::Shr),
        op("NAND", "key-nand", BinOp::Nand),
        op("NOR", "key-nor", BinOp::Nor),
        op("ROL", "key-rol", BinOp::Rol),
        op("ROR", "key-ror", BinOp::Ror),
        func("x^y", "key-pow", Cmd::Op(BinOp::Pow)),
        func("n!", "key-factorial", Cmd::Post(Post::Factorial)),
        digit("A", "key-a", 10),
        digit("B", "key-b", 11),
        digit("C", "key-hex-c", 12),
        digit("D", "key-d", 13),
        digit("E", "key-hex-e", 14),
        digit("F", "key-f", 15),
    ]
}

/// The scientific keypad of the graphing view, 9 columns x 5 rows: the
/// functions left of the digits; 2nd swaps x² / x³, √ / ∛, sin / cos / tan
/// and their inverses, 10^x / e^x, log / log₂.
#[must_use]
pub fn scientific_keys(second: bool) -> Vec<KeyDef> {
    let pick = |a: KeyDef, b: KeyDef| if second { b } else { a };
    vec![
        KeyDef {
            label: "2nd",
            id: "key-second",
            action: Action::Second,
            kind: Kind::Func,
        },
        func("\u{3c0}", "key-pi", Cmd::Const(Const::Pi)),
        func("e", "key-e", Cmd::Const(Const::E)),
        func("C", "key-c", Cmd::Clear),
        func("\u{232b}", "key-back", Cmd::Backspace),
        func("(", "key-lparen", Cmd::LParen),
        func(")", "key-rparen", Cmd::RParen),
        func("n!", "key-factorial", Cmd::Post(Post::Factorial)),
        op("\u{f7}", "key-divide", BinOp::Div),
        // row 2
        pick(
            func("x\u{b2}", "key-square", Cmd::Post(Post::Square)),
            func("x\u{b3}", "key-cube", Cmd::Post(Post::Cube)),
        ),
        func("x^y", "key-pow", Cmd::Op(BinOp::Pow)),
        pick(func("sin", "key-sin", Cmd::Func(Func::Sin)), func("sin\u{207b}\u{b9}", "key-asin", Cmd::Func(Func::Asin))),
        pick(func("cos", "key-cos", Cmd::Func(Func::Cos)), func("cos\u{207b}\u{b9}", "key-acos", Cmd::Func(Func::Acos))),
        pick(func("tan", "key-tan", Cmd::Func(Func::Tan)), func("tan\u{207b}\u{b9}", "key-atan", Cmd::Func(Func::Atan))),
        digit("7", "key-7", 7),
        digit("8", "key-8", 8),
        digit("9", "key-9", 9),
        op("\u{d7}", "key-multiply", BinOp::Mul),
        // row 3
        pick(
            func("\u{221a}x", "key-sqrt", Cmd::Func(Func::Sqrt)),
            func("\u{221b}x", "key-cbrt", Cmd::Func(Func::Cbrt)),
        ),
        pick(func("10^x", "key-pow10", Cmd::Func(Func::Pow10)), func("e^x", "key-exp", Cmd::Func(Func::Exp))),
        pick(func("log", "key-log", Cmd::Func(Func::Log)), func("log\u{2082}", "key-log2", Cmd::Func(Func::Log2))),
        func("ln", "key-ln", Cmd::Func(Func::Ln)),
        func("x", "key-x", Cmd::Var),
        digit("4", "key-4", 4),
        digit("5", "key-5", 5),
        digit("6", "key-6", 6),
        op("\u{2212}", "key-minus", BinOp::Sub),
        // row 4
        func("|x|", "key-abs", Cmd::Func(Func::Abs)),
        func("1/x", "key-recip", Cmd::Func(Func::Recip)),
        func("mod", "key-mod", Cmd::Op(BinOp::Mod)),
        KeyDef {
            label: "F-E",
            id: "key-fe",
            action: Action::FlipFe,
            kind: Kind::Func,
        },
        func("Exp", "key-exponent", Cmd::Exp),
        digit("1", "key-1", 1),
        digit("2", "key-2", 2),
        digit("3", "key-3", 3),
        op("+", "key-plus", BinOp::Add),
        // row 5
        func("MC", "key-mc", Cmd::MemClear),
        func("MR", "key-mr", Cmd::MemRecall),
        func("M+", "key-mplus", Cmd::MemAdd),
        func("MS", "key-ms", Cmd::MemStore),
        func("%", "key-percent", Cmd::Post(Post::Percent)),
        NEGATE,
        digit("0", "key-0", 0),
        POINT,
        EQUALS,
    ]
}

/// What a screen reader says for a key whose label is a sign.
#[must_use]
pub fn spoken(k: &KeyDef) -> &'static str {
    match k.id {
        "key-percent" => "Percent",
        "key-ce" => "Clear entry",
        "key-c" => "Clear",
        "key-back" => "Backspace",
        "key-recip" => "Reciprocal",
        "key-square" => "Square",
        "key-cube" => "Cube",
        "key-sqrt" => "Square root",
        "key-cbrt" => "Cube root",
        "key-divide" => "Divide",
        "key-multiply" => "Multiply",
        "key-minus" => "Minus",
        "key-plus" => "Plus",
        "key-negate" => "Change sign",
        "key-point" => "Decimal point",
        "key-equals" => "Equals",
        "key-mc" => "Memory clear",
        "key-mr" => "Memory recall",
        "key-mplus" => "Memory add",
        "key-mminus" => "Memory subtract",
        "key-ms" => "Memory store",
        "key-second" => "Second functions",
        "key-pi" => "Pi",
        "key-lparen" => "Open parenthesis",
        "key-rparen" => "Close parenthesis",
        "key-factorial" => "Factorial",
        "key-pow" => "Power",
        "key-pow10" => "Ten to the power",
        "key-exp" => "e to the power",
        "key-abs" => "Absolute value",
        "key-mod" => "Modulo",
        "key-fe" => "Scientific notation",
        "key-exponent" => "Exponent",
        "key-x" => "The variable x",
        "key-shl" => "Shift left",
        "key-shr" => "Shift right",
        "key-rol" => "Rotate left",
        "key-ror" => "Rotate right",
        _ => k.label,
    }
}

/// The data of a key's click: the app and what the key does.
pub struct KeyRef {
    pub app: RefAny,
    pub action: Action,
}

/// A key's look: its kind, then a state.
fn key_css(k: &KeyDef, enabled: bool, on: bool) -> String {
    let kind = match k.kind {
        Kind::Digit => look::KEY_DIGIT,
        Kind::Op => look::KEY_OP,
        Kind::Func => look::KEY_FUNC,
        Kind::Equals => look::KEY_EQUALS,
        Kind::Mem => look::KEY_MEM,
    };
    format!(
        "{} {} {} {}",
        look::KEY,
        kind,
        if on { look::KEY_ON } else { "" },
        if enabled { "" } else { look::KEY_OFF }
    )
}

/// The callbacks a key needs besides its click: the paste and copy chords
/// reach the FOCUSED node only, and a key the user tabbed to is focused.
pub struct KeyHooks {
    pub click: CallbackType,
    pub paste: CallbackType,
    pub copy: CallbackType,
}

/// One key: a themed box with its label, a Tab stop with its spoken name;
/// a key that cannot act in this state (a digit the base does not take) is
/// dimmed, inert and out of the Tab order.
#[must_use]
pub fn key_dom(app: &RefAny, k: &KeyDef, enabled: bool, on: bool, hooks: &KeyHooks) -> Dom {
    if k.action == Action::Blank {
        return Dom::create_div();
    }
    let mut dom = Dom::create_div_with_text(AzString::from(k.label))
        .with_id(ids::named(k.id))
        .with_css(key_css(k, enabled, on))
        .with_accessibility_info(AccessibilityInfo::named(spoken(k), AccessibilityRole::PushButton))
        .with_callback(EventFilter::Focus(FocusEventFilter::Paste), app.clone(), hooks.paste)
        .with_callback(EventFilter::Focus(FocusEventFilter::Copy), app.clone(), hooks.copy);
    if enabled {
        dom = dom.with_tab_index(TabIndex::Auto).with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(KeyRef {
                app: app.clone(),
                action: k.action,
            }),
            hooks.click,
        );
    }
    dom
}

/// A grid of `keys` in `columns` columns and as many rows as they fill,
/// the rows sharing the height the grid gets (`row_px` > 0: a fixed row
/// height instead). `base`: Programmer mode's base - a digit it does not
/// take is dimmed; `on`: the keys drawn pressed (2nd, F-E).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn grid(
    app: &RefAny,
    keys: &[KeyDef],
    columns: usize,
    base: Option<Base>,
    on: &[&str],
    id: AzString,
    css: &str,
    hooks: &KeyHooks,
) -> Dom {
    let rows = keys.len().div_ceil(columns.max(1));
    // `minmax(0, 1fr)`, not `1fr` (= `minmax(auto, 1fr)`): a bare `1fr`
    // track cannot shrink below its key's min-content, and a column of wide
    // labels would push the last column out of the keypad.
    let template_columns = vec!["minmax(0, 1fr)"; columns].join(" ");
    let template_rows = vec!["minmax(0, 1fr)"; rows].join(" ");
    let cells: Vec<Dom> = keys
        .iter()
        .map(|k| {
            let enabled = match (k.action, base) {
                (Action::Calc(Cmd::Digit(d)), Some(b)) => b.accepts(d),
                (Action::Calc(Cmd::Point), Some(_)) => false,
                _ => true,
            };
            key_dom(app, k, enabled, on.contains(&k.id), hooks)
        })
        .collect();
    Dom::create_div()
        .with_id(id)
        .with_css(format!(
            "display: grid; grid-template-columns: {template_columns}; \
             grid-template-rows: {template_rows}; gap: 5px; min-height: 0px; min-width: 0px; {css}"
        ))
        .with_children(DomVec::from_vec(cells))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_keypad_key_has_a_unique_id() {
        for (name, keys) in [
            ("standard", standard_keys(false)),
            ("standard, programmer", standard_keys(true)),
            ("memory", memory_keys()),
            ("programmer", programmer_keys()),
            ("scientific", scientific_keys(false)),
            ("scientific 2nd", scientific_keys(true)),
        ] {
            let mut seen = Vec::new();
            for k in keys.iter().filter(|k| k.action != Action::Blank) {
                assert!(k.id.starts_with("key-"), "{name}: {}", k.id);
                assert!(!seen.contains(&k.id), "{name}: {} twice", k.id);
                seen.push(k.id);
            }
        }
        // The programmer view shows the standard keypad beside its panel:
        // no id twice across the two.
        let mut ids: Vec<&str> = standard_keys(true).iter().map(|k| k.id).collect();
        for k in programmer_keys() {
            assert!(!ids.contains(&k.id), "{} on both programmer pads", k.id);
            ids.push(k.id);
        }
    }

    #[test]
    fn the_keypads_fill_their_grids() {
        assert_eq!(standard_keys(false).len(), 6 * 4);
        assert_eq!(standard_keys(true).len(), 6 * 4);
        assert_eq!(scientific_keys(false).len(), 5 * 9);
        assert_eq!(programmer_keys().len(), 3 * 6);
        assert_eq!(memory_keys().len(), 5);
    }

    #[test]
    fn the_second_key_swaps_the_function_row() {
        let first = scientific_keys(false);
        let second = scientific_keys(true);
        assert!(first.iter().any(|k| k.id == "key-sin"));
        assert!(second.iter().any(|k| k.id == "key-asin"));
        assert!(second.iter().any(|k| k.id == "key-cube"));
    }

    #[test]
    fn signs_are_spoken_as_words() {
        let keys = standard_keys(false);
        let divide = keys.iter().find(|k| k.id == "key-divide").unwrap();
        assert_eq!(spoken(divide), "Divide");
        let seven = keys.iter().find(|k| k.id == "key-7").unwrap();
        assert_eq!(spoken(seven), "7");
        assert!(key_css(seven, true, false).contains("EB Garamond"));
    }
}
