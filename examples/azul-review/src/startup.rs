//! The timeline of the start, for the scripts that measure it: one line per step, `AZREVIEW_PHASE <name> unix_us=<n>` on
//! standard output, the wall clock in microseconds so that a script can lay it beside its own clock and the engine's.
//!
//! The steps the app sees: `main` (the first thing `run` does), `args`, `kit`, `state` (the root listed, the first file open),
//! `speech`, `app_created` (`App::create` returned: the engine's own setup, its font registry and system style among it),
//! `run_called` (just before the event loop is entered), `window_created` (the window's create callback), `first_layout` (the
//! first time the window's DOM is built) and `tree_rows` (the first time the tree's rows are built). Between `run_called`
//! and `window_created` is the engine's window creation; its own log has the finer steps.

use std::cell::Cell;

/// The steps told once from inside the window's callbacks.
#[derive(Debug, Default)]
pub struct Told {
    first_layout: Cell<bool>,
    tree_rows: Cell<bool>,
}

impl Told {
    /// Tells the first layout, once.
    pub fn first_layout(&self) {
        if !self.first_layout.replace(true) {
            phase("first_layout");
        }
    }

    /// Tells the first build of the tree's rows, once.
    pub fn tree_rows(&self) {
        if !self.tree_rows.replace(true) {
            phase("tree_rows");
        }
    }
}

/// Says that step `name` happened now.
pub fn phase(name: &str) {
    let us = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_micros());
    println!("AZREVIEW_PHASE {name} unix_us={us}");
}
