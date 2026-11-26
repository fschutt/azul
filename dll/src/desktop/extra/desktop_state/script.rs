//! The words of a test's desktop: what a headless run's `DesktopState` says when the file named by
//! `AZ_DESKTOP_STATE_FILE` exists (read at every query, so a test changes the desktop while the app runs).
//!
//! One `key=value` a line; blank lines and lines starting with `#` are skipped:
//!
//! ```text
//! front=com.apple.Safari|Safari     the app in front (a bundle id, and a name after a `|`); `none`: not known
//! pointer=100.5,200                 the pointer, points from the top left of the primary display; `none`
//! locked=1                          the screen is locked (1, true, yes, on)
//! asleep=0                          the display is asleep (0, false, no, off)
//! ```
//!
//! Pure text: nothing here reads a file or the desktop. What is wrong with the text is said in
//! [`Script::problems`] and the rest is still read: a test's script is the test's own words and a typo in
//! it is the test's to see, not the app's to crash on.

/// What a script says of the desktop.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Script {
    /// The app in front: its bundle id and its name.
    pub front: Option<(String, String)>,
    /// The pointer on the desktop, points from the top left of the primary display.
    pub pointer: Option<(f32, f32)>,
    /// The screen is locked.
    pub locked: bool,
    /// The display is asleep.
    pub display_asleep: bool,
    /// What was wrong with the text, one sentence each.
    pub problems: Vec<String>,
}

/// The most of a script that is read, bytes.
pub const MAX_BYTES: usize = 4_096;
/// The longest a bundle id or a name may be, characters.
pub const MAX_NAME: usize = 256;

/// `1`, `true`, `yes`, `on` (any case) as `true`; `0`, `false`, `no`, `off` as `false`.
fn switch(word: &str) -> Option<bool> {
    match word.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
    }
}

/// `text` cut to at most [`MAX_NAME`] characters.
fn capped(text: &str, what: &str, problems: &mut Vec<String>) -> String {
    if text.chars().count() > MAX_NAME {
        problems.push(format!("the {what} is over the most of {MAX_NAME} characters: cut"));
        text.chars().take(MAX_NAME).collect()
    } else {
        text.to_string()
    }
}

/// The desktop `text` describes.
#[must_use]
pub fn parse(text: &str) -> Script {
    let mut script = Script::default();
    let mut text = text;
    if text.len() > MAX_BYTES {
        let mut at = MAX_BYTES;
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text = &text[..at];
        script.problems.push(format!("the script is over the most of {MAX_BYTES} bytes: the rest is not read"));
    }
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty() && !l.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else {
            script.problems.push(format!("the line {line:?} has no `=`"));
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "front" => {
                if value.eq_ignore_ascii_case("none") || value.is_empty() {
                    script.front = None;
                } else {
                    let (bundle, name) = value.split_once('|').unwrap_or((value, value));
                    let bundle = capped(bundle.trim(), "bundle id", &mut script.problems);
                    let name = capped(name.trim(), "name", &mut script.problems);
                    script.front = Some((bundle, name));
                }
            }
            "pointer" => {
                if value.eq_ignore_ascii_case("none") || value.is_empty() {
                    script.pointer = None;
                } else {
                    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
                    let numbers: Option<Vec<f32>> = parts.iter().map(|p| p.parse::<f32>().ok().filter(|n| n.is_finite())).collect();
                    match numbers.as_deref() {
                        Some([x, y]) => script.pointer = Some((*x, *y)),
                        _ => script.problems.push(format!("the pointer {value:?} is not two finite numbers, x and y")),
                    }
                }
            }
            "locked" => match switch(value) {
                Some(on) => script.locked = on,
                None => script.problems.push(format!("the setting locked is {value:?}, not on or off")),
            },
            "asleep" => match switch(value) {
                Some(on) => script.display_asleep = on,
                None => script.problems.push(format!("the setting asleep is {value:?}, not on or off")),
            },
            other => script.problems.push(format!("the key {other:?} is none of front, pointer, locked, asleep")),
        }
    }
    script
}

#[cfg(test)]
#[path = "script_test.rs"]
mod script_test;
