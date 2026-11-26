//! The recorder demo's own command line and clock, as plain functions: the switch that says which of the three
//! sources it records, and the local time's offset from what `date +%z` says. No window, no azul.

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
