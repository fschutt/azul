//! List selection model - the items a list, a tile grid, a slide rail or a
//! canvas has selected, the ANCHOR a Shift range extends from and the FOCUS
//! the keyboard is on. Windows Explorer's rules (and every desktop list's),
//! written once:
//!
//! - a plain click: the item alone; it becomes the anchor and the focus;
//! - Ctrl (Cmd on macOS) + click: the item in or out; it becomes the anchor
//!   and the focus;
//! - Shift + click: the items from the anchor to it, nothing else; the anchor
//!   stays;
//! - Ctrl + Shift + click: the items from the anchor to it ADDED;
//! - an arrow key: the focus moves; Shift extends from the anchor, Ctrl moves
//!   the focus alone, Ctrl+Space toggles the focused item;
//! - select all, select none, invert, keep only the items still listed.
//!
//! KEYS. An item is a `u64`. A list whose rows are what it selects (a mail
//! list, a slide rail) keys its rows by their INDEX: the methods without an
//! `order` take a range between two keys by number, and `count` rows for
//! select-all and the arrows. A list of items with ids passes its VISIBLE
//! ORDER (the ids top to bottom) to the `*_in` methods. Items named by a
//! string (a file path, a uuid) key by [`ListSelection::key_of`].
//!
//! The selection owns no view: the app keeps one, feeds each click and key
//! through it and marks the rows it renders ([`ListSelection::contains`]).
//! The keys are kept ascending without duplicates.
//!
//! `keep_one` (a slide rail - one slide is always on the canvas): a
//! Ctrl-toggle never takes the last selected item out.
//!
//! Key types: [`ListSelection`].

use alloc::vec::Vec;

use azul_css::corety::{OptionU64, U64Vec};

/// The selected items of a list, the anchor of a Shift range and the item
/// the keyboard is on.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListSelection {
    /// The selected keys, ascending, no duplicates.
    pub keys: U64Vec,
    /// Where a Shift range starts: the last plain or Ctrl selection.
    pub anchor: OptionU64,
    /// The item the keyboard is on.
    pub focus: OptionU64,
    /// A Ctrl-toggle never takes the last selected item out.
    pub keep_one: bool,
}

/// What a range or an arrow walks: the keys are positions (`count` of them;
/// `u64::MAX` when unbounded - a range is then taken by number), or the keys
/// in their visible order.
#[derive(Debug, Clone, Copy)]
enum Order<'a> {
    Positions(u64),
    Keys(&'a [u64]),
}

impl Order<'_> {
    /// How many items the order holds.
    const fn len(self) -> u64 {
        match self {
            Self::Positions(count) => count,
            Self::Keys(keys) => keys.len() as u64,
        }
    }

    /// Where `key` stands in the order.
    fn position(self, key: u64) -> Option<u64> {
        match self {
            Self::Positions(count) => (key < count).then_some(key),
            Self::Keys(keys) => keys.iter().position(|k| *k == key).map(|i| i as u64),
        }
    }

    /// The key at `position`.
    fn key_at(self, position: u64) -> u64 {
        match self {
            Self::Positions(_) => position,
            Self::Keys(keys) => keys[position as usize],
        }
    }

    /// The keys from `a` to `b` (either way round): `b` alone when `a` is
    /// not in the order, nothing when `b` is not.
    fn range(self, a: u64, b: u64) -> Vec<u64> {
        let Some(j) = self.position(b) else {
            return Vec::new();
        };
        let i = self.position(a).unwrap_or(j);
        let (from, to) = if i <= j { (i, j) } else { (j, i) };
        (from..=to).map(|p| self.key_at(p)).collect()
    }

    /// Every key of the order, in order.
    fn all(self) -> Vec<u64> {
        (0..self.len()).map(|p| self.key_at(p)).collect()
    }
}

/// `keys` ascending, without duplicates.
fn ascending(mut keys: Vec<u64>) -> U64Vec {
    keys.sort_unstable();
    keys.dedup();
    U64Vec::from_vec(keys)
}

impl ListSelection {
    /// Nothing selected, no anchor, no focus.
    #[must_use]
    pub const fn create() -> Self {
        Self {
            keys: U64Vec::from_const_slice(&[]),
            anchor: OptionU64::None,
            focus: OptionU64::None,
            keep_one: false,
        }
    }

    /// Whether a Ctrl-toggle may empty the selection (`false`, the default)
    /// or always leaves one item (`true`: a slide rail).
    pub const fn set_keep_one(&mut self, keep_one: bool) {
        self.keep_one = keep_one;
    }

