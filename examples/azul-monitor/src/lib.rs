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
/// The sampler: an azul Thread that reads the system once a second.
pub mod sampler;
/// What a tick redraws (the live views, never the page).
pub mod ticks;

/// The app's start (the window follows).
pub fn start() {}
