//! The engine's own lines about its renderer, and the verdict on them. std only (no azul types): tested on every host.
//!
//! At `AZ_LOG=info` the engine writes (Android: logcat tag `RustStdoutStderr`; the app may read its OWN lines):
//!
//! ```text
//! [render] backend=<cpu|gpu> requested=<cpu|gpu|auto> device=<GL_RENDERER text, or 'software' for the CPU renderer>
//! [render] gpu unavailable at <stage>: <cause>; drawing on the CPU          (BEFORE the backend=cpu line)
//! [render] first GPU frame: <ms> ms
//! [render] frames n=<N> over_ms=<ms> interval_ms median=<m> p95=<p> max=<x> draw_ms median=<m> p95=<p>
//! ```
//!
//! The row asks one question of them: which renderer ran, on which driver, how long does a frame take; and holds the engine
//! to what the app asked for: a GPU that was requested and a CPU that ran is a FAILURE with the engine's own reason.

/// What the `backend=` line says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Backend {
    /// `cpu` or `gpu`: the renderer that RAN.
    pub ran: String,
    /// `cpu`, `gpu` or `auto`: what was asked for.
    pub requested: String,
    /// The driver (`GL_RENDERER`) or `software`.
    pub device: String,
}

/// What a `frames` line says, in milliseconds.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frames {
    /// Frames in the second the line covers.
    pub n: u64,
    /// The time between two frames: median, 95th percentile, worst.
    pub interval_median: f64,
    /// 95th percentile of the interval.
    pub interval_p95: f64,
    /// Worst interval.
    pub interval_max: f64,
    /// The time a frame took to draw: median.
    pub draw_median: f64,
    /// 95th percentile of the draw time.
    pub draw_p95: f64,
}

/// What the lines say, judged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Judged {
    /// A backend line exists and is what was asked for; the text names renderer, driver and frame times.
    Pass(String),
    /// The GPU was requested and the CPU ran (the engine's reason is in the text).
    Fail(String),
    /// No `backend=` line at all (the engine was not asked to log at `info`).
    NoLines,
}

fn after<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let at = line.find(key)?;
    Some(&line[at + key.len()..])
}

fn token<'a>(text: &'a str) -> &'a str {
    text.split_whitespace().next().unwrap_or("")
}

/// The `[render] backend=...` line, wherever the engine's own prefix (`[ 12us][Info][Rendering]`) puts it.
#[must_use]
pub fn parse_backend(line: &str) -> Option<Backend> {
    let rest = after(line, "[render] backend=")?;
    let ran = token(rest).to_string();
    let requested = token(after(rest, "requested=")?).to_string();
    let device = after(rest, "device=")?.trim().to_string();
    if ran.is_empty() || requested.is_empty() {
        return None;
    }
    Some(Backend { ran, requested, device })
}

fn number(text: &str, key: &str) -> Option<f64> {
    token(after(text, key)?).parse().ok()
}

/// The `[render] frames n=...` line.
#[must_use]
pub fn parse_frames(line: &str) -> Option<Frames> {
    let rest = after(line, "[render] frames ")?;
    let interval = after(rest, "interval_ms ")?;
    let draw = after(rest, "draw_ms ")?;
    Some(Frames {
        n: number(rest, "n=")? as u64,
        interval_median: number(interval, "median=")?,
        interval_p95: number(interval, "p95=")?,
        interval_max: number(interval, "max=")?,
        draw_median: number(draw, "median=")?,
        draw_p95: number(draw, "p95=")?,
    })
}

/// The engine's reason for not using the GPU: what follows `gpu unavailable at `, without the line's own prefix.
#[must_use]
pub fn fallback_cause(lines: &[String]) -> Option<String> {
    lines
        .iter()
        .rev()
        .find_map(|line| after(line, "[render] gpu unavailable at ").map(|cause| format!("gpu unavailable at {}", cause.trim())))
}

/// The cost of the first GPU frame (WebRender compiles its programs in it), in ms.
#[must_use]
pub fn first_gpu_frame_ms(lines: &[String]) -> Option<f64> {
    lines.iter().rev().find_map(|line| number(after(line, "[render] first GPU frame: ")?, ""))
}

/// The verdict on `lines` (the `[render]` lines of the app's own log, in order). `app_side` is what the app measured itself
/// (`get_frame_timings`), appended to the text.
#[must_use]
pub fn judge(lines: &[String], app_side: &str) -> Judged {
    let Some(backend) = lines.iter().rev().find_map(|line| parse_backend(line)) else {
        return Judged::NoLines;
    };
    let mut text = format!("{} (requested {}) on {}", backend.ran, backend.requested, backend.device);
    if let Some(ms) = first_gpu_frame_ms(lines) {
        text.push_str(&format!("; first GPU frame {ms:.0} ms"));
    }
    if let Some(f) = lines.iter().rev().find_map(|line| parse_frames(line)) {
        text.push_str(&format!(
            "; {} frames in the last second: interval median {:.1} / p95 {:.1} / max {:.1} ms, draw median {:.1} / p95 {:.1} ms",
            f.n, f.interval_median, f.interval_p95, f.interval_max, f.draw_median, f.draw_p95
        ));
    }
    if !app_side.is_empty() {
        text.push_str("; ");
        text.push_str(app_side);
    }
    if backend.requested == "gpu" && backend.ran == "cpu" {
        let cause = fallback_cause(lines).unwrap_or_else(|| "no 'gpu unavailable at' line before it".to_string());
        return Judged::Fail(format!("requested gpu, ran cpu: {cause}; {text}"));
    }
    Judged::Pass(text)
}

/// A list of timings (microseconds) as `median / p95 / max` in milliseconds, or `None` for no sample.
#[must_use]
pub fn spread_ms(samples_us: &[u64]) -> Option<(f64, f64, f64)> {
    if samples_us.is_empty() {
        return None;
    }
    let mut sorted = samples_us.to_vec();
    sorted.sort_unstable();
    let at = |q: f64| sorted[(((sorted.len() - 1) as f64) * q).round() as usize] as f64 / 1000.0;
    Some((at(0.5), at(0.95), at(1.0)))
}

#[cfg(test)]
#[path = "render_lines_test.rs"]
mod tests;
