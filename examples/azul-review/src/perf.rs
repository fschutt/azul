//! The ink's own timings, for the people who measure it (`AZREVIEW_PROFILE=1`): what a pointer move
//! costs in AzReview's handler, what drawing a frame of an ink layer costs, and what the engine says
//! its presented frames cost (`CallbackInfo::get_frame_timings`). Kept per stretch of drawing and
//! summed up in one line, `AZREVIEW_PERF ...`, when the stroke ends. A plain run keeps nothing.

/// The most samples one stretch keeps of each kind: a long stroke must not grow the buffers without end.
pub const MAX_SAMPLES: usize = 20_000;

/// The timings of one stretch of drawing.
#[derive(Debug, Default)]
pub struct Perf {
    on: bool,
    moves: Vec<u32>,
    draws: Vec<u32>,
    frame_work: Vec<u32>,
    frame_interval: Vec<u32>,
}

/// The value at the `q` (0 to 1) of `values` (sorted here); 0 for none.
#[must_use]
pub fn quantile(values: &mut [u32], q: f64) -> u32 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    // The nearest rank: the smallest value that at least `q` of the values are at or under.
    let rank = (q.clamp(0.0, 1.0) * values.len() as f64).ceil() as usize;
    values[rank.clamp(1, values.len()) - 1]
}

fn push(into: &mut Vec<u32>, value: u32) {
    if into.len() < MAX_SAMPLES {
        into.push(value);
    }
}

impl Perf {
    /// A recorder that keeps samples when `on`.
    #[must_use]
    pub fn new(on: bool) -> Self {
        Self {
            on,
            ..Self::default()
        }
    }

    /// Whether samples are kept (the callers skip taking the time when not).
    #[must_use]
    pub const fn is_on(&self) -> bool {
        self.on
    }

    /// A pointer move was handled in `us` microseconds.
    pub fn moved(&mut self, us: u32) {
        if self.on {
            push(&mut self.moves, us);
        }
    }

    /// A frame of an ink layer was drawn in `us` microseconds.
    pub fn drawn(&mut self, us: u32) {
        if self.on {
            push(&mut self.draws, us);
        }
    }

    /// The engine's last presented frame: its work and the interval since the one before.
    pub fn frame(&mut self, work_us: u32, interval_us: u32) {
        if self.on {
            push(&mut self.frame_work, work_us);
            push(&mut self.frame_interval, interval_us);
        }
    }

    /// The stretch in one line, and the next stretch begins; `None` when nothing was kept.
    pub fn summary(&mut self) -> Option<String> {
        if self.moves.is_empty() && self.draws.is_empty() && self.frame_work.is_empty() {
            return None;
        }
        let mut line = format!("AZREVIEW_PERF moves={}", self.moves.len());
        for (name, values) in [
            ("move_us", &mut self.moves),
            ("draw_us", &mut self.draws),
            ("frame_work_us", &mut self.frame_work),
            ("frame_interval_us", &mut self.frame_interval),
        ] {
            let n = values.len();
            line.push_str(&format!(
                " {name}_n={n} {name}_p50={} {name}_p99={} {name}_max={}",
                quantile(values, 0.5),
                quantile(values, 0.99),
                quantile(values, 1.0),
            ));
            values.clear();
        }
        Some(line)
    }
}

#[cfg(test)]
#[path = "perf_test.rs"]
mod perf_test;
