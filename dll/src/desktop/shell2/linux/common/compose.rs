//! XKB compose sequences (dead keys, the Compose key), shared by the X11 and
//! Wayland backends.
//!
//! Neither Linux backend had any: both went straight from a keycode to
//! `xkb_state_key_get_utf8` (Wayland) or the core `XLookupString` (X11 with no
//! input method), and neither of those knows what a compose sequence is. So
//! `dead_acute` typed a bare `´` — or nothing at all — and the following `e`
//! typed a bare `e`; `é` was unreachable without a system IME. That is not an
//! exotic case: it is how every accented character is entered on the French,
//! Portuguese, Czech, Turkish, Vietnamese and international-US layouts, and how
//! `©`, `€`, `—` and the arrows are entered on all of them.
//!
//! libxkbcommon already ships the whole mechanism (it reads the same
//! `Compose` / `X11/locale/*/Compose` files Xlib does). What was missing was
//! the four calls that drive it, so this module is exactly that: a sequencer
//! around `xkb_compose_state`, plus the swallow/commit policy the key handlers
//! apply to its answer.
//!
//! The libxkbcommon entry points are held as plain function pointers rather
//! than as an `Rc<Xkb>` so the sequencer can be driven by stubs in a test —
//! there is no display server, no compositor and no Compose file in CI, and
//! the swallow/commit policy is precisely the part that was wrong by being
//! absent.

use std::ffi::{c_char, CString};

use super::super::super::common::debug_server::LogCategory;
use super::super::x11::defines::xkb_context;
use crate::{log_debug, log_warn};

/// Opaque `struct xkb_compose_table`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct xkb_compose_table {
    _private: [u8; 0],
}

/// Opaque `struct xkb_compose_state`.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct xkb_compose_state {
    _private: [u8; 0],
}

/// `XKB_COMPOSE_COMPILE_NO_FLAGS` — the only compile flag libxkbcommon defines.
pub const XKB_COMPOSE_COMPILE_NO_FLAGS: u32 = 0;
/// `XKB_COMPOSE_STATE_NO_FLAGS` — the only state flag libxkbcommon defines.
pub const XKB_COMPOSE_STATE_NO_FLAGS: u32 = 0;

/// `enum xkb_compose_feed_result`.
const FEED_IGNORED: i32 = 0;
const FEED_ACCEPTED: i32 = 1;

/// `enum xkb_compose_status`.
const STATUS_NOTHING: i32 = 0;
const STATUS_COMPOSING: i32 = 1;
const STATUS_COMPOSED: i32 = 2;
const STATUS_CANCELLED: i32 = 3;

/// Stack buffer for a composed string. Real sequences produce one character;
/// the heap retry below exists because the libxkbcommon contract allows more,
/// not because a Compose file is expected to.
const COMPOSED_STACK_BYTES: usize = 64;

/// The libxkbcommon entry points a sequencer needs, as raw function pointers.
///
/// The context pair is in here because the sequencer OWNS its context: a
/// compose table is a property of the LOCALE, not of the compositor's keymap,
/// and on Wayland the keymap context is thrown away and rebuilt on every
/// `wl_keyboard.keymap` event — a table borrowed from it would dangle the
/// first time the user switched layouts.
#[derive(Clone, Copy)]
pub struct ComposeFns {
    pub context_new: unsafe extern "C" fn(u32) -> *mut xkb_context,
    pub context_unref: unsafe extern "C" fn(*mut xkb_context),
    pub table_new_from_locale:
        unsafe extern "C" fn(*mut xkb_context, *const c_char, u32) -> *mut xkb_compose_table,
    pub table_unref: unsafe extern "C" fn(*mut xkb_compose_table),
    pub state_new: unsafe extern "C" fn(*mut xkb_compose_table, u32) -> *mut xkb_compose_state,
    pub state_unref: unsafe extern "C" fn(*mut xkb_compose_state),
    pub state_feed: unsafe extern "C" fn(*mut xkb_compose_state, u32) -> i32,
    pub state_reset: unsafe extern "C" fn(*mut xkb_compose_state),
    pub state_get_status: unsafe extern "C" fn(*mut xkb_compose_state) -> i32,
    pub state_get_utf8: unsafe extern "C" fn(*mut xkb_compose_state, *mut c_char, usize) -> i32,
}