    /// [`Self::set_keep_one`] for the builder chain.
    #[must_use]
    pub const fn with_keep_one(mut self, keep_one: bool) -> Self {
        self.set_keep_one(keep_one);
        self
    }

    /// Whether `key` is selected.
    #[must_use]
    pub fn contains(&self, key: u64) -> bool {
        self.keys.as_slice().binary_search(&key).is_ok()
    }

    /// How many items are selected.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.keys.len()
    }

    /// Whether nothing is selected.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// The one selected key, when exactly one is selected.
    #[must_use]
    pub fn single(&self) -> OptionU64 {
        match self.keys.as_slice() {
            [only] => OptionU64::Some(*only),
            _ => OptionU64::None,
        }
    }

    /// A plain click: `key` alone; it is the new anchor and focus.
    pub fn click(&mut self, key: u64) {
        self.keys = U64Vec::from_vec(alloc::vec![key]);
        self.anchor = OptionU64::Some(key);
        self.focus = OptionU64::Some(key);
    }

    /// Ctrl+click: `key` in or out (with `keep_one`, the last selected item
    /// stays in); it is the new anchor and focus.
    pub fn toggle(&mut self, key: u64) {
        let mut keys = self.keys.as_slice().to_vec();
        match keys.binary_search(&key) {
            Ok(at) => {
                if !(self.keep_one && keys.len() == 1) {
                    keys.remove(at);
                }
            }
            Err(at) => keys.insert(at, key),
        }
        self.keys = U64Vec::from_vec(keys);
        self.anchor = OptionU64::Some(key);
        self.focus = OptionU64::Some(key);
    }

    /// Shift+click on row `key` of a positional list: the rows from the
    /// anchor to it, nothing else; the anchor stays (no anchor: `key`).
    pub fn extend(&mut self, key: u64) {
        self.extend_over(Order::Positions(u64::MAX), key);
    }

    /// Shift+click on `key` of a keyed list shown in `order`: the items from
    /// the anchor to it in that order, nothing else; the anchor stays. A key
    /// not in `order` selects nothing; an anchor not in it, `key` alone.
    pub fn extend_in(&mut self, order: &[u64], key: u64) {
        self.extend_over(Order::Keys(order), key);
    }

    /// Ctrl+Shift+click on row `key` of a positional list: the rows from the
    /// anchor to it added; the anchor stays.
    pub fn add_range(&mut self, key: u64) {
        self.add_range_over(Order::Positions(u64::MAX), key);
    }

    /// Ctrl+Shift+click on `key` of a keyed list shown in `order`: the items
    /// from the anchor to it added; the anchor stays.
    pub fn add_range_in(&mut self, order: &[u64], key: u64) {
        self.add_range_over(Order::Keys(order), key);
    }

    /// A click on row `key` of a positional list with the modifiers held:
    /// plain = [`Self::click`], `ctrl` = [`Self::toggle`], `shift` =
    /// [`Self::extend`], both = [`Self::add_range`].
    pub fn select(&mut self, key: u64, shift: bool, ctrl: bool) {
        self.select_over(Order::Positions(u64::MAX), key, shift, ctrl);
    }

    /// [`Self::select`] for a keyed list shown in `order`.
    pub fn select_in(&mut self, order: &[u64], key: u64, shift: bool, ctrl: bool) {
        self.select_over(Order::Keys(order), key, shift, ctrl);
    }

    /// [`Self::select`] for the builder chain: the selection after a click
    /// on row `key` with the modifiers held.
    #[must_use]
    pub fn apply(mut self, key: u64, shift: bool, ctrl: bool) -> Self {
        self.select(key, shift, ctrl);
        self
    }

    /// Select all: rows `0..count`. With nothing focused, the first is.
    pub fn select_all(&mut self, count: u64) {
        self.select_all_over(Order::Positions(count));
    }

    /// Select all: every key of `order`. With nothing focused, the first is.
    pub fn select_all_in(&mut self, order: &[u64]) {
        self.select_all_over(Order::Keys(order));
    }

    /// Select none. The anchor and the focus stay; `keep_one` does not apply
    /// (the app clears on purpose).
    pub fn clear(&mut self) {
        self.keys = U64Vec::from_const_slice(&[]);
    }

    /// Invert selection over rows `0..count`.
    pub fn invert(&mut self, count: u64) {
        self.invert_over(Order::Positions(count));
    }

    /// Invert selection over the keys of `order`.
    pub fn invert_in(&mut self, order: &[u64]) {
        self.invert_over(Order::Keys(order));
    }

    /// Keeps only rows `0..count` (the list got shorter): the rest, and an
    /// anchor or focus past the end, go.
    pub fn retain(&mut self, count: u64) {
        self.retain_over(Order::Positions(count));
    }

    /// Keeps only the keys still in `order` (after a listing, a search): the
    /// rest, and an anchor or focus no longer listed, go.
    pub fn retain_in(&mut self, order: &[u64]) {
        self.retain_over(Order::Keys(order));
    }

    /// Selects exactly `keys` (what a paste or a marquee made): the first is
    /// the anchor, the last the focus.
    pub fn select_keys(&mut self, keys: &[u64]) {
        self.anchor = keys.first().copied().into();
        self.focus = keys.last().copied().into();
        self.keys = ascending(keys.to_vec());
    }

    /// An arrow key over rows `0..count`: the focus moves `delta` rows
    /// (clamped); `extend` (Shift) selects from the anchor to it, `keep`
    /// (Ctrl) moves only the focus, otherwise the focused row is the
    /// selection. With nothing focused the first row (the last, going back)
    /// is the target. Returns the focused row.
    pub fn step(&mut self, delta: i64, extend: bool, keep: bool, count: u64) -> OptionU64 {
        self.step_over(Order::Positions(count), delta, extend, keep)
    }

    /// [`Self::step`] over the keys of `order`.
    pub fn step_in(&mut self, order: &[u64], delta: i64, extend: bool, keep: bool) -> OptionU64 {
        self.step_over(Order::Keys(order), delta, extend, keep)
    }

    /// Ctrl+Space: the focused item in or out of the selection.
    pub fn toggle_focused(&mut self) {
        if let Some(key) = self.focus.into_option() {
            self.toggle(key);
        }
    }

    /// The key of an item named by a string (a file path, a uuid): equal
    /// names give equal keys. The same hash `NodeData::set_key` uses - stable
    /// within a process, not a value to store.
    #[must_use]
    pub fn key_of(name: &str) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut hasher = azul_core::hash::DefaultHasher::new();
        name.hash(&mut hasher);
        hasher.finish()
    }
}

