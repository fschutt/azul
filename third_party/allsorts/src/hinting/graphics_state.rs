use super::f26dot6::{F2Dot14, F26Dot6};

/// Rounding mode for the TrueType interpreter.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RoundState {
    /// Round to half grid (0.5 pixel boundaries).
    HalfGrid,
    /// Round to grid (integer pixel boundaries).
    Grid,
    /// Round to double grid (0.5 pixel boundaries, same as half grid but different phase).
    DoubleGrid,
    /// Round down to grid.
    DownToGrid,
    /// Round up to grid.
    UpToGrid,
    /// No rounding.
    Off,
    /// Super rounding (configurable period, phase, threshold).
    Super,
    /// Super 45-degree rounding.
    Super45,
}

/// Graphics state for the TrueType bytecode interpreter.
///
/// Contains all ~30 state variables that control how instructions behave.
/// Reset to defaults before each glyph program (but prep can modify defaults).
#[derive(Clone, Debug)]
pub struct GraphicsState {
    /// Freedom vector: direction along which points are moved.
    pub freedom_vector: (F2Dot14, F2Dot14),
    /// Projection vector: direction along which distances are measured.
    pub projection_vector: (F2Dot14, F2Dot14),
    /// Dual projection vector: used for measuring original outline distances.
    pub dual_projection_vector: (F2Dot14, F2Dot14),
    /// Reference point 0.
    pub rp0: u32,
    /// Reference point 1.
    pub rp1: u32,
    /// Reference point 2.
    pub rp2: u32,
    /// Zone pointer 0 (0 = twilight, 1 = glyph).
    pub zp0: u32,
    /// Zone pointer 1.
    pub zp1: u32,
    /// Zone pointer 2.
    pub zp2: u32,
    /// Loop variable: how many times certain instructions repeat.
    pub loop_value: u32,
    /// Rounding mode.
    pub round_state: RoundState,
    /// Minimum distance (F26Dot6): smallest distance after rounding.
    pub minimum_distance: F26Dot6,
    /// Control value cut-in (F26Dot6): threshold for using CVT vs actual distance.
    pub control_value_cut_in: F26Dot6,
    /// Single width cut-in (F26Dot6).
    pub single_width_cut_in: F26Dot6,
    /// Single width value (F26Dot6).
    pub single_width_value: F26Dot6,
    /// Auto flip: whether MIRP auto-corrects direction.
    pub auto_flip: bool,
    /// Delta base: ppem value at which DELTA instructions start.
    pub delta_base: u16,
    /// Delta shift: number of bits to shift DELTA arguments.
    pub delta_shift: u16,
    /// Instruction control flags.
    pub instruct_control: u8,
    /// Scan control flag.
    pub scan_control: u32,
    /// Scan type.
    pub scan_type: i32,

    // Super rounding parameters (used when round_state is Super or Super45)
    /// Super round period (F26Dot6).
    pub super_round_period: F26Dot6,
    /// Super round phase (F26Dot6).
    pub super_round_phase: F26Dot6,
    /// Super round threshold (F26Dot6).
    pub super_round_threshold: F26Dot6,
}

impl Default for GraphicsState {
    fn default() -> Self {
        GraphicsState {
            // Default vectors along x-axis
            freedom_vector: (F2Dot14::ONE, F2Dot14::ZERO),
            projection_vector: (F2Dot14::ONE, F2Dot14::ZERO),
            dual_projection_vector: (F2Dot14::ONE, F2Dot14::ZERO),
            rp0: 0,
            rp1: 0,
            rp2: 0,
            zp0: 1, // glyph zone
            zp1: 1,
            zp2: 1,
            loop_value: 1,
            round_state: RoundState::Grid,
            minimum_distance: F26Dot6::ONE,        // 1 pixel
            control_value_cut_in: F26Dot6(68),     // 17/16 pixel = 68/64
            single_width_cut_in: F26Dot6::ZERO,
            single_width_value: F26Dot6::ZERO,
            auto_flip: true,
            delta_base: 9,
            delta_shift: 3,
            instruct_control: 0,
            scan_control: 0,
            scan_type: 0,
            super_round_period: F26Dot6(64), // 1 pixel
            super_round_phase: F26Dot6::ZERO,
            super_round_threshold: F26Dot6(32), // 0.5 pixel (half of period)
        }
    }
}

