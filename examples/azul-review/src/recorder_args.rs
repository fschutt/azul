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

/// The longest the demo waits after its window is up before it starts the recording on its own, milliseconds.
pub const AUTO_DELAY_MS_MAX: u64 = 60_000;

/// A recording the demo makes on its own (`--auto SECONDS [--auto-delay-ms MS]`): it starts when its window is up (or
/// `delay_ms` after that: the program's own start, its frameworks loading and its windows activating, is then over and
/// the start of the devices is measured alone), stops after `seconds`, says where it was saved and closes. For a run
/// of the real devices that is announced and short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Auto {
    /// How long it records, within [`AUTO_SECONDS_MIN`] and [`AUTO_SECONDS_MAX`].
    pub seconds: u64,
    /// How long after the window is up it starts, within 0 and [`AUTO_DELAY_MS_MAX`].
    pub delay_ms: u64,
    /// What was said of numbers that were brought into their ranges (the caller prints it).
    pub note: Option<String>,
}

/// A whole number of `what` in `text`: digits only; one that does not fit is the largest.
fn whole(flag: &str, text: &str, what: &str) -> Result<u64, String> {
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return Err(format!("{flag} needs a whole number of {what}, not {text:?}"));
    }
    Ok(text.parse::<u64>().unwrap_or(u64::MAX))
}

/// `--auto SECONDS` and `--auto-delay-ms MS` taken out of the arguments (the last of each given counts), and the rest in
/// their order. A number outside its range (too small, or too large to be a number at all) is brought into it with a
/// note: a recording that does not match what was typed is said, not hidden.
///
/// # Errors
/// A switch without a number, or with something that is no whole number; a delay with no recording to delay.
pub fn take_auto(args: Vec<String>) -> Result<(Option<Auto>, Vec<String>), String> {
    let (mut seconds, mut delay, mut rest) = (None, None, Vec::new());
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--auto" => {
                let text = args.next().ok_or_else(|| String::from("--auto needs the seconds to record, e.g. --auto 10"))?;
                seconds = Some((whole("--auto", &text, "seconds")?, text));
            }
            "--auto-delay-ms" => {
                let text = args.next().ok_or_else(|| String::from("--auto-delay-ms needs the milliseconds to wait, e.g. --auto-delay-ms 2500"))?;
                delay = Some((whole("--auto-delay-ms", &text, "milliseconds")?, text));
            }
            _ => rest.push(arg),
        }
    }
    let Some((wanted, text)) = seconds else {
        return match delay {
            Some(_) => Err(String::from("--auto-delay-ms delays a recording that --auto makes: give --auto SECONDS too")),
            None => Ok((None, rest)),
        };
    };
    let secs = wanted.clamp(AUTO_SECONDS_MIN, AUTO_SECONDS_MAX);
    let mut notes = Vec::new();
    if secs != wanted {
        notes.push(format!("--auto {text}: recording {secs} seconds instead (at least {AUTO_SECONDS_MIN} seconds and at most {AUTO_SECONDS_MAX} seconds are recorded on their own)"));
    }
    let (delay_wanted, delay_text) = delay.unwrap_or((0, String::new()));
    let delay_ms = delay_wanted.min(AUTO_DELAY_MS_MAX);
    if delay_ms != delay_wanted {
        notes.push(format!("--auto-delay-ms {delay_text}: waiting {AUTO_DELAY_MS_MAX} ms instead (the most the demo waits)"));
    }
    Ok((Some(Auto { seconds: secs, delay_ms, note: (!notes.is_empty()).then(|| notes.join("; ")) }), rest))
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