/// What the key handler must do with the keysym it just fed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeAction {
    /// Not part of any sequence: translate the key the normal way.
    Pass,
    /// Mid-sequence. The key produces NO text — this is the half whose absence
    /// made `´` type a literal acute accent instead of arming the sequence.
    Composing,
    /// The sequence resolved. Insert this text INSTEAD of whatever the raw
    /// keysym would have produced (`é`, not `e`).
    Commit(String),
    /// The sequence was abandoned (an undefined continuation). No text: the
    /// keys that went into it are gone, which is what every other toolkit
    /// does and what the user expects after a mistyped sequence.
    Cancelled,
}

/// Drives one `xkb_compose_state`.
///
/// One per window, like the `xkb_state` next to it: a half-typed sequence
/// belongs to the window that is typing it.
pub struct ComposeSequencer {
    fns: ComposeFns,
    context: *mut xkb_context,
    table: *mut xkb_compose_table,
    state: *mut xkb_compose_state,
}

impl ComposeSequencer {
    /// Compile the locale's Compose file and open a state on it.
    ///
    /// `None` when the locale has no Compose file, when libxkbcommon is too
    /// old to export the compose API, or when the compile fails — all of which
    /// leave the backends on their previous (compose-less) behaviour rather
    /// than breaking key input.
    pub fn new(fns: ComposeFns) -> Option<Self> {
        let context = unsafe { (fns.context_new)(0) };
        if context.is_null() {
            return None;
        }
        let locale = compose_locale();
        let Ok(c_locale) = CString::new(locale.clone()) else {
            unsafe { (fns.context_unref)(context) };
            return None;
        };
        let table = unsafe {
            (fns.table_new_from_locale)(context, c_locale.as_ptr(), XKB_COMPOSE_COMPILE_NO_FLAGS)
        };
        if table.is_null() {
            log_warn!(
                LogCategory::Platform,
                "[xkb] no compose table for locale '{}' — dead keys will type their raw \
                 accent instead of composing",
                locale
            );
            unsafe { (fns.context_unref)(context) };
            return None;
        }
        let state = unsafe { (fns.state_new)(table, XKB_COMPOSE_STATE_NO_FLAGS) };
        if state.is_null() {
            unsafe {
                (fns.table_unref)(table);
                (fns.context_unref)(context);
            }
            return None;
        }
        log_debug!(
            LogCategory::Platform,
            "[xkb] compose table loaded for locale '{}'",
            locale
        );
        Some(Self {
            fns,
            context,
            table,
            state,
        })
    }

    /// Build a sequencer around an already-created table + state. The
    /// production path is [`Self::new`]; this exists so the sequencing policy
    /// can be driven without libxkbcommon.
    pub fn from_parts(
        fns: ComposeFns,
        table: *mut xkb_compose_table,
        state: *mut xkb_compose_state,
    ) -> Self {
        Self {
            fns,
            context: std::ptr::null_mut(),
            table,
            state,
        }
    }

    /// Feed one PRESSED keysym and say what the caller must do with it.
    ///
    /// Only presses: a release never advances a sequence, and feeding one
    /// would cancel every sequence at the moment the dead key came back up.
    pub fn feed(&mut self, keysym: u32) -> ComposeAction {
        if self.state.is_null() {
            return ComposeAction::Pass;
        }
        // Modifiers and any keysym the table does not participate in come back
        // IGNORED, which is what keeps Shift+dead_acute from cancelling.
        if unsafe { (self.fns.state_feed)(self.state, keysym) } != FEED_ACCEPTED {
            return ComposeAction::Pass;
        }
        match unsafe { (self.fns.state_get_status)(self.state) } {
            STATUS_COMPOSING => ComposeAction::Composing,
            STATUS_COMPOSED => {
                let text = self.composed_text();
                // Reset EXPLICITLY. libxkbcommon leaves the state in COMPOSED
                // until something else moves it, so a sequencer that skips
                // this reports the same `é` again for the next key pressed.
                unsafe { (self.fns.state_reset)(self.state) };
                if text.is_empty() {
                    ComposeAction::Cancelled
                } else {
                    ComposeAction::Commit(text)
                }
            }
            STATUS_CANCELLED => {
                unsafe { (self.fns.state_reset)(self.state) };
                ComposeAction::Cancelled
            }
            // STATUS_NOTHING and anything a future libxkbcommon adds.
            _ => ComposeAction::Pass,
        }
    }

