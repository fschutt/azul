//! AzMonitor's DOM ids, classes and markers, each name defined ONCE, every
//! one carrying the app's `__azmonitor_` prefix (user ruling 2026-10-02, like
//! the widgets' `__azul_`): no clash with a widget's or another app's names,
//! and no string copied at run time for a fixed name.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azmonitor_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azmonitor_", $value));)*
    };
}

names! {
    // ---- the tool row ----
    /// The tab row (Processes, Performance) and the tools beside it.
    TOOLS = "tools";
    /// The filter field.
    FILTER = "filter";
    /// "End process".
    END_PROCESS = "end-process";

    // ---- the live views (VirtualViews a tick re-renders; id and marker) ----
    /// The cards strip over the table.
    CARDS = "cards";
    /// The process table's view.
    TABLE_VIEW = "table-view";
    /// The process table itself (the DataTable's id).
    TABLE = "processes";
    /// The performance page.
    PERFORMANCE = "performance";
    /// The empty state until the first reading.
    WAITING = "waiting";

    // ---- the cards ----
    CARD_CPU = "card-cpu";
    CARD_MEMORY = "card-memory";
    CARD_DISK = "card-disk";
    CARD_NETWORK = "card-network";
    /// A card's class.
    CARD = "card";
    /// A card's headline (the measure and its value).
    CARD_VALUE = "card-value";

    // ---- the performance page ----
    CHART_CPU = "chart-cpu";
    CHART_MEMORY = "chart-memory";
    CHART_DISK = "chart-disk";
    CHART_NETWORK = "chart-network";
    /// The per-core bars.
    CORES = "cores";
    /// One core's row (class).
    CORE = "core";
    /// The stats block (uptime, processes, memory, swap).
    STATS = "stats";

    // ---- the status bar's live labels (markers) ----
    STATUS_PROCESSES = "status-processes";
    STATUS_CPU = "status-cpu";
    STATUS_MEMORY = "status-memory";
    STATUS_NOTICE = "status-notice";
    STATUS_SPEED = "status-speed";

    // ---- the end-process question ----
    /// The question's modal.
    CONFIRM = "confirm-end";

    // ---- the settings ----
    /// The update speed control.
    SPEED = "speed";
    /// "Export the last minute".
    EXPORT = "export";
}
