//! What a tick (a new reading) redraws: the 1 Hz path that must not lay the
//! page out again.
//!
//! The window is built ONCE (and again only for what the user does: a
//! click, a key, a new screen). The numbers that change every second live
//! in LIVE VIEWS - VirtualViews whose callbacks read the model: the cards
//! strip, the process table, the performance page. A reading re-renders the
//! live views the screen shows (`CallbackInfo::trigger_virtual_view_rerender`:
//! one callback, one small nested DOM laid out in place) and rewrites the
//! status bar's marked labels (`StatusBar::update_segment_label`). No
//! `layout()`, no DOM diff of the page, no relayout of the chrome.
//!
//! The one exception is the FIRST reading: until it arrives the window shows
//! an empty state, so the first reading builds the page with its live views.

/// The screens (the tab row).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The process table under the cards strip.
    #[default]
    Processes,
    /// The charts: CPU (and each core), memory, disk, network.
    Performance,
}

impl Screen {
    /// Every screen, in the tab row's order.
    pub const ALL: [Self; 2] = [Self::Processes, Self::Performance];

    /// The tab's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Processes => "Processes",
            Self::Performance => "Performance",
        }
    }

    /// The `--screen` name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Processes => "processes",
            Self::Performance => "performance",
        }
    }

    /// The tab at `index`.
    #[must_use]
    pub fn at(index: usize) -> Self {
        Self::ALL.get(index).copied().unwrap_or_default()
    }

    /// Its index in the tab row.
    #[must_use]
    pub fn index(self) -> usize {
        Self::ALL.iter().position(|s| *s == self).unwrap_or(0)
    }
}

/// A live view: a VirtualView a tick re-renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveView {
    /// The cards strip over the table (CPU, memory, disk, network).
    Cards,
    /// The process table.
    Table,
    /// The performance page.
    Performance,
}

/// What one reading redraws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TickPlan {
    /// Build the page again (`Update::RefreshDom`): the first reading only.
    pub refresh_dom: bool,
    /// The live views to re-render in place.
    pub views: Vec<LiveView>,
    /// Rewrite the status bar's marked labels.
    pub status: bool,
}

/// What a reading redraws: `first` = the window's first reading, `screen`
/// the screen shown, `settings_open` = the settings page covers it.
#[must_use]
pub fn plan(first: bool, screen: Screen, settings_open: bool) -> TickPlan {
    if first {
        // The empty state gives way to the page: one build, every view in it.
        return TickPlan {
            refresh_dom: true,
            views: Vec::new(),
            status: false,
        };
    }
    let views = if settings_open {
        Vec::new()
    } else {
        match screen {
            Screen::Processes => vec![LiveView::Cards, LiveView::Table],
            Screen::Performance => vec![LiveView::Performance],
        }
    };
    TickPlan {
        refresh_dom: false,
        views,
        status: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tick_never_rebuilds_the_page() {
        for screen in Screen::ALL {
            for settings in [false, true] {
                assert!(
                    !plan(false, screen, settings).refresh_dom,
                    "{screen:?} settings={settings}"
                );
            }
        }
    }

    #[test]
    fn the_first_reading_builds_the_page_once() {
        let p = plan(true, Screen::Processes, false);
        assert!(p.refresh_dom);
        // The rebuild renders every view anyway.
        assert!(p.views.is_empty());
    }

    #[test]
    fn a_tick_re_renders_only_the_live_views_of_the_screen_shown() {
        assert_eq!(
            plan(false, Screen::Processes, false).views,
            vec![LiveView::Cards, LiveView::Table]
        );
        assert_eq!(
            plan(false, Screen::Performance, false).views,
            vec![LiveView::Performance]
        );
    }

    #[test]
    fn the_settings_page_shows_no_live_view_but_the_status_bar_keeps_up() {
        let p = plan(false, Screen::Processes, true);
        assert!(p.views.is_empty());
        assert!(p.status);
        assert!(plan(false, Screen::Performance, false).status);
    }

    #[test]
    fn screens_have_titles_names_and_places() {
        assert_eq!(Screen::at(1), Screen::Performance);
        assert_eq!(Screen::at(9), Screen::Processes);
        assert_eq!(Screen::Performance.index(), 1);
        assert_eq!(Screen::Processes.title(), "Processes");
        assert_eq!(Screen::Performance.name(), "performance");
    }
}
