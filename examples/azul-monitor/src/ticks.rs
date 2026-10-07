//! What a tick (a new reading) redraws: the 1 Hz path that must not lay the
//! page out again.
//!
//! The window is built ONCE (and again only for what the user does: a
//! click, a key, a new tab). The numbers that change every second live in
//! LIVE VIEWS - VirtualViews whose callbacks read the model: the process
//! table, the Performance, Networking and Users pages. A reading re-renders
//! the live view of the tab shown (`CallbackInfo::trigger_virtual_view_rerender`:
//! one callback, one small nested DOM laid out in place) and rewrites the
//! status bar's marked labels (`StatusBar::update_segment_label`). No
//! `layout()`, no DOM diff of the page, no relayout of the chrome.
//!
//! Two exceptions: the FIRST reading (until it arrives the window shows an
//! empty state, so the first reading builds the page with its live views),
//! and the process table while the user's hand is on it (a scroll or a drag
//! in progress): it is left as it is until the hand rests
//! (`table::hands_on`), so the rows do not re-sort under the pointer.

/// The tabs: the old Task Manager's, those AzMonitor can fill (it lists no
/// windows - Applications - and no system services - Services).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The process table.
    #[default]
    Processes,
    /// The CPU and memory meters and their history graphs.
    Performance,
    /// The network's history graph and its figures.
    Networking,
    /// Who runs the processes.
    Users,
}

impl Screen {
    /// Every tab, in the tab row's order.
    pub const ALL: [Self; 4] = [
        Self::Processes,
        Self::Performance,
        Self::Networking,
        Self::Users,
    ];

    /// The tab's title.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Processes => "Processes",
            Self::Performance => "Performance",
            Self::Networking => "Networking",
            Self::Users => "Users",
        }
    }

    /// The `--screen` name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Processes => "processes",
            Self::Performance => "performance",
            Self::Networking => "networking",
            Self::Users => "users",
        }
    }

    /// The tab a `--screen` name opens (`None`: not a tab).
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|s| s.name() == name)
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

    /// The live view the tab shows.
    #[must_use]
    pub const fn view(self) -> LiveView {
        match self {
            Self::Processes => LiveView::Table,
            Self::Performance => LiveView::Performance,
            Self::Networking => LiveView::Networking,
            Self::Users => LiveView::Users,
        }
    }

    /// Whether the tab shows history graphs (which scroll between readings).
    #[must_use]
    pub const fn has_graphs(self) -> bool {
        matches!(self, Self::Performance | Self::Networking)
    }
}

/// A live view: a VirtualView a tick re-renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveView {
    /// The process table.
    Table,
    /// The Performance page.
    Performance,
    /// The Networking page.
    Networking,
    /// The Users page.
    Users,
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
/// the tab shown, `settings_open` = the settings page covers it,
/// `hands_on_table` = the user is scrolling / dragging in the process table.
#[must_use]
pub fn plan(first: bool, screen: Screen, settings_open: bool, hands_on_table: bool) -> TickPlan {
    if first {
        // The empty state gives way to the page: one build, every view in it.
        return TickPlan {
            refresh_dom: true,
            views: Vec::new(),
            status: false,
        };
    }
    let view = screen.view();
    let views = if settings_open || (view == LiveView::Table && hands_on_table) {
        Vec::new()
    } else {
        vec![view]
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
                for hands in [false, true] {
                    assert!(
                        !plan(false, screen, settings, hands).refresh_dom,
                        "{screen:?} settings={settings} hands={hands}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_first_reading_builds_the_page_once() {
        let p = plan(true, Screen::Processes, false, false);
        assert!(p.refresh_dom);
        // The rebuild renders every view anyway.
        assert!(p.views.is_empty());
    }

    #[test]
    fn a_tick_re_renders_only_the_live_view_of_the_tab_shown() {
        assert_eq!(
            plan(false, Screen::Processes, false, false).views,
            vec![LiveView::Table]
        );
        assert_eq!(
            plan(false, Screen::Performance, false, false).views,
            vec![LiveView::Performance]
        );
        assert_eq!(
            plan(false, Screen::Networking, false, false).views,
            vec![LiveView::Networking]
        );
        assert_eq!(
            plan(false, Screen::Users, false, false).views,
            vec![LiveView::Users]
        );
    }

    #[test]
    fn a_tick_leaves_the_table_alone_while_the_hand_is_on_it() {
        let p = plan(false, Screen::Processes, false, true);
        assert!(p.views.is_empty(), "the rows do not re-sort under the pointer");
        assert!(p.status, "the status bar keeps up");
        // The hand on the table does not stop the other tabs' views.
        assert_eq!(
            plan(false, Screen::Performance, false, true).views,
            vec![LiveView::Performance]
        );
    }

    #[test]
    fn the_settings_page_shows_no_live_view_but_the_status_bar_keeps_up() {
        let p = plan(false, Screen::Processes, true, false);
        assert!(p.views.is_empty());
        assert!(p.status);
        assert!(plan(false, Screen::Performance, false, false).status);
    }

    #[test]
    fn tabs_have_titles_names_places_and_views() {
        assert_eq!(Screen::at(1), Screen::Performance);
        assert_eq!(Screen::at(3), Screen::Users);
        assert_eq!(Screen::at(9), Screen::Processes);
        assert_eq!(Screen::Networking.index(), 2);
        assert_eq!(Screen::Processes.title(), "Processes");
        assert_eq!(Screen::Performance.name(), "performance");
        assert_eq!(Screen::named("networking"), Some(Screen::Networking));
        assert_eq!(Screen::named("services"), None);
        assert_eq!(Screen::Users.view(), LiveView::Users);
        assert!(Screen::Performance.has_graphs());
        assert!(Screen::Networking.has_graphs());
        assert!(!Screen::Processes.has_graphs());
    }
}
