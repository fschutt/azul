//! The opaque ids and the one unit of time of the report engine.
//!
//! Implements planning/architecture.md section 3.9: "the notifier gets a receipt number at once"
//! ([`ReportId`]), "counting is per link and per account, from reports only, and no content is looked at"
//! ([`LinkRef`], [`AccountRef`], [`DeviceRef`]: opaque bytes, never a name or an address) and the 30 days of the
//! automatic steps ([`Day`], [`Days`]).
//!
//! AZUL OVERRIDE of the common rules (brief section 7): this crate has no `azlin-proto`, so its units are its
//! own newtypes. A time never travels as a bare `u32`: a point is a [`Day`] (days since 1970-01-01 UTC, the
//! unit of D37: "not_before and expires_at are day-start times"), a span is [`Days`].
//!
//! Caller: S46-INT-2 (the leader task that feeds the ledger builds the refs and the days), S46-INT-3 (AzCtl's
//! Reports page reads `ReportId` and `Day`), S46-INT-4 (AzDrive's report dialog makes the `ReportId`).

use core::fmt;

/// Writes `bytes` as lower-case hex.
fn hex(f: &mut fmt::Formatter<'_>, bytes: &[u8]) -> fmt::Result {
    for b in bytes {
        write!(f, "{b:02x}")?;
    }
    Ok(())
}

macro_rules! opaque_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name([u8; 16]);

        impl $name {
            /// The id with these 16 bytes (opaque to this crate: it only compares them).
            pub const fn from_bytes(bytes: [u8; 16]) -> $name {
                $name(bytes)
            }

            /// The 16 bytes.
            pub const fn as_bytes(&self) -> &[u8; 16] {
                &self.0
            }
        }
    };
}

opaque_id!(
    /// The receipt number of a report (planning/architecture.md section 3.9: "the notifier gets a receipt number
    /// at once"; Art. 16(4) DSA). Random, made by the reporter's app BEFORE the drop, so that it exists when the
    /// request answers `ok` (drift X-DELTA-W1S46A-6: the plan does not say who makes it).
    ///
    /// Caller: S46-INT-4 (AzDrive's report dialog makes it), S46-INT-3 (AzCtl's Reports page keys the queue by it).
    ReportId
);

opaque_id!(
    /// A link, as the registry names it: opaque to the ledger (it counts per link and never looks at what the
    /// link holds).
    ///
    /// Caller: S46-INT-2 (the leader task, from the registry's link id).
    LinkRef
);

opaque_id!(
    /// An account, as the registry names it: a pseudonymous id, never a name, an address or a payer.
    ///
    /// Caller: S46-INT-2 (the leader task, from the registry's account id).
    AccountRef
);

opaque_id!(
    /// A reporting device: a PSEUDONYM the caller derives from the device's credential (never an address: the
    /// ledger must be able to count devices without learning who or where they are). Its `Debug` prints nothing
    /// of the bytes.
    ///
    /// Caller: S46-INT-2 (the leader task derives it from the credential of the request's device).
    DeviceRef
);

impl fmt::Debug for ReportId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ReportId(")?;
        hex(f, &self.0)?;
        f.write_str(")")
    }
}

impl fmt::Debug for LinkRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LinkRef(")?;
        hex(f, &self.0)?;
        f.write_str(")")
    }
}

impl fmt::Debug for AccountRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AccountRef(")?;
        hex(f, &self.0)?;
        f.write_str(")")
    }
}

impl fmt::Debug for DeviceRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("DeviceRef(..)")
    }
}

/// A day: days since 1970-01-01 UTC (a local newtype: this crate has no clock). Whoever has the clock (the
/// leader task, AzCtl) makes it; the engine only compares and adds days.
///
/// Caller: S46-INT-2, S46-INT-3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Day(u32);

/// A span of days: a window, a deadline.
///
/// Caller: S46-INT-2 (the windows of `Config`), S46-INT-3 (the due dates of the Reports page).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Days(u32);

impl Day {
    /// The day `n` days after 1970-01-01.
    pub const fn new(n: u32) -> Day {
        Day(n)
    }

    /// The number of days since 1970-01-01.
    pub const fn get(self) -> u32 {
        self.0
    }

    /// This day plus `span`. The sum stops at the last day a `u32` can hold (a day that no clock reaches: the
    /// year 11,761,191), so it never wraps and never panics (drift X-DELTA-W1S46A-15: a typed span).
    pub const fn plus(self, span: Days) -> Day {
        Day(self.0.saturating_add(span.0))
    }

    /// How many days this day is after `earlier`: `None` when `earlier` is the later one.
    pub const fn since(self, earlier: Day) -> Option<Days> {
        match self.0.checked_sub(earlier.0) {
            Some(n) => Some(Days(n)),
            None => None,
        }
    }

    /// Whether `earlier` is still inside a window of `window` days that ends on this day: a day counts through
    /// `earlier + window - 1` inclusive and no longer on `earlier + window` (a window of 30 days: a report on
    /// day d counts on d..=d+29 and not on d+30). A day that is LATER than this one has not left any window, so
    /// it counts.
    pub const fn within(self, earlier: Day, window: Days) -> bool {
        match self.0.checked_sub(earlier.0) {
            Some(age) => age < window.0,
            None => true,
        }
    }
}

impl Days {
    /// A span of `n` days.
    pub const fn new(n: u32) -> Days {
        Days(n)
    }

    /// The number of days.
    pub const fn get(self) -> u32 {
        self.0
    }
}
