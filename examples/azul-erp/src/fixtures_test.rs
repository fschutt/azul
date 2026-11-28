//! What the window's tests share: the sample register as a state, the app on it.

use azul::prelude::RefAny;
use chrono::NaiveDate;

use crate::{app::State, ui::Erp};

/// The day of every test: the sample's days are relative to it.
pub(crate) fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 10, 3).unwrap()
}

/// A state with the sample register, its writes still queued.
pub(crate) fn sampled_state() -> State {
    let mut state = State::new(today());
    state.load(&[]);
    let mut n = 0;
    let mut mint = move || {
        n += 1;
        format!("00000000-0000-4000-8000-{n:012}")
    };
    state.seed_sample(&mut mint);
    state
}

/// The app on the sample register: English words, a desktop window, no kit behind it.
pub(crate) fn erp() -> Erp {
    crate::l10n::in_english();
    Erp::new(RefAny::new(()), sampled_state(), false, (1280.0, 800.0))
}

/// The app with no record at all (a first launch).
pub(crate) fn empty_erp() -> Erp {
    crate::l10n::in_english();
    let mut state = State::new(today());
    state.load(&[]);
    Erp::new(RefAny::new(()), state, false, (1280.0, 800.0))
}
