//! Pointer-SEAT bookkeeping for the shells that can present more than one
//! cursor (9b-ii-a).
//!
//! A shell that binds N `wl_seat`s (or sees N X11 master pointers) needs one
//! rule for "which azul seat is this event from": the seat bound FIRST is the
//! primary - `FullWindowState::mouse_state`, the one every pre-existing path
//! reads - and every later one is keyed by its registry name. The rule lives
//! here, generic over the proxy pointer type, because a file `cfg`-gated to
//! Linux is a file whose tests never run on this machine; the Wayland shell
//! instantiates it with `*mut c_void` and casts at the edges.
//!
//! What is NOT modelled, on purpose: per-seat keyboards and touch (the engine
//! has one `KeyboardState` and one `TouchState`), and the primary seat going
//! away (a compositor that revokes its only seat has bigger problems than us).

use azul_core::window::PRIMARY_POINTER_SEAT;

/// One bound seat.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeatEntry<P> {
    /// The registry global's name: what the compositor calls this seat, and
    /// - for a non-primary seat - the azul seat id. Global names start at 1,
    /// so no seat can collide with `PRIMARY_POINTER_SEAT`.
    pub global_name: u32,
    /// The `wl_seat` proxy.
    pub seat: P,
    /// The seat's `wl_pointer`, once its capabilities said it has one.
    pub pointer: Option<P>,
    /// The seat's `wl_keyboard` while it advertises one (9b-ii-a-i-b).
    pub keyboard: Option<P>,
    /// The bound interface version.
    pub version: u32,
}

/// Every seat this window has bound, primary first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeatTable<P> {
    entries: Vec<SeatEntry<P>>,
}

impl<P> Default for SeatTable<P> {
    fn default() -> Self {
        Self::new()
    }
}

impl<P> SeatTable<P> {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }
}

impl<P: Copy + PartialEq> SeatTable<P> {
    /// Record a newly bound seat and answer its azul seat id: the FIRST seat
    /// is the primary, every later one is keyed by its global name.
    pub fn insert(&mut self, global_name: u32, seat: P, version: u32) -> u64 {
        self.entries.push(SeatEntry {
            global_name,
            seat,
            pointer: None,
            keyboard: None,
            version,
        });
        self.seat_id_at(self.entries.len() - 1)
    }

    fn seat_id_at(&self, index: usize) -> u64 {
        if index == 0 {
            PRIMARY_POINTER_SEAT
        } else {
            u64::from(self.entries[index].global_name)
        }
    }

    fn index_of_seat(&self, seat: P) -> Option<usize> {
        self.entries.iter().position(|e| e.seat == seat)
    }

    /// Is this the seat bound first?
    #[must_use]
    pub fn is_primary(&self, seat: P) -> bool {
        self.index_of_seat(seat) == Some(0)
    }

    /// The azul seat id of a bound seat, `None` for a seat never bound.
    #[must_use]
    pub fn seat_id_of(&self, seat: P) -> Option<u64> {
        self.index_of_seat(seat).map(|i| self.seat_id_at(i))
    }

    /// Which seat a `wl_pointer` event came from.
    ///
    /// `PRIMARY_POINTER_SEAT` for the primary's pointer AND for a pointer the
    /// table does not know: the single-seat shell never looked at the proxy
    /// at all, so "unknown means primary" is exactly what every pre-existing
    /// path did, and a second seat can only ever be recognised, never
    /// invented.
    #[must_use]
    pub fn seat_id_for_pointer(&self, pointer: P) -> u64 {
        self.entries
            .iter()
            .position(|e| e.pointer == Some(pointer))
            .map_or(PRIMARY_POINTER_SEAT, |i| self.seat_id_at(i))
    }

    /// The pointer proxy a seat currently has, if any.
    #[must_use]
    pub fn pointer_of(&self, seat: P) -> Option<P> {
        self.index_of_seat(seat).and_then(|i| self.entries[i].pointer)
    }

    /// The seat gained (`Some`) or lost (`None`) its pointer capability.
    /// Ignored for a seat never bound.
    pub fn set_pointer(&mut self, seat: P, pointer: Option<P>) {
        if let Some(i) = self.index_of_seat(seat) {
            self.entries[i].pointer = pointer;
        }
    }

    /// The seat a `wl_keyboard` belongs to, by the keyboard PROXY (the same
    /// rule as the pointers - listener user data is the window and gets
    /// re-pointed wholesale). Unknown keyboards are the primary's.
    #[must_use]
    pub fn seat_id_for_keyboard(&self, keyboard: P) -> u64 {
        self.entries
            .iter()
            .position(|e| e.keyboard == Some(keyboard))
            .map_or(PRIMARY_POINTER_SEAT, |i| self.seat_id_at(i))
    }

