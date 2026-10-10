//! The recorded sessions of `--sample` (and of the headless E2E): bytes a
//! shell would have written - ANSI colours, a build, `git status`, a
//! listing, a long log for the scrollback - replayed into a terminal with
//! no PTY, so a screenshot or a test sees the same screen every time. What
//! is typed is echoed; two commands run, for a flood to watch, scroll up
//! from and follow again: `seq N` and `yes | head -n N` ([`Stream`]).

/// The prompt: the folder in bold green, the branch in cyan.
pub const PROMPT: &str = "\x1b[1;32m~/azul-apps\x1b[0m \x1b[36mmain\x1b[0m $ ";

/// The crates the sample build compiles.
const CRATES: [&str; 12] = [
    "proc-macro2",
    "unicode-ident",
    "quote",
    "syn",
    "serde_derive",
    "serde",
    "azul-css",
    "azul-core",
    "azul-layout",
    "azul-dll",
    "azul-appkit",
    "AzWriter",
];

/// A build, a `git status`, a listing, the 16 and 256 colours and a true
/// colour ramp, and the prompt waiting.
pub fn build_session() -> Vec<u8> {
    let mut s = String::new();
    s.push_str(PROMPT);
    s.push_str("cargo run -p AzWriter\r\n");
    for name in CRATES {
        s.push_str(&format!("\x1b[1;32m   Compiling\x1b[0m {name} v0.1.0\r\n"));
    }
    s.push_str(
        "\x1b[1;32m    Finished\x1b[0m `release` profile [optimized] target(s) in 41.20s\r\n",
    );
    s.push_str("\x1b[1;32m     Running\x1b[0m `target/release/AzWriter`\r\n");
    s.push_str(PROMPT);
    s.push_str("git status -sb\r\n");
    s.push_str("## \x1b[32mmain\x1b[0m...\x1b[31morigin/main\x1b[0m [ahead \x1b[32m1\x1b[0m]\r\n");
    s.push_str(" \x1b[31mM\x1b[0m apps/writer/src/lib.rs\r\n");
    s.push_str("\x1b[31m??\x1b[0m planning/terminal.md\r\n");
    s.push_str(PROMPT);
    s.push_str("ls planning/core\r\n");
    s.push_str("\x1b[1;34mcalculator.md\x1b[0m  \x1b[1;34mclock.md\x1b[0m  code-editor.md  terminal.md\r\n");
    s.push_str(PROMPT);
    s.push_str("colours\r\n");
    s.push_str(&colours());
    s.push_str(PROMPT);
    s.into_bytes()
}

/// The 16 ANSI colours (normal, then bright), a row of the 256-colour cube
/// and a true colour ramp.
fn colours() -> String {
    let mut s = String::new();
    for i in 0..8 {
        s.push_str(&format!("\x1b[4{i}m  \x1b[0m"));
    }
    s.push_str("\r\n");
    for i in 0..8 {
        s.push_str(&format!("\x1b[10{i}m  \x1b[0m"));
    }
    s.push_str("\r\n");
    for i in 16..52u32 {
        s.push_str(&format!("\x1b[48;5;{i}m \x1b[0m"));
    }
    s.push_str("\r\n");
    for i in 0..36u32 {
        let r = 255 - i * 7;
        let b = i * 7;
        s.push_str(&format!("\x1b[48;2;{r};64;{b}m \x1b[0m"));
    }
    s.push_str("\r\n");
    s
}

/// `lines` lines of a web server's log (`journalctl -fu web`): the
/// scrollback's yardstick.
pub fn log_session(lines: usize) -> Vec<u8> {
    let mut s = String::new();
    s.push_str("\x1b[1;32mbuild.example.org\x1b[0m:~$ journalctl -fu web\r\n");
    let paths = ["/", "/app.css", "/api/items", "/api/x", "/favicon.ico"];
    for i in 0..lines {
        let path = paths[i % paths.len()];
        let (status, colour) = if path == "/api/x" {
            (404, 31)
        } else {
            (200, 32)
        };
        let (minute, second) = ((i / 60) % 60, i % 60);
        s.push_str(&format!(
            "Sep 15 10:{minute:02}:{second:02} web[812]: GET {path} \x1b[{colour}m{status}\x1b[0m {} ms\r\n",
            1 + i % 17
        ));
    }
    s.into_bytes()
}

/// The lines a sample stream writes a tick - at 60 ticks a second 3,000
/// lines a second: a flood to watch go by, slow enough that a scroll up
/// lands before 10,000 lines of scrollback carry what is in view away.
pub const STREAM_LINES_PER_TICK: usize = 50;

/// What a stream writes on every line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StreamKind {
    /// `seq`: the line's number.
    Seq,
    /// `yes`: "y".
    Yes,
}