/// The rules, once, over either kind of order.
impl ListSelection {
    fn extend_over(&mut self, order: Order<'_>, key: u64) {
        let anchor = self.anchor.into_option().unwrap_or(key);
        self.keys = ascending(order.range(anchor, key));
        self.anchor = OptionU64::Some(anchor);
        self.focus = OptionU64::Some(key);
    }

    fn add_range_over(&mut self, order: Order<'_>, key: u64) {
        let anchor = self.anchor.into_option().unwrap_or(key);
        let mut keys = self.keys.as_slice().to_vec();
        keys.extend(order.range(anchor, key));
        self.keys = ascending(keys);
        self.anchor = OptionU64::Some(anchor);
        self.focus = OptionU64::Some(key);
    }

    fn select_over(&mut self, order: Order<'_>, key: u64, shift: bool, ctrl: bool) {
        match (shift, ctrl) {
            (false, false) => self.click(key),
            (false, true) => self.toggle(key),
            (true, false) => self.extend_over(order, key),
            (true, true) => self.add_range_over(order, key),
        }
    }

    fn select_all_over(&mut self, order: Order<'_>) {
        let all = order.all();
        if self.focus.is_none() {
            self.focus = all.first().copied().into();
        }
        self.keys = ascending(all);
    }

    fn invert_over(&mut self, order: Order<'_>) {
        let inverted: Vec<u64> = order
            .all()
            .into_iter()
            .filter(|k| !self.contains(*k))
            .collect();
        self.keys = ascending(inverted);
    }

    fn retain_over(&mut self, order: Order<'_>) {
        // A keyed order is searched sorted: a re-listing of a big folder
        // with much of it selected stays fast.
        let mut listed: Vec<u64> = match order {
            Order::Positions(_) => Vec::new(),
            Order::Keys(keys) => keys.to_vec(),
        };
        listed.sort_unstable();
        let shown = |k: u64| match order {
            Order::Positions(count) => k < count,
            Order::Keys(_) => listed.binary_search(&k).is_ok(),
        };
        let kept: Vec<u64> = self
            .keys
            .as_slice()
            .iter()
            .copied()
            .filter(|k| shown(*k))
            .collect();
        self.keys = U64Vec::from_vec(kept);
        if self.focus.into_option().is_some_and(|f| !shown(f)) {
            self.focus = OptionU64::None;
        }
        if self.anchor.into_option().is_some_and(|a| !shown(a)) {
            self.anchor = OptionU64::None;
        }
    }

