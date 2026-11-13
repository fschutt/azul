//! The window's location tabs, as plain data: Ctrl+T opens a tab on the folder the window shows
//! (Finder's habit), Ctrl+W closes one, Ctrl+Tab and Ctrl+1..9 switch. The tabs are not drawn
//! as a strip; the tab overview lists them and the strip's tab button counts them.
//!
//! The window shows one tab at a time and keeps that tab's state live in `DriveState` (the
//! place, the history, the selection, the layout, the search, the scroll). Every other tab is
//! parked here as a [`TabState`]. A switch parks the live state and hands back the target's,
//! which the window then restores. No window types here, so all of it is tested without one.

use crate::{
    browse::{History, Place},
    model::{Selection, ViewLayout},
};

/// What one location tab keeps while another one shows.
#[derive(Debug, Clone)]
pub struct TabState {
    /// The place the tab shows.
    pub place: Place,
    /// Its own Back and Forward: a new tab starts with none.
    pub history: History,
    /// Its selection with the anchor and the focus (the arrows go on from there).
    pub selection: Selection,
    /// Its layout. The current tab's layout is the one the settings save.
    pub layout: ViewLayout,
    /// The search box's text; empty when the tab has no search open. The search runs again when
    /// the tab comes back, so its results are current.
    pub search: String,
    /// How far the folder view was scrolled (px); the view scrolls back there once the folder
    /// is listed again.
    pub scroll: f32,
}

impl TabState {
    /// A tab on `place` in `layout`: no history, nothing selected, no search, at the top.
    #[must_use]
    pub fn at(place: Place, layout: ViewLayout) -> TabState {
        TabState {
            place,
            history: History::default(),
            selection: Selection::default(),
            layout,
            search: String::new(),
            scroll: 0.0,
        }
    }
}

/// What closing a tab came to.
#[derive(Debug, Clone)]
pub enum Closed {
    /// Nothing closed: the window always keeps one tab.
    LastTab,
    /// Another tab closed; the one showing stays as it is.
    Other,
    /// The tab showing closed: this one shows now.
    Current(TabState),
}

/// The open tabs, in their order, and which one shows.
#[derive(Debug, Clone)]
pub struct LocationTabs {
    /// Every tab in order. The entry of the tab showing is what it was when it was last parked;
    /// its live state is the window's, which every call that needs it is handed.
    tabs: Vec<TabState>,
    /// The tab showing; always a valid index (there is at least one tab).
    current: usize,
}