    /// Abandon a half-typed sequence — focus left the window, or the app reset
    /// its IME. Leaving it armed makes the NEXT window's first keystroke
    /// complete a sequence the user started somewhere else.
    pub fn reset(&mut self) {
        if !self.state.is_null() {
            unsafe { (self.fns.state_reset)(self.state) };
        }
    }

    /// Read the composed string, honouring the `xkb_compose_state_get_utf8`
    /// overflow contract: the return value is the byte length the string NEEDS
    /// (excluding the NUL), and when that does not fit, the buffer holds a
    /// truncated result — which for UTF-8 means a chopped code point.
    fn composed_text(&self) -> String {
        let mut stack = [0 as c_char; COMPOSED_STACK_BYTES];
        let needed =
            unsafe { (self.fns.state_get_utf8)(self.state, stack.as_mut_ptr(), stack.len()) };
        if needed <= 0 {
            return String::new();
        }
        let needed = needed as usize;
        if needed < stack.len() {
            return decode(&stack[..needed]);
        }
        // +1 for the NUL libxkbcommon always writes.
        let mut heap = vec![0 as c_char; needed + 1];
        let written =
            unsafe { (self.fns.state_get_utf8)(self.state, heap.as_mut_ptr(), heap.len()) };
        if written <= 0 {
            return String::new();
        }
        let written = (written as usize).min(heap.len() - 1);
        decode(&heap[..written])
    }
}

impl Drop for ComposeSequencer {
    fn drop(&mut self) {
        unsafe {
            if !self.state.is_null() {
                (self.fns.state_unref)(self.state);
            }
            if !self.table.is_null() {
                (self.fns.table_unref)(self.table);
            }
            if !self.context.is_null() {
                (self.fns.context_unref)(self.context);
            }
        }
    }
}

fn decode(bytes: &[c_char]) -> String {
    let raw: Vec<u8> = bytes.iter().map(|b| *b as u8).collect();
    String::from_utf8_lossy(&raw).into_owned()
}