    fn step_over(&mut self, order: Order<'_>, delta: i64, extend: bool, keep: bool) -> OptionU64 {
        let len = order.len();
        if len == 0 {
            return OptionU64::None;
        }
        let last = len - 1;
        let current = self.focus.into_option().and_then(|f| order.position(f));
        let target = match current {
            None if delta < 0 => last,
            None => 0,
            Some(at) => {
                let moved = i128::from(at) + i128::from(delta);
                u64::try_from(moved.clamp(0, i128::from(last))).unwrap_or(0)
            }
        };
        let key = order.key_at(target);
        if keep {
            self.focus = OptionU64::Some(key);
        } else if extend {
            self.extend_over(order, key);
        } else {
            self.click(key);
        }
        OptionU64::Some(key)
    }
}

impl Default for ListSelection {
    fn default() -> Self {
        Self::create()
    }
}

#[cfg(test)]
mod list_selection_tests {
    use super::*;

    fn keys(s: &ListSelection) -> Vec<u64> {
        s.keys.as_slice().to_vec()
    }

    fn some(k: u64) -> OptionU64 {
        OptionU64::Some(k)
    }

    #[test]
    fn a_plain_click_selects_the_item_alone_and_moves_anchor_and_focus_there() {
        let mut s = ListSelection::create();
        assert!(s.is_empty());
        s.select(3, false, false);
        s.select(5, false, false);
        assert_eq!(keys(&s), vec![5]);
        assert_eq!((s.anchor, s.focus), (some(5), some(5)));
        assert!(s.contains(5) && !s.contains(3));
        assert_eq!(s.single(), some(5));
    }

    #[test]
    fn ctrl_click_toggles_an_item_and_moves_the_anchor_there() {
        let mut s = ListSelection::create();
        s.select(2, false, false);
        s.select(7, false, true);
        assert_eq!(keys(&s), vec![2, 7]);
        assert_eq!(s.anchor, some(7));
        s.select(2, false, true);
        assert_eq!(keys(&s), vec![7], "a second Ctrl+click takes it out");
        assert_eq!((s.anchor, s.focus), (some(2), some(2)));
        s.select(7, false, true);
        assert!(
            s.is_empty(),
            "without keep_one a list may end up with nothing selected"
        );
        assert_eq!(s.single(), OptionU64::None);
    }

    #[test]
    fn shift_click_selects_the_rows_from_the_anchor_and_keeps_the_anchor() {
        let mut s = ListSelection::create();
        s.select(4, false, false);
        s.select(7, true, false);
        assert_eq!(keys(&s), vec![4, 5, 6, 7]);
        assert_eq!((s.anchor, s.focus), (some(4), some(7)));
        s.select(2, true, false);
        assert_eq!(keys(&s), vec![2, 3, 4], "a new range replaces the old one");
        assert_eq!(s.anchor, some(4));
        let mut fresh = ListSelection::create();
        fresh.extend(6);
        assert_eq!(keys(&fresh), vec![6], "no anchor yet: the row alone");
        assert_eq!(fresh.anchor, some(6));
    }

    #[test]
    fn ctrl_shift_click_adds_the_range_to_the_selection() {
        let mut s = ListSelection::create();
        s.select(1, false, false);
        s.select(8, false, true);
        s.select(10, true, true);
        assert_eq!(keys(&s), vec![1, 8, 9, 10]);
        assert_eq!((s.anchor, s.focus), (some(8), some(10)));
    }

    #[test]
    fn a_keyed_list_takes_its_ranges_in_the_visible_order_not_by_number() {
        let order = [40, 10, 30, 20];
        let mut s = ListSelection::create();
        s.select_in(&order, 10, false, false);
        s.select_in(&order, 20, true, false);
        assert_eq!(
            keys(&s),
            vec![10, 20, 30],
            "10, 30, 20 on screen; kept ascending"
        );
        s.select_in(&order, 40, false, true);
        s.add_range_in(&order, 10);
        assert_eq!(keys(&s), vec![10, 20, 30, 40]);
        // A key not shown selects nothing; an anchor not shown, the key alone.
        let mut t = ListSelection::create();
        t.click(99);
        t.extend_in(&order, 30);
        assert_eq!(keys(&t), vec![30]);
        t.extend_in(&order, 77);
        assert!(t.is_empty());
    }

    #[test]
    fn the_keys_stay_ascending_and_unique() {
        let mut s = ListSelection::create();
        for k in [9, 3, 6, 1] {
            s.toggle(k);
        }
        assert_eq!(keys(&s), vec![1, 3, 6, 9]);
        s.select_keys(&[5, 2, 5, 8]);
        assert_eq!(keys(&s), vec![2, 5, 8]);
        assert_eq!(
            (s.anchor, s.focus),
            (some(5), some(8)),
            "first given anchors, last given is focused"
        );
    }