impl LocationTabs {
    /// One tab, showing `first`.
    #[must_use]
    pub fn new(first: TabState) -> LocationTabs {
        LocationTabs {
            tabs: vec![first],
            current: 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    /// The index of the tab showing.
    #[must_use]
    pub fn current(&self) -> usize {
        self.current
    }

    /// Every tab as the overview lists it: the parked ones, and `live` for the one showing.
    #[must_use]
    pub fn snapshot(&self, live: TabState) -> Vec<TabState> {
        let mut all = self.tabs.clone();
        all[self.current] = live;
        all
    }

    /// Ctrl+T: parks `live` and opens a tab after it on the same place and layout, with no
    /// history, no selection, no search and at the top. Returns the new tab's state to show.
    pub fn open(&mut self, live: TabState) -> TabState {
        let fresh = TabState::at(live.place.clone(), live.layout);
        self.tabs[self.current] = live;
        self.current += 1;
        self.tabs.insert(self.current, fresh.clone());
        fresh
    }

    /// Shows tab `to`: parks `live` and returns `to`'s state. `None` (and nothing changes) for
    /// the tab already showing or an index past the last tab.
    pub fn switch(&mut self, to: usize, live: TabState) -> Option<TabState> {
        if to == self.current || to >= self.tabs.len() {
            return None;
        }
        self.tabs[self.current] = live;
        self.current = to;
        Some(self.tabs[to].clone())
    }

    /// Ctrl+Tab (`back`: Ctrl+Shift+Tab): the tab after the one showing (before it), round the
    /// ends.
    #[must_use]
    pub fn cycle(&self, back: bool) -> usize {
        let n = self.tabs.len();
        if back {
            (self.current + n - 1) % n
        } else {
            (self.current + 1) % n
        }
    }

    /// Ctrl+`digit` (1..9): the digit-th tab, when there is one.
    #[must_use]
    pub fn jump(&self, digit: u8) -> Option<usize> {
        let index = usize::from(digit).checked_sub(1)?;
        (digit <= 9 && index < self.tabs.len()).then_some(index)
    }

    /// Ctrl+W, or the close box in the overview: closes tab `at`. Closing the tab showing shows
    /// the one that moves into its place (the one before it when it was the last). An index
    /// past the last tab closes nothing and counts as [`Closed::Other`].
    pub fn close(&mut self, at: usize, live: TabState) -> Closed {
        if self.tabs.len() < 2 {
            return Closed::LastTab;
        }
        if at >= self.tabs.len() {
            return Closed::Other;
        }
        self.tabs[self.current] = live;
        self.tabs.remove(at);
        let showing = at == self.current;
        if at < self.current || self.current >= self.tabs.len() {
            self.current -= 1;
        }
        if showing {
            Closed::Current(self.tabs[self.current].clone())
        } else {
            Closed::Other
        }
    }
}

/// The Ctrl digit the overview shows beside tab `index`: the first nine tabs have one.
#[must_use]
pub fn shortcut_digit(index: usize) -> Option<u8> {
    u8::try_from(index + 1).ok().filter(|digit| *digit <= 9)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder(prefix: &str) -> Place {
        Place::folder("home", prefix)
    }

    /// A tab that has been somewhere: a history, a selection, a layout, a search, a scroll.
    fn lived_in(prefix: &str) -> TabState {
        let mut history = History::default();
        history.visit(Place::ThisPc);
        let mut selection = Selection::default();
        selection.click(&format!("{prefix}a.txt"));
        TabState {
            place: folder(prefix),
            history,
            selection,
            layout: ViewLayout::LargeIcons,
            search: String::from("steuer"),
            scroll: 120.0,
        }
    }

    fn places(tabs: &LocationTabs, live: TabState) -> Vec<Place> {
        tabs.snapshot(live).into_iter().map(|t| t.place).collect()
    }

    #[test]
    fn a_new_tab_opens_on_the_current_folder_after_it_with_only_the_layout_carried_over() {
        let mut tabs = LocationTabs::new(TabState::at(folder("Documents/"), ViewLayout::Details));
        let fresh = tabs.open(lived_in("Documents/"));
        assert_eq!(
            fresh.place,
            folder("Documents/"),
            "Finder's habit: the folder the window shows"
        );
        assert!(
            !fresh.history.can_go_back() && !fresh.history.can_go_forward(),
            "no history"
        );
        assert!(fresh.selection.is_empty(), "nothing selected");
        assert_eq!(
            fresh.layout,
            ViewLayout::LargeIcons,
            "the layout goes along"
        );
        assert!(fresh.search.is_empty(), "no search");
        assert_eq!(fresh.scroll, 0.0, "at the top");
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.current(), 1, "the new tab shows");
        let all = tabs.snapshot(fresh.clone());
        assert_eq!(
            all[0].search, "steuer",
            "the tab left behind is parked as it was"
        );
        assert_eq!(all[0].scroll, 120.0);
    }

    #[test]
    fn a_new_tab_is_inserted_right_after_the_one_showing() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let b = tabs.open(TabState::at(folder("A/"), ViewLayout::Details));
        let mut b_live = b.clone();
        b_live.place = folder("B/");
        let c = tabs.open(b_live);
        let mut c_live = c.clone();
        c_live.place = folder("C/");
        // Back to the first tab, then a new one: it goes between A and B.
        let a = tabs.switch(0, c_live).expect("the first tab");
        let fresh = tabs.open(a);
        assert_eq!(tabs.current(), 1);
        assert_eq!(
            places(&tabs, fresh),
            vec![folder("A/"), folder("A/"), folder("B/"), folder("C/")]
        );
    }

