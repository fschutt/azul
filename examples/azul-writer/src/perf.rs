use std::{
    cell::RefCell,
    sync::OnceLock,
    time::{Duration, Instant},
};

pub use crate::args::FrameLog as Mode;

static M: OnceLock<Mode> = OnceLock::new();

pub fn init_frame_log(mode: Mode) {
    let _ = M.set(mode);
}

pub fn mode() -> Mode {
    M.get().copied().unwrap_or(Mode::Off)
}

thread_local! {
    static PHASES: RefCell<Vec<(&'static str, Duration)>> = const { RefCell::new(Vec::new()) };
}

pub struct Phase {
    name: &'static str,
    start: Option<Instant>,
}

impl Phase {
    #[must_use]
    pub fn start(name: &'static str) -> Self {
        Self {
            name,
            start: (mode() != Mode::Off).then(Instant::now),
        }
    }
}

impl Drop for Phase {
    fn drop(&mut self) {
        if let Some(t) = self.start {
            let d = t.elapsed();
            PHASES.with(|p| p.borrow_mut().push((self.name, d)));
        }
    }
}

pub fn take_phases() -> Vec<(&'static str, Duration)> {
    PHASES.with(|p| std::mem::take(&mut *p.borrow_mut()))
}

pub fn next_frame_number() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phases_record_nothing_when_the_env_var_is_unset() {
        if mode() != Mode::Off {
            return;
        }
        let _ = take_phases();
        {
            let _p = Phase::start("test");
        }
        assert!(
            take_phases().is_empty(),
            "a disabled Phase must not push a record"
        );
    }
}