impl GraphicsState {
    /// Apply rounding according to the current round_state.
    pub fn round(&self, distance: F26Dot6) -> F26Dot6 {
        let sign = if distance.0 >= 0 { 1i32 } else { -1i32 };
        let val = distance.abs();

        let result = match self.round_state {
            RoundState::Off => return distance,
            RoundState::Grid => val.round(),
            RoundState::HalfGrid => {
                // Round to nearest half pixel (n + 0.5)
                let floored = val.floor();
                F26Dot6(floored.0 + 32)
            }
            RoundState::DoubleGrid => {
                // Round to nearest half pixel
                F26Dot6((val.0 + 16) & !31)
            }
            RoundState::DownToGrid => val.floor(),
            RoundState::UpToGrid => {
                if val.0 & 63 == 0 {
                    val
                } else {
                    val.ceil()
                }
            }
            RoundState::Super | RoundState::Super45 => {
                self.super_round(val)
            }
        };

        // Ensure minimum distance of 0 after rounding (result is non-negative)
        let result = if result.0 < 0 { F26Dot6::ZERO } else { result };

        F26Dot6(result.0 * sign)
    }

    fn super_round(&self, val: F26Dot6) -> F26Dot6 {
        let period = self.super_round_period;
        let phase = self.super_round_phase;
        let threshold = self.super_round_threshold;

        if period.0 == 0 {
            return val;
        }

        let val_minus_phase = F26Dot6(val.0 - phase.0);

        let rounded = if val_minus_phase.0 >= 0 {
            let n = (val_minus_phase.0 + threshold.0) / period.0;
            F26Dot6(n * period.0 + phase.0)
        } else {
            let n = -((-val_minus_phase.0 + threshold.0) / period.0);
            F26Dot6(n * period.0 + phase.0)
        };

        if rounded.0 < phase.0 {
            F26Dot6(phase.0)
        } else {
            rounded
        }
    }

    /// Set the super rounding parameters from an opcode argument.
    ///
    /// `is_45` selects Super45 mode (period is sqrt(2)/2 instead of 1).
    pub fn set_super_round(&mut self, n: u32, is_45: bool) {
        // Period (bits 7-6)
        let period_bits = (n >> 6) & 0x03;
        self.super_round_period = match period_bits {
            0 => F26Dot6(32),  // 1/2 pixel
            1 => F26Dot6(64),  // 1 pixel
            2 => F26Dot6(128), // 2 pixels
            _ => F26Dot6(64),  // reserved, default to 1 pixel
        };

        if is_45 {
            // For 45-degree rounding, multiply period by sqrt(2)/2 ≈ 0.7071
            // In F26Dot6: period * 46 / 64 (approximation)
            self.super_round_period = F26Dot6(
                (self.super_round_period.0 as i64 * 46 / 64) as i32,
            );
        }

        // Phase (bits 5-4): derived from the actual period (period*{0,1/4,1/2,3/4}),
        // NOT hardcoded 16/32/48 which is only correct when period == 64.
        let period = self.super_round_period.0;
        let phase_bits = (n >> 4) & 0x03;
        self.super_round_phase = match phase_bits {
            0 => F26Dot6::ZERO,
            1 => F26Dot6(period / 4),
            2 => F26Dot6(period / 2),
            3 => F26Dot6(period * 3 / 4),
            _ => unreachable!(),
        };

        // Threshold (bits 3-0)
        let threshold_bits = n & 0x0F;
        self.super_round_threshold = if threshold_bits == 0 {
            F26Dot6(self.super_round_period.0 - 1)
        } else {
            F26Dot6((threshold_bits as i32 - 4) * self.super_round_period.0 / 8)
        };
    }
}
