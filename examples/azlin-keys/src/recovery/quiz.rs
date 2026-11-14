//! The written-down quiz: at setup the app checks that the recovery code was really written down by
//! asking for 4 of its 5 groups (planning/architecture.md section 5.5 "Recovery": "asking for 4 of
//! its 5 groups"; build unit S15, lane W1-S15d).
//!
//! The groups are numbered 1 to 5 as the text shows them (`XXXXX-XXXXX-XXXXX-XXXXX-XXXXXX`: groups
//! of 5, 5, 5, 5 and 6 symbols, DRIFT X-DELTA-W1S15D-2). The quiz leaves out one group, chosen from
//! the entropy seam by rejection sampling (every group is equally likely to be the one left out),
//! and asks for the other four in ascending order. An answer is normalised like the code's text is
//! read (case, spaces and hyphens, `O` for `0`, `I` and `L` for `1`) and compared with the group in
//! constant time; all four are compared before the first wrong one is named, so the time does not
//! say which was wrong (DRIFT X-DELTA-W1S15D-17: the brief leaves the numbering and the order open).

use core::fmt;

use subtle::ConstantTimeEq;
use zeroize::Zeroizing;

use super::code::{group_of, normalise_char, symbols_of, RecoveryCode};
use super::error::RecoveryError;
use crate::error::EntropyError;
use crate::seam::Entropy;

/// The groups of a recovery code's text: 5.
///
/// Caller: `Quiz`.
pub const QUIZ_GROUPS: u8 = 5;

/// The groups a quiz asks for: 4.
///
/// Caller: `Quiz`.
pub const QUIZ_ASKED: usize = 4;

/// The most symbols an answer is read for (the longest group has 6): a longer answer is wrong
/// without being copied.
const MAX_ANSWER_SYMBOLS: usize = 32;

/// The most bytes drawn to choose the group that is left out: a byte of 255 or more is rejected
/// (255 is 5 times 51), so the chance of the limit is 2^-512.
const MAX_DRAWS: usize = 64;

/// Why a quiz answer is not right: the number of the first group (1 to 5) that is wrong. A swapped
/// pair, a missing group, a group of another code and a typo all name the first group they spoil.
///
/// Caller: INT-S15-1 (the setup screen says which group to look at again).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuizFailure {
    /// This group (1 to 5) was answered wrongly.
    Wrong(u8),
}

impl fmt::Display for QuizFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QuizFailure::Wrong(group) => write!(f, "group {group} is not right"),
        }
    }
}

impl std::error::Error for QuizFailure {}

/// A quiz: four of the five groups, in ascending order.
///
/// Caller: INT-S15-1 (the setup screen asks `positions()`, then calls `check`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quiz {
    positions: [u8; QUIZ_ASKED],
}

/// One number 0 to 4, chosen uniformly from the seam: a byte below 255, modulo 5.
fn draw_below_five(ent: &mut dyn Entropy) -> Result<u8, RecoveryError> {
    for _ in 0..MAX_DRAWS {
        let mut draw = [0u8; 1];
        ent.fill(&mut draw)?;
        let [byte] = draw;
        if byte < 255 {
            return Ok(byte % 5);
        }
    }
    Err(RecoveryError::Entropy(EntropyError::Exhausted))
}

/// An answer as a string of canonical symbols (a character that is no symbol becomes the byte 0,
/// which no group holds), in a buffer that is wiped when dropped. An answer with more than
/// `MAX_ANSWER_SYMBOLS` symbols becomes the single byte 0.
fn normalise(answer: &str) -> Zeroizing<Vec<u8>> {
    let mut out = Zeroizing::new(Vec::with_capacity(MAX_ANSWER_SYMBOLS));
    let mut too_long = false;
    for c in answer.chars() {
        if c == '-' || c.is_whitespace() {
            continue;
        }
        if out.len() >= MAX_ANSWER_SYMBOLS {
            too_long = true;
            break;
        }
        out.push(normalise_char(c));
    }
    if too_long {
        out.clear();
        out.push(0);
    }
    out
}

impl Quiz {
    /// A quiz that leaves out one group, drawn from the seam (the same entropy gives the same quiz).
    /// On an entropy failure there is no quiz.
    ///
    /// Caller: INT-S15-1 (account setup).
    pub fn new(ent: &mut dyn Entropy) -> Result<Quiz, RecoveryError> {
        let left_out = draw_below_five(ent)?;
        let mut positions = [0u8; QUIZ_ASKED];
        let mut slots = positions.iter_mut();
        for group in 1..=QUIZ_GROUPS {
            if group - 1 == left_out {
                continue;
            }
            if let Some(slot) = slots.next() {
                *slot = group;
            }
        }
        Ok(Quiz { positions })
    }

    /// The groups asked for, 1 to 5, in ascending order: four different numbers.
    ///
    /// Caller: INT-S15-1 (the screen shows "group 1", "group 2", ...).
    pub fn positions(&self) -> [u8; QUIZ_ASKED] {
        self.positions
    }

    /// Checks the four answers, given in the order of [`Quiz::positions`], against `code`: all four
    /// must be right. The first wrong one is named.
    ///
    /// Caller: INT-S15-1 (account setup: the account is not finished until this passes).
    pub fn check(
        &self,
        code: &RecoveryCode,
        answers: &[&str; QUIZ_ASKED],
    ) -> Result<(), QuizFailure> {
        let symbols = symbols_of(code.as_secret());
        let mut first_wrong: Option<u8> = None;
        for (position, answer) in self.positions.iter().zip(answers.iter()) {
            let given = normalise(answer);
            let right = match group_of(symbols.as_slice(), *position) {
                Some(group) => bool::from(given.as_slice().ct_eq(group)),
                None => false,
            };
            if !right && first_wrong.is_none() {
                first_wrong = Some(*position);
            }
        }
        match first_wrong {
            None => Ok(()),
            Some(position) => Err(QuizFailure::Wrong(position)),
        }
    }
}
