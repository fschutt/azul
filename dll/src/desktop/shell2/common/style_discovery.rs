//! What the start of an app asks of the system and of subprocesses, as pure decisions with their tests: whether a
//! command-line fallback may answer an accessibility flag, and the line `App::create` logs for each of its steps
//! (`AZ_LOG=debug`), so that the next number of the start is read and not guessed.

use azul_css::dynamic_selector::BoolCondition;

/// Whether the command line may answer an accessibility flag (reduced motion, increased contrast): only where the native
/// query did not run (`native_answered` false) and left the flag at its default. A native `False` is an answer: asking
/// `defaults read` again costs a process (7 ms on a Mac, on the window's thread) for the answer the system already gave.
#[must_use]
pub fn cli_fallback_wanted(native_answered: bool, native: BoolCondition) -> bool {
    !native_answered && native == BoolCondition::False
}

/// A registry value as the text `reg query` prints it (`    <name>    <REG_TYPE>    <data>`, a DWORD as `0x` and
/// lowercase hex, binary as uppercase hex, a string as it is, a line ending of `\r\n`): the Windows style discovery
/// reads the registry natively (`RegGetValueW`) and hands the SAME text to the parsers that read `reg query`'s output
/// (`azul_css::system::windows_accent`, the build number, the flags), so what it reads and how it reads it are the
/// parsers' that are tested, and no `reg.exe` or PowerShell is started for it (each is a process: 30 to 100 ms, and
/// PowerShell several hundred, on the window's thread, before the first frame).
///
/// `ty` is the registry type (1 `REG_SZ`, 2 `REG_EXPAND_SZ`, 3 `REG_BINARY`, 4 `REG_DWORD`) and `bytes` the data as the
/// registry holds it (a string as UTF-16LE with its terminating NUL, a DWORD as 4 little-endian bytes). `None` for a
/// type the discovery does not read or data that does not fit it.
#[must_use]
pub fn reg_value_text(name: &str, ty: u32, bytes: &[u8]) -> Option<String> {
    let (type_name, data) = match ty {
        4 => {
            let dword = u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?);
            ("REG_DWORD", format!("0x{dword:x}"))
        }
        1 | 2 => {
            if bytes.len() % 2 != 0 {
                return None;
            }
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            let text = String::from_utf16(&units).ok()?;
            (if ty == 1 { "REG_SZ" } else { "REG_EXPAND_SZ" }, text.trim_end_matches('\0').to_owned())
        }
        3 => ("REG_BINARY", bytes.iter().map(|b| format!("{b:02X}")).collect::<String>()),
        _ => return None,
    };
    Some(format!("    {name}    {type_name}    {data}\r\n"))
}

/// The line of a step of `App::create`.
#[must_use]
pub fn step_line(step: &str, micros: u64) -> String {
    format!("[app] App::create: {step} {micros} us")
}

/// The clock of `App::create`'s steps: each is measured from the one before it.
#[derive(Debug, Clone, Copy)]
pub struct CreateSteps {
    start_us: u64,
    last_us: u64,
}

impl CreateSteps {
    /// The steps begin at `now_us` (microseconds of any clock that does not go back).
    #[must_use]
    pub const fn at(now_us: u64) -> Self {
        Self { start_us: now_us, last_us: now_us }
    }

    /// The step `step` ended at `now_us`: its line (the time since the step before; a clock that did not move or went
    /// back is no time).
    pub fn lap_at(&mut self, step: &str, now_us: u64) -> String {
        let line = step_line(step, now_us.saturating_sub(self.last_us));
        self.last_us = self.last_us.max(now_us);
        line
    }

    /// The whole time from the start to `now_us`.
    #[must_use]
    pub const fn total_at(&self, now_us: u64) -> u64 {
        now_us.saturating_sub(self.start_us)
    }
}

#[cfg(test)]
#[path = "style_discovery_test.rs"]
mod style_discovery_test;
