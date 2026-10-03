//! The calculator's DOM ids and classes, each name defined ONCE, every one
//! carrying the app's `__azcalc_` prefix (user ruling 2026-10-02, like the
//! widgets' `__azul_`): no clash with a widget's or another app's names, and
//! no string copied at run time for a fixed name.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azcalc_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azcalc_", $value));)*
    };
}

names! {
    // ---- the calculator screen ----
    /// The display (expression over result).
    DISPLAY = "display";
    EXPRESSION = "expression";
    RESULT = "result";
    NOTICE = "notice";
    /// The keypad grid of the screen's mode.
    KEYPAD = "keypad";
    /// The memory keys' row (Standard).
    MEMORY_ROW = "memory-row";
    /// Programmer: the HEX / DEC / OCT / BIN rows, the word size, the bits.
    BASES = "bases";
    WORD = "word";
    BITS = "bits";
    /// Scientific: DEG / RAD / GRAD.
    ANGLE = "angle";
    // ---- the side panel ----
    PANEL = "panel";
    PANEL_TABS = "panel-tabs";
    HISTORY = "history";
    /// Class of one history entry.
    HISTORY_ENTRY = "history-entry";
    CLEAR_HISTORY = "clear-history";
    MEMORY = "memory";
    /// Class of one memory entry.
    MEMORY_ENTRY = "memory-entry";
    // ---- the mode row ----
    MODES = "modes";
    TOGGLE_PANEL = "toggle-panel";
    SETTINGS = "settings";
    // ---- the date screen ----
    DATE_VIEW = "date-view";
    DATE_KIND = "date-kind";
    DATE_FROM = "date-from";
    DATE_TO = "date-to";
    DATE_SIGN = "date-sign";
    DATE_YEARS = "date-years";
    DATE_MONTHS = "date-months";
    DATE_DAYS = "date-days";
    DATE_TODAY = "date-today";
    DATE_RESULT = "date-result";
    // ---- the converter ----
    CONVERT_VIEW = "convert-view";
    CONV_CATEGORY = "conv-category";
    CONV_FROM_VALUE = "conv-from-value";
    CONV_FROM_UNIT = "conv-from-unit";
    CONV_SWAP = "conv-swap";
    CONV_TO_VALUE = "conv-to-value";
    CONV_TO_UNIT = "conv-to-unit";
    CONV_RATE = "conv-rate";
    CONV_RECENT = "conv-recent";
    // ---- the app's settings sections ----
    SET_GROUPING = "set-grouping";
    SET_ANGLE = "set-angle";
    SET_KEEP_HISTORY = "set-keep-history";
    SET_CLEAR_HISTORY = "set-clear-history";
}

/// A name made at run time from a part defined once elsewhere (a key of a
/// keypad table, a base row, a bit): the prefix, then `suffix`.
#[must_use]
pub fn named(suffix: &str) -> AzString {
    AzString::from(format!("{PREFIX}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_app_prefix() {
        for name in [DISPLAY, KEYPAD, HISTORY_ENTRY, DATE_FROM, CONV_TO_UNIT, SET_CLEAR_HISTORY] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        assert_eq!(named("key-7").as_str(), "__azcalc_key-7");
    }
}
