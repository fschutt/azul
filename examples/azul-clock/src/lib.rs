//! AzClock: alarms, timers, a stopwatch and a world clock (the plan:
//! azul-apps/planning/core/clock.md).
//!
//! The model is plain Rust, tested without a window:
//! - [`alarm`]: an alarm (a time of day, an RRULE repeat, snooze), its
//!   occurrences as instants - DST-correct in any zone;
//! - [`tone`]: the alarm sounds, synthesised into PCM for azul's AudioSink.

pub mod alarm;
/// The DOM ids and classes (`__azclock_` prefix), each defined once.
pub mod ids;
pub mod tone;

/// Starts AzClock (the switches are read from the command line).
pub fn start() {}
