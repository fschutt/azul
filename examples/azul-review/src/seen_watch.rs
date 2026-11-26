//! The timer that looks at the pages in view: a page that has been in view long enough is seen (`progress.rs` says how
//! long), and what was seen is what a file's review is made of.
//!
//! It looks four times a second, reads the strip's scroll and width from the window, tells the model how much time passed
//! and builds the totals again when what they sum changed. It costs a few microseconds a tick and asks for the window to be
//! built again only when something changed (a page became seen, the totals moved).

use std::time::Instant;

use azul::{prelude::*, time::SystemTimeDiff};

use crate::{ui, AppState};

/// How often the pages in view are looked at, ms.
pub const TICK_MS: u64 = 250;
/// How long what was seen waits to be written, s.
pub const SAVE_EVERY_S: u64 = 20;

/// Starts the timer, once.
pub fn start(info: &mut CallbackInfo, app: &RefAny) {
    let mut handle = app.clone();
    let first = handle.downcast_mut::<AppState>().is_some_and(|mut s| {
        let first = !s.watching;
        s.watching = true;
        first
    });
    if first {
        let timer = Timer::create(app.clone(), on_tick, info.get_system_time_fn())
            .with_interval(Duration::System(SystemTimeDiff::from_millis(TICK_MS)));
        info.add_timer(TimerId::unique(), timer);
    }
}

extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let view = ui::strip_view(&info.callback_info);
    let changed = {
        let Some(mut s) = data.downcast_mut::<AppState>() else {
            return TimerCallbackReturn::terminate_unchanged();
        };
        let now = Instant::now();
        let elapsed = s
            .watched_at
            .map_or(0, |t| u32::try_from(now.duration_since(t).as_millis()).unwrap_or(u32::MAX));
        s.watched_at = Some(now);
        let mut changed = view.is_some_and(|(x, w)| s.watch_pages(x, w, elapsed));
        if s.rollup_dirty {
            s.refresh_rollup();
            changed = true;
        }
        // How far the symbol index is, at the tree's foot.
        if s.symbols_line_changed() {
            changed = true;
        }
        changed
    };
    // What was seen is kept every so often, not at every page.
    let save_now = data.downcast_mut::<AppState>().is_some_and(|mut s| {
        let due = s.progress.is_dirty() && s.progress_saved_at.is_none_or(|t| t.elapsed().as_secs() >= SAVE_EVERY_S);
        if due {
            s.progress_saved_at = Some(Instant::now());
        }
        due
    });
    if save_now {
        crate::progress_store::save(&data, &mut info.callback_info);
    }
    if changed {
        // The tree's marks are rows of a VirtualView: it builds them again.
        info.callback_info.trigger_all_virtual_view_rerender();
        return TimerCallbackReturn::continue_and_refresh_dom();
    }
    TimerCallbackReturn::continue_unchanged()
}