/// The locale whose Compose file to compile.
///
/// Same precedence `setlocale(LC_CTYPE, "")` uses, because that is the locale
/// Xlib's own compose handling picks and the two must agree on which file is
/// in force. `"C"` has no Compose file, so an unset environment simply leaves
/// compose unavailable rather than picking a wrong one.
fn compose_locale() -> String {
    for var in ["LC_ALL", "LC_CTYPE", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            if !value.is_empty() {
                return value;
            }
        }
    }
    "C".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in Compose file: the sequences below are the ones the real
    /// `en_US.UTF-8` table defines, driving the REAL sequencer through the
    /// same four calls libxkbcommon exports.
    struct FakeTable {
        fed: Vec<u32>,
        resets: usize,
        /// Set by a test to make `get_utf8` return something longer than the
        /// stack buffer.
        long_result: bool,
    }

    const DEAD_ACUTE: u32 = 0xfe51;
    const MULTI_KEY: u32 = 0xff20;
    const KEY_A: u32 = 0x61;
    const KEY_C: u32 = 0x63;
    const KEY_E: u32 = 0x65;
    const KEY_O: u32 = 0x6f;
    const KEY_Q: u32 = 0x71;
    const KEY_LSHIFT: u32 = 0xffe1;

    /// The sequences the fake table knows.
    const SEQUENCES: [(&[u32], &str); 2] = [
        (&[DEAD_ACUTE, KEY_E], "é"),
        (&[MULTI_KEY, KEY_O, KEY_C], "©"),
    ];

    impl FakeTable {
        fn new() -> Box<Self> {
            Box::new(Self {
                fed: Vec::new(),
                resets: 0,
                long_result: false,
            })
        }

        /// Does the fed prefix still lead somewhere?
        fn status(&self) -> i32 {
            if self.fed.is_empty() {
                return STATUS_NOTHING;
            }
            if SEQUENCES.iter().any(|(seq, _)| *seq == self.fed.as_slice()) {
                return STATUS_COMPOSED;
            }
            if SEQUENCES
                .iter()
                .any(|(seq, _)| seq.starts_with(self.fed.as_slice()))
            {
                return STATUS_COMPOSING;
            }
            STATUS_CANCELLED
        }

        fn result(&self) -> String {
            if self.long_result {
                return "é".repeat(COMPOSED_STACK_BYTES);
            }
            SEQUENCES
                .iter()
                .find(|(seq, _)| *seq == self.fed.as_slice())
                .map(|(_, out)| (*out).to_string())
                .unwrap_or_default()
        }
    }

    unsafe extern "C" fn fake_feed(state: *mut xkb_compose_state, keysym: u32) -> i32 {
        let t = &mut *(state as *mut FakeTable);
        // Modifiers are IGNORED by the real table too — holding Shift to reach
        // a dead key must not cancel the sequence it is about to start.
        if keysym == KEY_LSHIFT {
            return FEED_IGNORED;
        }
        // A keysym that starts nothing and continues nothing is IGNORED, which
        // is how ordinary typing stays on the normal path.
        if t.fed.is_empty() && !SEQUENCES.iter().any(|(seq, _)| seq[0] == keysym) {
            return FEED_IGNORED;
        }
        t.fed.push(keysym);
        FEED_ACCEPTED
    }

    unsafe extern "C" fn fake_status(state: *mut xkb_compose_state) -> i32 {
        (*(state as *mut FakeTable)).status()
    }

    unsafe extern "C" fn fake_reset(state: *mut xkb_compose_state) {
        let t = &mut *(state as *mut FakeTable);
        t.fed.clear();
        t.resets += 1;
    }

    unsafe extern "C" fn fake_get_utf8(
        state: *mut xkb_compose_state,
        buf: *mut c_char,
        size: usize,
    ) -> i32 {
        let t = &*(state as *mut FakeTable);
        let out = t.result();
        let bytes = out.as_bytes();
        // The real contract: return what the string NEEDS, write what fits,
        // always NUL-terminate.
        if size > 0 {
            let copy = bytes.len().min(size - 1);
            for (i, b) in bytes[..copy].iter().enumerate() {
                *buf.add(i) = *b as c_char;
            }
            *buf.add(copy) = 0;
        }
        bytes.len() as i32
    }

    static UNREFS: std::sync::Mutex<(usize, usize)> = std::sync::Mutex::new((0, 0));

    unsafe extern "C" fn fake_state_unref(_s: *mut xkb_compose_state) {
        UNREFS.lock().unwrap().0 += 1;
    }
    unsafe extern "C" fn fake_table_unref(_t: *mut xkb_compose_table) {
        UNREFS.lock().unwrap().1 += 1;
    }
    unsafe extern "C" fn unused_table_new(
        _c: *mut xkb_context,
        _l: *const c_char,
        _f: u32,
    ) -> *mut xkb_compose_table {
        std::ptr::null_mut()
    }
    unsafe extern "C" fn unused_state_new(
        _t: *mut xkb_compose_table,
        _f: u32,
    ) -> *mut xkb_compose_state {
        std::ptr::null_mut()
    }
    unsafe extern "C" fn unused_context_new(_f: u32) -> *mut xkb_context {
        std::ptr::null_mut()
    }
    unsafe extern "C" fn unused_context_unref(_c: *mut xkb_context) {}

    fn fns() -> ComposeFns {
        ComposeFns {
            context_new: unused_context_new,
            context_unref: unused_context_unref,
            table_new_from_locale: unused_table_new,
            table_unref: fake_table_unref,
            state_new: unused_state_new,
            state_unref: fake_state_unref,
            state_feed: fake_feed,
            state_reset: fake_reset,
            state_get_status: fake_status,
            state_get_utf8: fake_get_utf8,
        }
    }

    /// Returns the sequencer AND the box backing it — the box must outlive the
    /// sequencer, so the caller holds both.
    fn sequencer() -> (ComposeSequencer, Box<FakeTable>) {
        let mut table = FakeTable::new();
        let ptr = (&mut *table) as *mut FakeTable as *mut xkb_compose_state;
        (
            ComposeSequencer::from_parts(fns(), std::ptr::null_mut(), ptr),
            table,
        )
    }

    /// THE defect: `´` then `e` must produce `é`, and the `´` press itself
    /// must produce NO text. Before this module the dead key typed a bare
    /// acute accent (or nothing) and the `e` typed a bare `e`.
    ///
    /// NEGATIVE CONTROL: make the `STATUS_COMPOSING` arm of
    /// `ComposeSequencer::feed` return `ComposeAction::Pass` — the dead key
    /// falls through to the raw-keysym path again and this fails on the first
    /// assert.
    #[test]
    fn a_dead_key_arms_the_sequence_and_the_next_key_commits_the_accent() {
        let (mut compose, _backing) = sequencer();

        assert_eq!(
            compose.feed(DEAD_ACUTE),
            ComposeAction::Composing,
            "the dead key must be swallowed, not typed"
        );
        assert_eq!(
            compose.feed(KEY_E),
            ComposeAction::Commit("é".to_string()),
            "the sequence must commit the composed character, not the raw 'e'"
        );
    }

    /// A three-key Compose sequence: both middle keys swallowed, one commit.
    #[test]
    fn a_multi_key_sequence_swallows_every_key_before_the_last() {
        let (mut compose, _backing) = sequencer();

        assert_eq!(compose.feed(MULTI_KEY), ComposeAction::Composing);
        assert_eq!(compose.feed(KEY_O), ComposeAction::Composing);
        assert_eq!(compose.feed(KEY_C), ComposeAction::Commit("©".to_string()));
    }

    /// Ordinary typing must not go anywhere near the compose machinery.
    #[test]
    fn a_key_that_starts_no_sequence_passes_through() {
        let (mut compose, _backing) = sequencer();
        assert_eq!(compose.feed(KEY_A), ComposeAction::Pass);
        assert_eq!(compose.feed(KEY_LSHIFT), ComposeAction::Pass);
    }

    /// A committed sequence must leave the state EMPTY, or the next keystroke
    /// re-commits the same character.
    ///
    /// NEGATIVE CONTROL: delete the
    /// `unsafe { (self.fns.state_reset)(self.state) }` from the
    /// `STATUS_COMPOSED` arm — the fake keeps `[dead_acute, e]` fed, the next
    /// `a` lands on a non-empty prefix and this reports `Cancelled`.
    #[test]
    fn a_commit_resets_the_sequence() {
        let (mut compose, _backing) = sequencer();
        compose.feed(DEAD_ACUTE);
        assert!(matches!(compose.feed(KEY_E), ComposeAction::Commit(_)));

        assert_eq!(
            compose.feed(KEY_A),
            ComposeAction::Pass,
            "the state stayed COMPOSED and swallowed the next key"
        );
    }

    /// A sequence with no defined continuation is abandoned: no text at all,
    /// and the state is clean for the next one.
    #[test]
    fn an_undefined_continuation_cancels_and_clears() {
        let (mut compose, _backing) = sequencer();
        compose.feed(DEAD_ACUTE);

        assert_eq!(compose.feed(KEY_Q), ComposeAction::Cancelled);
        assert_eq!(
            compose.feed(KEY_A),
            ComposeAction::Pass,
            "a cancelled sequence must not poison the next keystroke"
        );
    }

    /// Focus loss abandons a half-typed sequence.
    #[test]
    fn reset_abandons_a_half_typed_sequence() {
        let (mut compose, _backing) = sequencer();
        compose.feed(DEAD_ACUTE);
        compose.reset();

        assert_eq!(
            compose.feed(KEY_A),
            ComposeAction::Pass,
            "the sequence survived a reset into the next focus"
        );
    }

    /// `xkb_compose_state_get_utf8` returns the length the string NEEDS, not
    /// the length it wrote. Trusting it against the stack buffer slices a
    /// UTF-8 string mid-code-point and reads past the array.
    ///
    /// NEGATIVE CONTROL: drop the heap retry in `composed_text` and return
    /// `decode(&stack[..needed.min(stack.len())])` — the result comes back
    /// truncated and this fails on the length.
    #[test]
    fn a_composed_string_longer_than_the_stack_buffer_survives() {
        let (mut compose, mut backing) = sequencer();
        backing.long_result = true;

        compose.feed(DEAD_ACUTE);
        let expected = "é".repeat(COMPOSED_STACK_BYTES);
        assert_eq!(compose.feed(KEY_E), ComposeAction::Commit(expected));
    }

    /// Both libxkbcommon objects are released, not just the state.
    #[test]
    fn dropping_the_sequencer_releases_the_state_and_the_table() {
        *UNREFS.lock().unwrap() = (0, 0);
        {
            let mut backing = FakeTable::new();
            let ptr = (&mut *backing) as *mut FakeTable as *mut xkb_compose_state;
            let _compose = ComposeSequencer::from_parts(fns(), 1 as *mut xkb_compose_table, ptr);
        }
        assert_eq!(*UNREFS.lock().unwrap(), (1, 1));
    }
}
