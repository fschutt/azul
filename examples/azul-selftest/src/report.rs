//! What the probes found and how it is written down. std only (no azul types): every host runs the
//! tests of this file (`report_test.rs`), a phone or not.
//!
//! One verdict per probe, and ONE rule about which of them is a failure: a device that is unavailable,
//! a permission that was refused and a thing that needs a person are NOT failures; a capability that
//! says it is available and then does not work IS.

/// What one probe found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// The real operation ran and answered what it should; the text holds the numbers.
    Pass(String),
    /// The device (or the target) has no such thing: not a failure.
    Unavailable(String),
    /// The platform said "not allowed" (the permission was refused or never granted): not a failure.
    Denied(String),
    /// The capability said it is available and then did not work: a bug.
    Fail(String),
    /// A person has to do something, written exactly: neither a pass nor a failure.
    NeedsHuman(String),
    /// There is no probe for it yet: the row exists so that the capability is not forgotten.
    Pending(String),
}

impl Verdict {
    /// The word in the list and in the report.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Pass(_) => "PASS",
            Self::Unavailable(_) => "UNAVAILABLE",
            Self::Denied(_) => "DENIED",
            Self::Fail(_) => "FAIL",
            Self::NeedsHuman(_) => "NEEDS-HUMAN",
            Self::Pending(_) => "PENDING",
        }
    }

    /// The numbers, the reason or the instruction.
    #[must_use]
    pub fn detail(&self) -> &str {
        match self {
            Self::Pass(s)
            | Self::Unavailable(s)
            | Self::Denied(s)
            | Self::Fail(s)
            | Self::NeedsHuman(s)
            | Self::Pending(s) => s,
        }
    }

    /// Whether the exit code is non-zero because of it.
    #[must_use]
    pub const fn is_failure(&self) -> bool {
        matches!(self, Self::Fail(_))
    }
}

/// The answer of one row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowResult {
    /// `group.name`, stable: a harness asserts on it.
    pub id: String,
    /// The group the row belongs to (device, sensors, security, system, media ...).
    pub group: String,
    /// What the list shows.
    pub title: String,
    /// What it found.
    pub verdict: Verdict,
    /// How long the probe ran.
    pub ms: u64,
}

/// Who ran the report, for the first fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Context {
    /// The app.
    pub app: String,
    /// Its version.
    pub version: String,
    /// `std::env::consts::OS`.
    pub platform: String,
    /// `std::env::consts::ARCH`.
    pub arch: String,
    /// Whether no person was part of the run.
    pub unattended: bool,
    /// Milliseconds since the app started.
    pub total_ms: u64,
}

/// How many rows each verdict has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// `PASS`.
    pub pass: usize,
    /// `UNAVAILABLE`.
    pub unavailable: usize,
    /// `DENIED`.
    pub denied: usize,
    /// `FAIL`.
    pub fail: usize,
    /// `NEEDS-HUMAN`.
    pub needs_human: usize,
    /// `PENDING`.
    pub pending: usize,
}

/// The verdicts of `rows`, counted.
#[must_use]
pub fn counts(rows: &[RowResult]) -> Counts {
    let mut c = Counts::default();
    for row in rows {
        match row.verdict {
            Verdict::Pass(_) => c.pass += 1,
            Verdict::Unavailable(_) => c.unavailable += 1,
            Verdict::Denied(_) => c.denied += 1,
            Verdict::Fail(_) => c.fail += 1,
            Verdict::NeedsHuman(_) => c.needs_human += 1,
            Verdict::Pending(_) => c.pending += 1,
        }
    }
    c
}

/// The process exit code: 1 when a capability that says it is available failed, else 0.
#[must_use]
pub fn exit_code(rows: &[RowResult]) -> i32 {
    i32::from(rows.iter().any(|row| row.verdict.is_failure()))
}

/// `s` as a JSON string, quotes included.
#[must_use]
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// The report as JSON: who ran it, the counts, every row.
#[must_use]
pub fn report_json(ctx: &Context, rows: &[RowResult]) -> String {
    let c = counts(rows);
    let mut j = String::new();
    j.push_str(&format!(
        "{{\"app\":{},\"version\":{},\"platform\":{},\"arch\":{},\"unattended\":{},\"total_ms\":{},",
        json_string(&ctx.app),
        json_string(&ctx.version),
        json_string(&ctx.platform),
        json_string(&ctx.arch),
        ctx.unattended,
        ctx.total_ms
    ));
    j.push_str(&format!(
        "\"pass\":{},\"unavailable\":{},\"denied\":{},\"fail\":{},\"needs_human\":{},\"pending\":{},\"rows\":[",
        c.pass, c.unavailable, c.denied, c.fail, c.needs_human, c.pending
    ));
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            j.push(',');
        }
        j.push_str(&format!(
            "{{\"id\":{},\"group\":{},\"title\":{},\"verdict\":{},\"detail\":{},\"ms\":{}}}",
            json_string(&row.id),
            json_string(&row.group),
            json_string(&row.title),
            json_string(row.verdict.code()),
            json_string(row.verdict.detail()),
            row.ms
        ));
    }
    j.push_str("]}");
    j
}

/// The word a harness greps for in logcat, `os_log` or stdout.
pub const REPORT_PREFIX: &str = "AZ_SELFTEST_REPORT";

/// The report as the ONE line the harnesses read: the prefix, a space, the JSON (which has no newline).
#[must_use]
pub fn report_line(ctx: &Context, rows: &[RowResult]) -> String {
    format!("{REPORT_PREFIX} {}", report_json(ctx, rows))
}

#[cfg(test)]
#[path = "report_test.rs"]
mod tests;
