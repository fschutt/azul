//! AzDashboard's DOM ids and classes, each name defined ONCE, every one
//! carrying the app's `__azdash_` prefix (user ruling 2026-10-02, like the
//! widgets' `__azul_`): no clash with a widget's or another app's names, and
//! no string copied at run time for a fixed name.
//!
//! The chart half (CHART7) adds its own names at the end, under its banner.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azdash_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azdash_", $value));)*
    };
}

names! {
    // ---- the table half ----
    /// The orders table (the DataTable's id and its marker).
    TABLE = "orders";
    /// The tool row over the table (the RecordsShell's tab row).
    TOOLS = "tools";
    /// "Clear filters".
    CLEAR_FILTERS = "clear-filters";
    /// "Clear sort".
    CLEAR_SORT = "clear-sort";
    /// The status bar's segments.
    STATUS_ROWS = "status-rows";
    STATUS_SHOWN = "status-shown";
    STATUS_SORT = "status-sort";
    STATUS_NOTICE = "status-notice";
    /// The empty state while the orders are generated.
    LOADING = "loading";
    /// The block the charts go into (the RecordsShell's cards strip).
    CHARTS = "charts";
}
