//! AzMonitor: the system monitor on the public azul API (skeleton; the
//! window follows).

/// The history of one measure: a ring of the last readings.
pub mod history;
/// The live machine: this computer through the `sysinfo` crate.
pub mod live;
/// The sampling model: readings, rates, the process rows, sort and filter.
pub mod model;
/// The sample machine (`--sample`): a deterministic system.
pub mod sample;

/// The app's start (the window follows).
pub fn start() {}
