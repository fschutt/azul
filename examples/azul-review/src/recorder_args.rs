//! The recorder demo's own command line and clock, as plain functions: the switch that says which of the three
//! sources it records, the one that makes it record on its own for a while, and the local time's offset from what
//! `date +%z` says. No window, no azul.

/// `screen,camera,mic` (any of them, in any order, as many as are wanted) as the three switches of the settings.
///
/// # Errors
/// The word that is none of the three.
pub fn parse_sources(list: &str) -> Result<(bool, bool, bool), String> {
    let (mut screen, mut camera, mut microphone) = (false, false, false);
    for word in list.split(',').map(str::trim).filter(|w| !w.is_empty()) {
        match word {
            "screen" => screen = true,
            "camera" => camera = true,
            "mic" => microphone = true,
            other => return Err(format!("{other:?} is none of screen, camera, mic")),
        }
    }
    Ok((screen, camera, microphone))
}

/// The arguments the demo reads itself (`--record screen,camera,mic`: what the recorder takes, the settings' own
/// choice when it is not given) taken out, and the rest for the app's switches.
pub fn own_arguments(args: Vec<String>) -> Result<(Option<(bool, bool, bool)>, Vec<String>), String> {
    let (mut record, mut rest) = (None, Vec::new());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "--record" {
            let list = args.next().ok_or_else(|| String::from("--record needs a list: screen,camera,mic"))?;
            record = Some(parse_sources(&list)?);
        } else {
            rest.push(arg);
        }
    }
    Ok((record, rest))
}

/// The shortest recording the demo makes on its own, seconds: under it no fragment of the camera's file is whole.
pub const AUTO_SECONDS_MIN: u64 = 2;

/// The longest, seconds: a recording that runs without anyone looking is not left running for more than ten minutes.
pub const AUTO_SECONDS_MAX: u64 = 600;

/// A recording the demo makes on its own (`--auto SECONDS`): it starts when its window is up, stops after `seconds`,
/// says where it was saved and closes. For a run of the real devices that is announced and short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Auto {
    /// How long it records, within [`AUTO_SECONDS_MIN`] and [`AUTO_SECONDS_MAX`].
    pub seconds: u64,
    /// What was said of a number that was brought into that range (the caller prints it).
    pub note: Option<String>,
}

/// `--auto SECONDS` taken out of the arguments (the last one given counts), and the rest in their order. A number
/// outside the range (too small, or too large to be a number at all) is brought into it with a note: a recording
/// that does not match what was typed is said, not hidden.
///
/// # Errors
/// A switch without a number, or with something that is no whole number of seconds.
pub fn take_auto(args: Vec<String>) -> Result<(Option<Auto>, Vec<String>), String> {
    let (mut auto, mut rest) = (None, Vec::new());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg != "--auto" {
            rest.push(arg);
            continue;
        }
        let text = args.next().ok_or_else(|| String::from("--auto needs the seconds to record, e.g. --auto 10"))?;
        let not_a_number = || format!("--auto needs a whole number of seconds, not {text:?}");
        if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
            return Err(not_a_number());
        }
        // Digits only: what does not fit in a u64 is far above the range.
        let wanted = text.parse::<u64>().unwrap_or(u64::MAX);
        let seconds = wanted.clamp(AUTO_SECONDS_MIN, AUTO_SECONDS_MAX);
        let note = (seconds != wanted).then(|| format!("--auto {text}: recording {seconds} seconds instead (at least {AUTO_SECONDS_MIN} seconds and at most {AUTO_SECONDS_MAX} seconds are recorded on their own)"));
        auto = Some(Auto { seconds, note });
    }
    Ok((auto, rest))
}

/// `+0100` / `-0530` as minutes; `None` for anything else.
#[must_use]
pub fn parse_utc_offset(text: &str) -> Option<i32> {
    let (sign, digits) = match text.as_bytes().first()? {
        b'+' => (1, &text[1..]),
        b'-' => (-1, &text[1..]),
        _ => return None,
    };
    if digits.len() != 4 || !digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let (hours, minutes): (i32, i32) = (digits[..2].parse().ok()?, digits[2..].parse().ok()?);
    (hours <= 14 && minutes < 60).then_some(sign * (hours * 60 + minutes))
}

#[cfg(test)]
#[path = "recorder_args_test.rs"]
mod recorder_args_test;
