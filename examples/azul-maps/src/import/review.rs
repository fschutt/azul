//! The review of an import's unplaced places: each title is looked up with the search, and a
//! single clear match is taken while anything less waits for the user to confirm or fix.

use super::Unplaced;

/// A match the search found for an unplaced place.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// What the search shows (`Brandenburger Tor, Berlin`).
    pub label: String,
    pub at: (f64, f64),
}

/// What the search's matches make of a title.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The match with this index is the place: taken without asking.
    Certain(usize),
    /// The user picks (or skips).
    Uncertain,
    /// The search found nothing.
    NotFound,
}

/// The text looked up for an unplaced place: its title, and its address where it has one.
#[must_use]
pub fn query_of(place: &Unplaced) -> String {
    let _ = place;
    String::new()
}

/// Whether `candidates` clearly name `title`: the first one's name is the title (without case
/// or accents) and no other match has that name too.
#[must_use]
pub fn verdict(title: &str, candidates: &[Candidate]) -> Verdict {
    let _ = (title, candidates);
    Verdict::NotFound
}