    #[must_use]
    pub fn keyboard_of(&self, seat: P) -> Option<P> {
        self.index_of_seat(seat).and_then(|i| self.entries[i].keyboard)
    }

    pub fn set_keyboard(&mut self, seat: P, keyboard: Option<P>) {
        if let Some(i) = self.index_of_seat(seat) {
            self.entries[i].keyboard = keyboard;
        }
    }

    /// A seat's global went away. Never removes the primary - see the module
    /// doc - and answers `None` for it, so a caller cannot mistake "refused"
    /// for "gone".
    pub fn remove_by_name(&mut self, global_name: u32) -> Option<SeatEntry<P>> {
        let i = self
            .entries
            .iter()
            .position(|e| e.global_name == global_name)?;
        if i == 0 {
            return None;
        }
        Some(self.entries.remove(i))
    }

    #[must_use]
    pub fn primary(&self) -> Option<&SeatEntry<P>> {
        self.entries.first()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every seat, primary first, as `(seat id, entry)`.
    pub fn iter(&self) -> impl Iterator<Item = (u64, &SeatEntry<P>)> {
        self.entries
            .iter()
            .enumerate()
            .map(move |(i, e)| (self.seat_id_at(i), e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Opaque "proxies": distinct integers stand in for distinct pointers.
    type P = usize;

    #[test]
    fn the_first_seat_is_the_primary_and_the_rest_are_named_by_their_global() {
        let mut t: SeatTable<P> = SeatTable::new();
        assert_eq!(t.insert(7, 100, 9), PRIMARY_POINTER_SEAT, "bound first: primary");
        assert_eq!(t.insert(12, 200, 9), 12, "bound second: its global name");
        assert_eq!(t.insert(3, 300, 9), 3, "a LOWER global name is still not the primary");
        assert!(t.is_primary(100));
        assert!(!t.is_primary(200));
        assert_eq!(t.seat_id_of(300), Some(3));
        assert_eq!(t.seat_id_of(999), None);
        assert_eq!(t.primary().map(|e| e.global_name), Some(7));
        let ids: Vec<u64> = t.iter().map(|(id, _)| id).collect();
        assert_eq!(ids, vec![0, 12, 3]);
    }

    #[test]
    fn a_pointer_names_its_seat_and_an_unknown_one_is_the_primary() {
        // THE RULE THE OLD SHELL HAD BY OMISSION: it never looked at which
        // wl_pointer an event came from, so everything was the primary. That
        // stays true for anything the table does not know - a second seat is
        // recognised, never invented.
        let mut t: SeatTable<P> = SeatTable::new();
        t.insert(1, 100, 9);
        t.insert(2, 200, 9);
        t.set_pointer(100, Some(1000));
        t.set_pointer(200, Some(2000));
        assert_eq!(t.seat_id_for_pointer(1000), PRIMARY_POINTER_SEAT);
        assert_eq!(t.seat_id_for_pointer(2000), 2);
        assert_eq!(t.seat_id_for_pointer(3000), PRIMARY_POINTER_SEAT, "unknown: primary");
        assert_eq!(t.pointer_of(200), Some(2000));
        // The capability went away.
        t.set_pointer(200, None);
        assert_eq!(t.pointer_of(200), None);
        assert_eq!(t.seat_id_for_pointer(2000), PRIMARY_POINTER_SEAT, "no longer anyone's");
        // A seat never bound is ignored, not inserted.
        t.set_pointer(999, Some(9990));
        assert_eq!(t.len(), 2);
    }

    #[test]
    fn a_removed_global_takes_its_seat_and_the_primary_is_refused() {
        let mut t: SeatTable<P> = SeatTable::new();
        t.insert(1, 100, 9);
        t.insert(2, 200, 9);
        t.set_pointer(200, Some(2000));
        let gone = t.remove_by_name(2).expect("the second seat");
        assert_eq!(gone.seat, 200);
        assert_eq!(gone.pointer, Some(2000));
        assert_eq!(t.len(), 1);
        assert_eq!(t.remove_by_name(2), None, "already gone");
        assert_eq!(t.remove_by_name(1), None, "the primary is never removed");
        assert_eq!(t.len(), 1);
        assert!(t.is_primary(100));
    }

    #[test]
    fn removing_a_middle_seat_keeps_the_primary_and_the_others_ids() {
        let mut t: SeatTable<P> = SeatTable::new();
        t.insert(1, 100, 9);
        t.insert(2, 200, 9);
        t.insert(3, 300, 9);
        assert!(t.remove_by_name(2).is_some());
        assert_eq!(t.seat_id_of(300), Some(3), "keyed by name, not by position");
        assert!(t.is_primary(100));
    }
}