    #[test]
    fn each_tab_comes_back_as_it_was_left() {
        let mut tabs = LocationTabs::new(TabState::at(folder("Documents/"), ViewLayout::Details));
        let second = tabs.open(lived_in("Documents/"));
        let mut second_live = second;
        second_live.place = folder("Pictures/");
        second_live.layout = ViewLayout::Tiles;
        let back = tabs.switch(0, second_live).expect("the first tab");
        assert_eq!(back.place, folder("Documents/"));
        assert!(back.history.can_go_back(), "its own history");
        assert_eq!(back.selection.keys(), vec![String::from("Documents/a.txt")]);
        assert_eq!(back.layout, ViewLayout::LargeIcons);
        assert_eq!(back.search, "steuer");
        assert_eq!(back.scroll, 120.0);
        assert_eq!(tabs.current(), 0);
        let again = tabs.switch(1, back).expect("the second tab");
        assert_eq!(
            again.place,
            folder("Pictures/"),
            "the second tab was parked as it was left"
        );
        assert_eq!(again.layout, ViewLayout::Tiles);
    }

    #[test]
    fn switching_to_the_tab_showing_or_past_the_last_does_nothing() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let live = tabs.open(TabState::at(folder("A/"), ViewLayout::Details));
        assert!(tabs.switch(1, live.clone()).is_none(), "the tab showing");
        assert!(tabs.switch(2, live).is_none(), "past the last tab");
        assert_eq!(tabs.current(), 1);
        assert_eq!(tabs.len(), 2);
    }

    #[test]
    fn ctrl_tab_cycles_forward_and_ctrl_shift_tab_back_round_the_ends() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let b = tabs.open(TabState::at(folder("A/"), ViewLayout::Details));
        let _c = tabs.open(b);
        assert_eq!(tabs.current(), 2);
        assert_eq!(tabs.cycle(false), 0, "after the last comes the first");
        assert_eq!(tabs.cycle(true), 1);
        let single = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        assert_eq!(single.cycle(false), 0, "one tab cycles to itself");
        assert_eq!(single.cycle(true), 0);
    }

    #[test]
    fn ctrl_and_a_digit_jumps_to_that_tab_and_a_digit_past_the_last_jumps_nowhere() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let b = tabs.open(TabState::at(folder("A/"), ViewLayout::Details));
        let _c = tabs.open(b);
        assert_eq!(tabs.jump(1), Some(0));
        assert_eq!(tabs.jump(3), Some(2));
        assert_eq!(tabs.jump(4), None);
        assert_eq!(tabs.jump(0), None);
        assert_eq!(tabs.jump(9), None);
    }

    #[test]
    fn the_last_tab_cannot_be_closed() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let live = TabState::at(folder("A/"), ViewLayout::Details);
        assert!(matches!(tabs.close(0, live), Closed::LastTab));
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.current(), 0);
    }

    #[test]
    fn closing_the_tab_showing_shows_the_next_one_or_the_one_before_at_the_end() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let mut b = tabs.open(TabState::at(folder("A/"), ViewLayout::Details));
        b.place = folder("B/");
        let mut c = tabs.open(b);
        c.place = folder("C/");
        let b = tabs.switch(1, c).expect("B");
        match tabs.close(1, b) {
            Closed::Current(next) => assert_eq!(next.place, folder("C/"), "the next tab moves in"),
            other => panic!("closing the tab showing: {other:?}"),
        }
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.current(), 1);
        let c = TabState::at(folder("C/"), ViewLayout::Details);
        match tabs.close(1, c) {
            Closed::Current(before) => {
                assert_eq!(
                    before.place,
                    folder("A/"),
                    "the last tab hands over to the one before"
                )
            }
            other => panic!("closing the last tab showing: {other:?}"),
        }
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs.current(), 0);
    }

    #[test]
    fn closing_another_tab_keeps_the_one_showing_with_its_live_state() {
        let mut tabs = LocationTabs::new(TabState::at(folder("A/"), ViewLayout::Details));
        let mut b = tabs.open(TabState::at(folder("A/"), ViewLayout::Details));
        b.place = folder("B/");
        let mut c = tabs.open(b);
        c.place = folder("C/");
        assert!(matches!(tabs.close(0, c.clone()), Closed::Other));
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs.current(), 1, "the tab showing moved one to the left");
        assert_eq!(places(&tabs, c), vec![folder("B/"), folder("C/")]);
    }

    #[test]
    fn only_the_first_nine_tabs_have_a_ctrl_digit() {
        assert_eq!(shortcut_digit(0), Some(1));
        assert_eq!(shortcut_digit(8), Some(9));
        assert_eq!(shortcut_digit(9), None);
    }
}