/// A command of the sample shell that writes many lines - `seq N` (1 to
/// N) or `yes | head -n N` (N times "y") - a few lines a tick
/// ([`STREAM_LINES_PER_TICK`]), so a flood can be watched, scrolled up from
/// and followed again with no PTY (`--sample`, the E2E).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stream {
    kind: StreamKind,
    /// The next line's number, from 1.
    next: u64,
    /// The last line's number.
    end: u64,
}

impl Stream {
    /// The stream `command` runs, if it is one that writes a line or more.
    #[must_use]
    pub fn parse(command: &str) -> Option<Self> {
        let mut words = command.split_whitespace();
        if words.next() == Some("seq") {
            let n: u64 = words.next()?.parse().ok()?;
            return (words.next().is_none() && n > 0).then_some(Self {
                kind: StreamKind::Seq,
                next: 1,
                end: n,
            });
        }
        // `yes | head -n N`, `yes|head -n4`, `yes | head -4`.
        let compact: String = command.chars().filter(|c| !c.is_whitespace()).collect();
        let count = compact.strip_prefix("yes|head")?;
        let count = count
            .strip_prefix("-n")
            .or_else(|| count.strip_prefix('-'))?;
        let n: u64 = count.parse().ok()?;
        (n > 0).then_some(Self {
            kind: StreamKind::Yes,
            next: 1,
            end: n,
        })
    }

    /// The lines still to come.
    #[must_use]
    pub const fn lines(&self) -> u64 {
        (self.end + 1).saturating_sub(self.next)
    }

    /// Whether every line was written.
    #[must_use]
    pub const fn done(&self) -> bool {
        self.next > self.end
    }

    /// The next `lines` lines (fewer at the end), each ending in CR LF.
    pub fn take(&mut self, lines: usize) -> Vec<u8> {
        let mut out = String::new();
        let mut left = lines;
        while left > 0 && !self.done() {
            match self.kind {
                StreamKind::Seq => out.push_str(&format!("{}\r\n", self.next)),
                StreamKind::Yes => out.push_str("y\r\n"),
            }
            self.next += 1;
            left -= 1;
        }
        out.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{session::Session, vt::GridSize};

    fn rows(session: &Session) -> Vec<String> {
        let term = session.term.lock();
        crate::vt::screen(&*term, true)
            .lines
            .as_slice()
            .iter()
            .map(|l| {
                l.runs
                    .as_slice()
                    .iter()
                    .map(|r| r.text.as_str().to_string())
                    .collect()
            })
            .collect()
    }

    #[test]
    fn the_build_session_ends_at_a_waiting_prompt() {
        let s = Session::replay(&build_session(), GridSize::new(80, 24), 10_000);
        let rows = rows(&s);
        let last = rows
            .iter()
            .rev()
            .find(|r| !r.is_empty())
            .cloned()
            .unwrap_or_default();
        assert_eq!(last, "~/azul-apps main $");
        assert!(rows.iter().any(|r| r.contains("Finished")));
    }

    #[test]
    fn the_log_session_fills_the_scrollback() {
        let s = Session::replay(&log_session(5_000), GridSize::new(80, 24), 10_000);
        let term = s.term.lock();
        let screen = crate::vt::screen(&*term, true);
        assert!(screen.history >= 4_970, "{}", screen.history);
    }

    #[test]
    fn the_sample_shell_knows_seq_and_yes_and_nothing_else() {
        let lines = |command: &str| Stream::parse(command).map(|s| s.lines());
        assert_eq!(lines("seq 3"), Some(3));
        assert_eq!(lines("  seq   12 "), Some(12));
        assert_eq!(lines("yes | head -n 100000"), Some(100_000));
        assert_eq!(lines("yes|head -n 4"), Some(4));
        assert_eq!(lines("yes | head -n4"), Some(4));
        assert_eq!(lines("yes | head -4"), Some(4));
        for other in ["echo hi", "seq", "seq x", "seq 0", "yes", "yes | head", "", "ls -la"] {
            assert_eq!(lines(other), None, "{other}");
        }
    }

    #[test]
    fn a_stream_writes_its_lines_a_few_at_a_time() {
        let mut seq = Stream::parse("seq 120").expect("seq");
        let first = seq.take(50);
        assert!(first.starts_with(b"1\r\n2\r\n"));
        assert!(first.ends_with(b"\r\n50\r\n"));
        assert!(!seq.done());
        let _ = seq.take(50);
        let last = seq.take(50);
        let expected: String = (101..=120).map(|i| format!("{i}\r\n")).collect();
        assert_eq!(last, expected.into_bytes());
        assert!(seq.done());
        assert!(seq.take(50).is_empty());
        let mut yes = Stream::parse("yes | head -n 3").expect("yes");
        assert_eq!(yes.take(50), b"y\r\ny\r\ny\r\n");
        assert!(yes.done());
    }
}
