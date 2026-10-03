//! AzMonitor: the system monitor on the public azul API (skeleton; the
//! window follows).

/// The history of one measure: a ring of the last readings.
pub mod history;
/// The sampling model: readings, rates, the process rows, sort and filter.
pub mod model;

/// The app's start (the window follows).
pub fn start() {}
