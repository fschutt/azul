//! `gasp` — Grid-fitting And Scan-conversion Procedure Table
//!
//! <https://learn.microsoft.com/en-us/typography/opentype/spec/gasp>

use crate::binary::read::{ReadBinary, ReadCtxt};
use crate::error::ParseError;

/// Flags indicating grid-fitting and anti-aliasing behavior for a ppem range.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GaspBehavior {
    pub flags: u16,
}

impl GaspBehavior {
    /// Use gridfitting (bytecode hinting).
    pub const GRIDFIT: u16 = 0x0001;
    /// Use grayscale rendering (anti-aliasing).
    pub const DOGRAY: u16 = 0x0002;
    /// Use gridfitting with ClearType symmetric smoothing.
    pub const SYMMETRIC_GRIDFIT: u16 = 0x0004;
    /// Use smoothing along multiple axes with ClearType.
    pub const SYMMETRIC_SMOOTHING: u16 = 0x0008;

    pub fn should_gridfit(self) -> bool {
        self.flags & Self::GRIDFIT != 0
    }

    pub fn should_antialias(self) -> bool {
        self.flags & Self::DOGRAY != 0
    }

    pub fn should_symmetric_gridfit(self) -> bool {
        self.flags & Self::SYMMETRIC_GRIDFIT != 0
    }

    pub fn should_symmetric_smoothing(self) -> bool {
        self.flags & Self::SYMMETRIC_SMOOTHING != 0
    }
}

/// A single range entry in the `gasp` table.
#[derive(Copy, Clone, Debug)]
pub struct GaspRange {
    /// Upper ppem limit for this range (inclusive).
    pub range_max_ppem: u16,
    /// Behavior flags for ppem values in this range.
    pub behavior: GaspBehavior,
}

/// The `gasp` (Grid-fitting And Scan-conversion Procedure) table.
///
/// Maps ppem size ranges to rendering behavior flags that control whether
/// bytecode hinting and/or anti-aliasing should be applied.
#[derive(Clone, Debug)]
pub struct GaspTable {
    pub version: u16,
    pub ranges: Vec<GaspRange>,
}

impl GaspTable {
    /// Look up the rendering behavior flags for a given ppem size.
    ///
    /// Scans the ranges in order and returns the flags for the first range
    /// whose `range_max_ppem` is >= the given ppem. If no range matches,
    /// returns default flags (gridfit + dogray).
    pub fn rendering_flags(&self, ppem: u16) -> GaspBehavior {
        for range in &self.ranges {
            if ppem <= range.range_max_ppem {
                return range.behavior;
            }
        }
        // Default: enable everything
        GaspBehavior {
            flags: GaspBehavior::GRIDFIT | GaspBehavior::DOGRAY,
        }
    }
}

impl ReadBinary for GaspTable {
    type HostType<'a> = GaspTable;

    fn read<'a>(ctxt: &mut ReadCtxt<'a>) -> Result<Self::HostType<'a>, ParseError> {
        let version = ctxt.read_u16be()?;
        ctxt.check(version <= 1)?;
        let num_ranges = ctxt.read_u16be()? as usize;
        let mut ranges = Vec::with_capacity(num_ranges);
        for _ in 0..num_ranges {
            let range_max_ppem = ctxt.read_u16be()?;
            let flags = ctxt.read_u16be()?;
            ranges.push(GaspRange {
                range_max_ppem,
                behavior: GaspBehavior { flags },
            });
        }
        Ok(GaspTable { version, ranges })
    }
}