    #[test]
    fn keep_one_refuses_to_toggle_the_last_item_out() {
        let mut rail = ListSelection::create().with_keep_one(true);
        rail.select(0, false, false);
        rail.select(2, false, true);
        rail.select(0, false, true);
        assert_eq!(keys(&rail), vec![2]);
        rail.select(2, false, true);
        assert_eq!(keys(&rail), vec![2], "the rail keeps one slide");
        rail.clear();
        assert!(rail.is_empty(), "clear is the app's own choice");
    }

    #[test]
    fn select_all_invert_and_clear_work_over_rows_and_over_an_order() {
        let mut s = ListSelection::create();
        s.select_all(4);
        assert_eq!(keys(&s), vec![0, 1, 2, 3]);
        assert_eq!(s.focus, some(0), "nothing was focused: the first row is");
        s.click(2);
        s.invert(4);
        assert_eq!(keys(&s), vec![0, 1, 3]);
        s.clear();
        assert!(s.is_empty());
        assert_eq!(s.focus, some(2), "select none keeps the focus");
        let order = [30, 10, 20];
        s.select_all_in(&order);
        assert_eq!(keys(&s), vec![10, 20, 30]);
        s.click(10);
        s.invert_in(&order);
        assert_eq!(keys(&s), vec![20, 30]);
    }

    #[test]
    fn retain_drops_the_items_no_longer_listed_and_a_lost_focus_or_anchor() {
        let mut s = ListSelection::create();
        s.select_keys(&[1, 5, 9]);
        s.retain_in(&[1, 9, 12]);
        assert_eq!(keys(&s), vec![1, 9]);
        assert_eq!(s.anchor, some(1));
        assert_eq!(s.focus, some(9));
        s.retain_in(&[1]);
        assert_eq!(keys(&s), vec![1]);
        assert_eq!(s.focus, OptionU64::None, "the focused item is gone");
        let mut rows = ListSelection::create();
        rows.select_keys(&[0, 3, 6]);
        rows.retain(4);
        assert_eq!(keys(&rows), vec![0, 3]);
        assert_eq!((rows.anchor, rows.focus), (some(0), OptionU64::None));
    }

    #[test]
    fn the_arrows_move_the_focus_shift_extends_and_ctrl_moves_the_focus_alone() {
        let mut s = ListSelection::create();
        assert_eq!(
            s.step(1, false, false, 5),
            some(0),
            "nothing focused: Down goes to the first"
        );
        assert_eq!(keys(&s), vec![0]);
        assert_eq!(s.step(2, true, false, 5), some(2));
        assert_eq!(keys(&s), vec![0, 1, 2], "Shift extends from the anchor");
        assert_eq!(s.anchor, some(0));
        assert_eq!(s.step(1, false, true, 5), some(3));
        assert_eq!(keys(&s), vec![0, 1, 2], "Ctrl moves the focus alone");
        s.toggle_focused();
        assert_eq!(
            keys(&s),
            vec![0, 1, 2, 3],
            "Ctrl+Space toggles the focused row"
        );
        assert_eq!(s.step(10, false, false, 5), some(4), "clamped at the end");
        assert_eq!(keys(&s), vec![4]);
        let mut back = ListSelection::create();
        assert_eq!(
            back.step(-1, false, false, 5),
            some(4),
            "nothing focused: Up goes to the last"
        );
        let mut empty = ListSelection::create();
        assert_eq!(empty.step(1, false, false, 0), OptionU64::None);
        let order = [7, 3, 5];
        let mut k = ListSelection::create();
        k.click(3);
        assert_eq!(k.step_in(&order, 1, true, false), some(5));
        assert_eq!(keys(&k), vec![3, 5]);
        assert_eq!(k.step_in(&order, -5, false, false), some(7));
        assert_eq!(keys(&k), vec![7]);
    }

    #[test]
    fn apply_is_select_for_the_builder_chain() {
        let s = ListSelection::create()
            .apply(1, false, false)
            .apply(3, true, false)
            .apply(2, false, true);
        assert_eq!(keys(&s), vec![1, 3]);
        assert_eq!(s.anchor, some(2));
        assert_eq!(ListSelection::default(), ListSelection::create());
    }

    #[test]
    fn key_of_gives_equal_names_equal_keys_and_different_names_different_keys() {
        assert_eq!(
            ListSelection::key_of("Documents/report.docx"),
            ListSelection::key_of(&alloc::string::String::from("Documents/report.docx"))
        );
        assert_ne!(ListSelection::key_of("a"), ListSelection::key_of("b"));
        assert_ne!(ListSelection::key_of(""), ListSelection::key_of(" "));
    }
}
