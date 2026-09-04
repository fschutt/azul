//! Form constraint validation - the shared vocabulary for `Invalid`.
//!
//! The RULES live in `azul_layout::form`, because they need the DOM and the
//! text pipeline. What lives here is the ANSWER: which constraints a control
//! failed, in a shape that crosses the C ABI and that a callback can read.
//!
//! # Why this is a manager and not an `EventData` variant
//!
//! The obvious design - and the one this item was originally filed against -
//! is a new `EventData::Validity(..)`. It does not work: `CallbackInfo` never
//! sees the `SyntheticEvent`. It carries the hit node and read-only access to
//! the `LayoutWindow`, and every other event payload an app can actually read
//! is parked in a manager and fetched by an accessor (`peek_raw_motion` is the
//! same shape). An `EventData` variant would have been an ABI addition that
//! no application could observe.

use alloc::{collections::BTreeMap, vec::Vec};

use crate::dom::DomNodeId;

/// Why a control failed validation.
///
/// Mirrors the flags HTML's `ValidityState` exposes, minus the ones no
/// attribute azul understands can produce. New reasons are APPENDED - the
/// discriminant is a bit position in [`ValidityState`], so renumbering would
/// silently change what a stored state means.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum ValidityReason {
    /// `required` and the value is empty.
    ValueMissing = 0,
    /// Shorter than `minlength`.
    TooShort = 1,
    /// Longer than `maxlength`.
    TooLong = 2,
    /// Below `min`.
    RangeUnderflow = 3,
    /// Above `max`.
    RangeOverflow = 4,
    /// Does not match `pattern` (11b-i-b). The whole value must match, and an
    /// empty value is exempt - both exactly as HTML's `patternMismatch`.
    PatternMismatch = 5,
}

impl ValidityReason {
    /// This reason's bit in a [`ValidityState`].
    #[must_use]
    pub const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

/// Every constraint one control failed, at once.
///
/// A SET and not a single reason, because HTML's `ValidityState` is a set:
/// one field can be both too short and out of range, and reporting only the
/// first would make the second appear only after the first was fixed.
///
/// A bitset rather than a struct of bools so that appending a reason - the
/// `PatternMismatch` 11b-i-b added - stays a pure enum append with no
/// change to this type's layout. The same trade `GamepadState::buttons` makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct ValidityState {
    /// Bit `n` set means the [`ValidityReason`] with discriminant `n` failed.
    /// Read it through [`Self::has`] rather than by hand.
    pub flags: u32,
}

impl ValidityState {
    /// A control that passed every constraint.
    #[must_use]
    pub const fn valid() -> Self {
        Self { flags: 0 }
    }

    /// Did this control fail `reason`?
    #[must_use]
    pub const fn has(self, reason: ValidityReason) -> bool {
        self.flags & reason.bit() != 0
    }

    /// Did it pass everything?
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.flags == 0
    }

    /// Record a failure.
    pub const fn insert(&mut self, reason: ValidityReason) {
        self.flags |= reason.bit();
    }
}

/// The outcome of the last constraint validation, so a callback can ask why
/// its control was rejected.
///
/// Replaced WHOLESALE on each validation rather than merged, which is what
/// makes a control that has since been fixed stop reporting: it simply is not
/// in the new map.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormValidationManager {
    failures: BTreeMap<DomNodeId, ValidityState>,
}

impl FormValidationManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish the result of one validation pass.
    pub fn set_failures(&mut self, failures: impl IntoIterator<Item = (DomNodeId, ValidityState)>) {
        self.failures = failures.into_iter().collect();
    }

    /// Why this control failed, or [`ValidityState::valid`] if it did not.
    ///
    /// A control nothing has validated yet reads as VALID rather than as
    /// unknown. That is the honest answer for a form nobody has submitted -
    /// HTML says the same, since an untouched field is valid until a
    /// constraint check says otherwise.
    #[must_use]
    pub fn state_of(&self, node: DomNodeId) -> ValidityState {
        self.failures
            .get(&node)
            .copied()
            .unwrap_or_else(ValidityState::valid)
    }

    /// Every control that failed the last validation, in document order.
    #[must_use]
    pub fn failing_nodes(&self) -> Vec<DomNodeId> {
        self.failures.keys().copied().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{dom::DomId, styled_dom::NodeHierarchyItemId};

    fn node(i: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::from_crate_internal(Some(crate::dom::NodeId::new(i))),
        }
    }

    /// The discriminants ARE bit positions. Renumbering the enum would leave
    /// every stored state meaning something else, with nothing failing to
    /// compile - the same hazard the sensor and IME wire codes have.
    #[test]
    fn the_reason_discriminants_are_bit_positions_and_are_all_distinct() {
        let all = [
            ValidityReason::ValueMissing,
            ValidityReason::TooShort,
            ValidityReason::TooLong,
            ValidityReason::RangeUnderflow,
            ValidityReason::RangeOverflow,
        ];
        let mut seen = 0u32;
        for r in all {
            assert_ne!(r.bit(), 0, "{r:?} has no bit");
            assert_eq!(seen & r.bit(), 0, "{r:?} shares a bit with an earlier reason");
            seen |= r.bit();
        }
        // Pinned so that APPENDING a reason is the only edit that passes:
        // reordering changes this number.
        assert_eq!(seen, 0b11111);
        assert_eq!(ValidityReason::ValueMissing.bit(), 1);
    }

    #[test]
    fn a_state_holds_several_failures_at_once() {
        let mut s = ValidityState::valid();
        assert!(s.is_valid());

        s.insert(ValidityReason::TooShort);
        s.insert(ValidityReason::RangeOverflow);
        assert!(!s.is_valid());
        assert!(s.has(ValidityReason::TooShort));
        assert!(s.has(ValidityReason::RangeOverflow));
        assert!(
            !s.has(ValidityReason::ValueMissing),
            "a reason nobody inserted must not read as failed"
        );

        // Idempotent: validating twice must not double-count anything.
        let before = s.flags;
        s.insert(ValidityReason::TooShort);
        assert_eq!(s.flags, before);
    }

    #[test]
    fn an_unvalidated_control_reads_as_valid_and_a_new_pass_clears_the_old_one() {
        let mut m = FormValidationManager::new();
        assert!(m.state_of(node(1)).is_valid());
        assert!(m.failing_nodes().is_empty());

        let mut bad = ValidityState::valid();
        bad.insert(ValidityReason::ValueMissing);
        m.set_failures([(node(1), bad)]);
        assert!(m.state_of(node(1)).has(ValidityReason::ValueMissing));
        assert_eq!(m.failing_nodes(), alloc::vec![node(1)]);

        // A second pass in which node 1 now passes and node 2 does not: the
        // old entry must be GONE, or a field the user just fixed would keep
        // reporting the error it no longer has.
        m.set_failures([(node(2), bad)]);
        assert!(m.state_of(node(1)).is_valid());
        assert!(m.state_of(node(2)).has(ValidityReason::ValueMissing));
    }
}
