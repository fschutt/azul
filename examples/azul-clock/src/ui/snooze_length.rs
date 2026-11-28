//! The snooze length of an alarm in the editor: one step of a short list of lengths either way.
//!
//! It is no text field. On a phone the keyboard covers the lower part of the page a field sits in,
//! and a length of a few minutes is a choice from a few, not a number to type: the stepper is the
//! same `- 10 min +` the settings use for "Ring for".

/// The lengths the editor steps through, shortest first; each is within `1..=MAX_SNOOZE_MINUTES`.
pub const SNOOZE_MINUTES: [u32; 7] = [1, 5, 10, 15, 20, 30, 60];

/// The length one step `delta` from `minutes` (below zero shorter, above zero longer): the neighbour
/// on that side in [`SNOOZE_MINUTES`]. A length that is not in the list (an alarm another version
/// saved with 7 minutes) steps to the entry beside it; at either end the length stays, and a step of
/// zero changes nothing.
#[must_use]
pub fn stepped(minutes: u32, delta: i32) -> u32 {
    match delta.signum() {
        -1 => SNOOZE_MINUTES.iter().rev().copied().find(|m| *m < minutes).unwrap_or(minutes),
        1 => SNOOZE_MINUTES.iter().copied().find(|m| *m > minutes).unwrap_or(minutes),
        _ => minutes,
    }
}

/// Whether there is a step from `minutes` in the direction of `delta` (the end of the list is none).
#[must_use]
pub fn can_step(minutes: u32, delta: i32) -> bool {
    stepped(minutes, delta) != minutes
}

#[cfg(test)]
#[path = "snooze_length_test.rs"]
mod snooze_length_test;
