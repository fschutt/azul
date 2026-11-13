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
    // ---- the tab row and the Processes tab's own row ----
    /// The tab row (Processes, Performance, Networking, Users).
    TOOLS = "tools";
    /// The row under the process table: the filter, "End Process".
    PROCESS_ACTIONS = "process-actions";
    /// The filter field.
    FILTER = "filter";
    /// "End Process".
    END_PROCESS = "end-process";

    // ---- the live views (VirtualViews a tick re-renders; id and marker) ----
    /// The process table's view.
    TABLE_VIEW = "table-view";
    /// The process table itself (the DataTable's id).
    TABLE = "processes";
    /// The Performance page.
    PERFORMANCE = "performance";
    /// The Networking page.
    NETWORKING = "networking";
    /// The Users page.
    USERS = "users";
    /// The empty state until the first reading.
    WAITING = "waiting";

    // ---- the graphs and meters (graph.rs) ----
    /// A history graph's black box (class).
    GRAPH = "graph";
    /// The strip of a graph that scrolls between readings (class).
    GRAPH_STRIP = "graph-strip";
    /// A usage meter (class).
    METER = "meter";
    /// A group box of a page (class).
    GROUP = "group";

    // ---- the Performance page ----
    /// The CPU usage meter's group.
    CPU_USAGE = "cpu-usage";
    /// The CPU usage history (one graph per core, or the whole CPU).
    CPU_HISTORY = "cpu-history";
    /// The memory meter's group.
    MEMORY_USAGE = "memory-usage";
    /// The memory usage history.
    MEMORY_HISTORY = "memory-history";
    /// The figures under the graphs (totals, memory, swap, disk).
    STATS = "stats";

    // ---- the Networking page ----
    /// The network utilization history.
    NETWORK_HISTORY = "network-history";
    /// The figures under it.
    NETWORK_STATS = "network-stats";

    // ---- the Users page ----
    /// The users table.
    USERS_TABLE = "users-table";
    /// One user's row (class).
    USER_ROW = "user-row";

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
