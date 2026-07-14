//! Clipboard Manager
//!
//! Manages clipboard content flow between the system clipboard and
//! the application.
//!
//! ## Architecture
//!
//! The clipboard manager acts as a bridge between system clipboard
//! operations and user callbacks:
//!
//! 1. **Paste Flow**: System clipboard → `ClipboardManager` → User Callback → `TextInputManager`
//!
//!    - When Ctrl+V is pressed, `event_v2` reads system clipboard and calls `set_paste_content()`
//!    - User's `On::Paste` callback can inspect content via `get_clipboard_content()`
//!    - User can modify/block paste by not calling the default paste action
//!    - After callback, content is cleared for next operation
//!
//! 2. **Copy Flow**: Selection → User Callback → `ClipboardManager` → System clipboard
//!
//!    - When Ctrl+C is pressed, user's `On::Copy` callback fires
//!    - Callback can inspect selected content and override via `set_copy_content()`
//!    - `event_v2` calls `get_copy_content()` to get final content (override or default)
//!    - Content is written to system clipboard via platform sync
//!    - After callback, content is cleared for next operation
//!
//! 3. **Cut Flow**: Same as Copy + delete selection

use crate::managers::selection::ClipboardContent;

/// Manages clipboard content flow between system clipboard and application
///
/// This manager temporarily holds clipboard content during clipboard operations,
/// allowing user callbacks to inspect and modify content before it's committed
/// to the system clipboard or pasted into the document.
#[derive(Debug, Clone, Default)]
pub struct ClipboardManager {
    /// Content from system clipboard when paste is triggered
    /// Available to user callbacks via `CallbackInfo::get_clipboard_content()`
    pending_paste_content: Option<ClipboardContent>,

    /// Content to be written to system clipboard after copy/cut
    /// Set by user callbacks via `CallbackInfo::set_copy_content()`
    pending_copy_content: Option<ClipboardContent>,
}

impl ClipboardManager {
    /// Create a new empty clipboard manager
    #[must_use] pub const fn new() -> Self {
        Self {
            pending_paste_content: None,
            pending_copy_content: None,
        }
    }

    // Paste Operations (System → Application)

    /// Sets content from the system clipboard (called before paste callbacks).
    pub fn set_paste_content(&mut self, content: ClipboardContent) {
        self.pending_paste_content = Some(content);
    }

    /// Returns the pending paste content, if any.
    #[must_use] pub const fn get_paste_content(&self) -> Option<&ClipboardContent> {
        self.pending_paste_content.as_ref()
    }

    // Copy Operations (Application → System)

    /// Sets content to be copied to the system clipboard.
    pub fn set_copy_content(&mut self, content: ClipboardContent) {
        self.pending_copy_content = Some(content);
    }

    /// Returns the pending copy content, if any.
    #[must_use] pub const fn get_copy_content(&self) -> Option<&ClipboardContent> {
        self.pending_copy_content.as_ref()
    }

    /// Takes the copy content, consuming it.
    pub const fn take_copy_content(&mut self) -> Option<ClipboardContent> {
        self.pending_copy_content.take()
    }

    // Lifecycle Management

    /// Clears all pending clipboard content.
    pub fn clear(&mut self) {
        self.pending_paste_content = None;
        self.pending_copy_content = None;
    }

    /// Clears only paste content.
    pub fn clear_paste(&mut self) {
        self.pending_paste_content = None;
    }

    /// Clears only copy content.
    pub fn clear_copy(&mut self) {
        self.pending_copy_content = None;
    }

    /// Returns `true` if there's pending paste content.
    #[must_use] pub const fn has_paste_content(&self) -> bool {
        self.pending_paste_content.is_some()
    }

    /// Returns `true` if there's pending copy content.
    #[must_use] pub const fn has_copy_content(&self) -> bool {
        self.pending_copy_content.is_some()
    }
}

#[cfg(test)]
mod autotest_generated {
    use azul_css::{props::basic::ColorU, AzString, OptionString};

    use super::*;
    use crate::managers::selection::StyledTextRun;

    // =========================================================================
    // Fixtures
    //
    // `ClipboardManager` is a two-slot `Option` cell, so the adversarial
    // surface is not arithmetic but *ownership and state*: the payload
    // (`ClipboardContent`) owns FFI vectors (`AzString`/`StyledTextRunVec`)
    // with destructor function pointers, so every set / take / clear / clone
    // is a chance for a double-free, a shallow clone that aliases a heap
    // buffer, or a slot leaking into the wrong slot. The tests below therefore
    // hammer: slot isolation, take-consumes semantics, deep-clone
    // independence + use-after-free, hostile payloads (1 MiB text, NUL bytes,
    // emoji/RTL/combining marks, NaN / infinite / negative-zero font sizes),
    // and a model-based state machine over the whole API.
    // =========================================================================

    /// Plain-text clipboard payload with no styled runs (what every live
    /// producer in the engine currently builds).
    fn plain(text: &str) -> ClipboardContent {
        ClipboardContent {
            plain_text: AzString::from(text),
            styled_runs: Vec::<StyledTextRun>::new().into(),
        }
    }

    /// A single styled run, parameterized on the numerically hostile field.
    fn run(text: &str, font_size_px: f32, family: Option<&str>) -> StyledTextRun {
        StyledTextRun {
            text: AzString::from(text),
            font_family: family.map_or(OptionString::None, |f| {
                OptionString::Some(AzString::from(f))
            }),
            font_size_px,
            color: ColorU {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
            is_bold: true,
            is_italic: false,
        }
    }

    /// Rich clipboard payload carrying `runs`.
    fn rich(text: &str, runs: Vec<StyledTextRun>) -> ClipboardContent {
        ClipboardContent {
            plain_text: AzString::from(text),
            styled_runs: runs.into(),
        }
    }

    /// Strings that have historically broken UTF-8 / FFI string handling.
    fn hostile_strings() -> Vec<String> {
        vec![
            String::new(),                                  // empty
            "\0".to_string(),                               // lone NUL
            "a\0b\0\0c".to_string(),                        // interior NULs
            "\r\n\t\x0b\x0c\x1b[0m".to_string(),            // control chars + ANSI
            "👨‍👩‍👧‍👦".to_string(),                            // ZWJ emoji family
            "e\u{0301}\u{0301}\u{0301}".to_string(),        // stacked combining marks
            "مرحبا بالعالم".to_string(),                    // RTL
            "\u{202e}reversed\u{202c}".to_string(),         // bidi override
            "\u{feff}bom".to_string(),                      // BOM
            "𝕬𝖟𝖚𝖑".to_string(),                            // 4-byte codepoints
            "\u{10ffff}".to_string(),                       // max scalar value
            "line1\nline2\r\nline3".to_string(),            // mixed newlines
            "x".repeat(1024 * 1024),                        // 1 MiB
            "🦀".repeat(100_000),                           // 400 KiB of 4-byte chars
        ]
    }

    /// NaN-tolerant structural comparison: `ClipboardContent` derives
    /// `PartialEq`, but `StyledTextRun::font_size_px` is an `f32`, so `==` is
    /// not reflexive once a NaN is in play. Compare bit patterns instead.
    fn content_eq_bitwise(a: &ClipboardContent, b: &ClipboardContent) -> bool {
        if a.plain_text.as_str() != b.plain_text.as_str() {
            return false;
        }
        let (ra, rb) = (a.styled_runs.as_slice(), b.styled_runs.as_slice());
        ra.len() == rb.len()
            && ra.iter().zip(rb.iter()).all(|(x, y)| {
                x.text.as_str() == y.text.as_str()
                    && x.font_family == y.font_family
                    && x.font_size_px.to_bits() == y.font_size_px.to_bits()
                    && x.color == y.color
                    && x.is_bold == y.is_bold
                    && x.is_italic == y.is_italic
            })
    }

    // =========================================================================
    // 1. Constructor + invariants
    // =========================================================================

    #[test]
    fn new_starts_empty_on_both_slots() {
        let m = ClipboardManager::new();
        assert!(m.get_paste_content().is_none());
        assert!(m.get_copy_content().is_none());
        assert!(!m.has_paste_content());
        assert!(!m.has_copy_content());
    }

    #[test]
    fn new_is_usable_in_const_context() {
        // `new()` is declared `const fn`; if that ever regresses this stops
        // compiling rather than silently becoming a runtime constructor.
        const EMPTY: ClipboardManager = ClipboardManager::new();
        assert!(!EMPTY.has_paste_content());
        assert!(!EMPTY.has_copy_content());
    }

    #[test]
    fn default_is_indistinguishable_from_new() {
        let d = ClipboardManager::default();
        let n = ClipboardManager::new();
        assert_eq!(d.has_paste_content(), n.has_paste_content());
        assert_eq!(d.has_copy_content(), n.has_copy_content());
        assert_eq!(d.get_paste_content(), n.get_paste_content());
        assert_eq!(d.get_copy_content(), n.get_copy_content());
    }

    // =========================================================================
    // 2. Round-trip: what goes in comes back out, byte for byte
    // =========================================================================

    #[test]
    fn paste_roundtrip_preserves_hostile_payloads() {
        for s in hostile_strings() {
            let mut m = ClipboardManager::new();
            m.set_paste_content(plain(&s));

            let got = m.get_paste_content().expect("paste content must be set");
            assert_eq!(
                got.plain_text.as_str(),
                s.as_str(),
                "paste payload mutated (len {})",
                s.len()
            );
            assert_eq!(got.plain_text.as_str().len(), s.len(), "byte length changed");
            assert!(m.has_paste_content());
            // The copy slot must stay untouched by a paste write.
            assert!(!m.has_copy_content());
        }
    }

    #[test]
    fn copy_roundtrip_preserves_hostile_payloads() {
        for s in hostile_strings() {
            let mut m = ClipboardManager::new();
            m.set_copy_content(plain(&s));

            assert_eq!(
                m.get_copy_content()
                    .expect("copy content must be set")
                    .plain_text
                    .as_str(),
                s.as_str()
            );
            // take() must hand back exactly what was put in.
            let taken = m.take_copy_content().expect("take must yield the content");
            assert_eq!(taken.plain_text.as_str(), s.as_str());
            assert_eq!(taken.plain_text.as_str().chars().count(), s.chars().count());
            assert!(!m.has_paste_content());
        }
    }

    #[test]
    fn empty_string_content_is_still_present_content() {
        // Presence, not emptiness: an empty selection copied to the clipboard
        // must still register as "there is content", otherwise the copy path
        // would silently fall back to a stale system clipboard.
        let mut m = ClipboardManager::new();
        m.set_paste_content(plain(""));
        m.set_copy_content(plain(""));

        assert!(m.has_paste_content());
        assert!(m.has_copy_content());
        assert_eq!(m.get_paste_content().unwrap().plain_text.as_str(), "");
        assert_eq!(m.get_copy_content().unwrap().plain_text.as_str(), "");
    }

    #[test]
    fn one_mib_payload_survives_a_full_set_take_cycle() {
        let big = "az".repeat(512 * 1024); // exactly 1 MiB
        let mut m = ClipboardManager::new();
        m.set_copy_content(plain(&big));

        let taken = m.take_copy_content().expect("1 MiB payload must round-trip");
        assert_eq!(taken.plain_text.as_str().len(), 1024 * 1024);
        assert_eq!(taken.plain_text.as_str(), big.as_str());
        assert!(!m.has_copy_content());
    }

    #[test]
    fn styled_runs_roundtrip_intact() {
        let runs = vec![
            run("hello", 12.0, Some("Arial")),
            run("", 0.0, None),
            run("🦀", 999.5, Some("")),
        ];
        let content = rich("hello🦀", runs);

        let mut m = ClipboardManager::new();
        m.set_copy_content(content.clone());

        let got = m.get_copy_content().expect("rich content must be set");
        assert_eq!(got.styled_runs.as_slice().len(), 3);
        assert!(content_eq_bitwise(got, &content));
        // Derived PartialEq must agree with the structural compare when no NaN
        // is involved.
        assert_eq!(*got, content);

        let taken = m.take_copy_content().unwrap();
        assert!(content_eq_bitwise(&taken, &content));
    }

    #[test]
    fn many_styled_runs_roundtrip() {
        let runs: Vec<StyledTextRun> = (0..5_000)
            .map(|i| run(&format!("run-{i}"), i as f32, Some("Font")))
            .collect();
        let content = rich("many", runs);

        let mut m = ClipboardManager::new();
        m.set_paste_content(content.clone());

        let got = m.get_paste_content().unwrap();
        assert_eq!(got.styled_runs.as_slice().len(), 5_000);
        assert_eq!(got.styled_runs.as_slice()[4_999].text.as_str(), "run-4999");
        assert!(content_eq_bitwise(got, &content));
    }

    // =========================================================================
    // 3. Numeric hostility carried through the manager
    // =========================================================================

    #[test]
    fn extreme_font_sizes_pass_through_bit_exact() {
        let extremes = [
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
            f32::EPSILON,
            0.0_f32,
            -0.0_f32,
            -1.0_f32,
        ];

        for size in extremes {
            let content = rich("x", vec![run("x", size, None)]);
            let mut m = ClipboardManager::new();
            m.set_copy_content(content.clone());

            let got = m.take_copy_content().expect("content must survive");
            let stored = got.styled_runs.as_slice()[0].font_size_px;
            // `to_bits` so that -0.0 != 0.0 is actually caught (they compare
            // equal under `==`).
            assert_eq!(
                stored.to_bits(),
                size.to_bits(),
                "font_size_px {size} was not preserved bit-exactly"
            );
        }
    }

    #[test]
    fn nan_font_size_survives_but_breaks_derived_equality() {
        let content = rich("nan", vec![run("nan", f32::NAN, Some("F"))]);
        let mut m = ClipboardManager::new();
        m.set_paste_content(content.clone());

        let got = m.get_paste_content().expect("NaN payload must still be stored");
        assert!(
            got.styled_runs.as_slice()[0].font_size_px.is_nan(),
            "NaN font size must be stored as-is, not normalized"
        );
        assert!(content_eq_bitwise(got, &content));

        // Documented consequence: derived PartialEq on ClipboardContent is not
        // reflexive once a NaN run is present. Callers must not use `==` on
        // clipboard content to detect "unchanged" when styled runs are in play.
        assert_ne!(content, content.clone());
        assert_ne!(m.get_paste_content(), Some(&content));

        // Predicates are unaffected by the payload's numeric contents.
        assert!(m.has_paste_content());
    }

    #[test]
    fn extreme_colors_pass_through() {
        for (r, g, b, a) in [(0, 0, 0, 0), (255, 255, 255, 255), (0, 255, 0, 1)] {
            let mut st = run("c", 1.0, None);
            st.color = ColorU { r, g, b, a };
            let mut m = ClipboardManager::new();
            m.set_copy_content(rich("c", vec![st]));

            let got = m.take_copy_content().unwrap();
            assert_eq!(got.styled_runs.as_slice()[0].color, ColorU { r, g, b, a });
        }
    }

    // =========================================================================
    // 4. take / clear semantics + slot isolation
    // =========================================================================

    #[test]
    fn take_copy_content_consumes_exactly_once() {
        let mut m = ClipboardManager::new();
        m.set_copy_content(plain("once"));
        assert!(m.has_copy_content());

        assert_eq!(
            m.take_copy_content().unwrap().plain_text.as_str(),
            "once",
            "first take must yield the content"
        );
        assert!(!m.has_copy_content(), "take must consume the slot");
        assert!(m.get_copy_content().is_none());
        assert!(
            m.take_copy_content().is_none(),
            "a second take must not resurrect the content"
        );
        assert!(m.take_copy_content().is_none());
    }

    #[test]
    fn take_on_fresh_manager_returns_none_repeatedly() {
        let mut m = ClipboardManager::new();
        for _ in 0..100 {
            assert!(m.take_copy_content().is_none());
        }
        assert!(!m.has_copy_content());
    }

    #[test]
    fn take_copy_does_not_disturb_paste_slot() {
        let mut m = ClipboardManager::new();
        m.set_paste_content(plain("paste"));
        m.set_copy_content(plain("copy"));

        let taken = m.take_copy_content().unwrap();
        assert_eq!(taken.plain_text.as_str(), "copy");
        assert!(m.has_paste_content(), "taking copy must not clear paste");
        assert_eq!(m.get_paste_content().unwrap().plain_text.as_str(), "paste");
    }

    #[test]
    fn set_overwrites_rather_than_accumulates() {
        let mut m = ClipboardManager::new();
        for i in 0..50 {
            m.set_paste_content(plain(&format!("paste-{i}")));
            m.set_copy_content(plain(&format!("copy-{i}")));
        }
        assert_eq!(m.get_paste_content().unwrap().plain_text.as_str(), "paste-49");
        assert_eq!(m.get_copy_content().unwrap().plain_text.as_str(), "copy-49");

        // And the last write wins for take() too.
        assert_eq!(
            m.take_copy_content().unwrap().plain_text.as_str(),
            "copy-49"
        );
    }

    #[test]
    fn clear_empties_both_slots() {
        let mut m = ClipboardManager::new();
        m.set_paste_content(plain("p"));
        m.set_copy_content(plain("c"));
        m.clear();

        assert!(!m.has_paste_content());
        assert!(!m.has_copy_content());
        assert!(m.get_paste_content().is_none());
        assert!(m.get_copy_content().is_none());
        assert!(m.take_copy_content().is_none());
    }

    #[test]
    fn clear_paste_and_clear_copy_are_slot_isolated() {
        let mut m = ClipboardManager::new();
        m.set_paste_content(plain("p"));
        m.set_copy_content(plain("c"));

        m.clear_paste();
        assert!(!m.has_paste_content());
        assert!(m.has_copy_content(), "clear_paste must not touch the copy slot");
        assert_eq!(m.get_copy_content().unwrap().plain_text.as_str(), "c");

        m.set_paste_content(plain("p2"));
        m.clear_copy();
        assert!(!m.has_copy_content());
        assert!(m.has_paste_content(), "clear_copy must not touch the paste slot");
        assert_eq!(m.get_paste_content().unwrap().plain_text.as_str(), "p2");
    }

    #[test]
    fn clears_are_idempotent_on_an_empty_manager() {
        let mut m = ClipboardManager::new();
        for _ in 0..10 {
            m.clear();
            m.clear_paste();
            m.clear_copy();
        }
        assert!(!m.has_paste_content());
        assert!(!m.has_copy_content());

        // ...and idempotent after a real clear, too.
        m.set_paste_content(plain("x"));
        m.clear_paste();
        m.clear_paste();
        m.clear_paste();
        assert!(m.get_paste_content().is_none());
    }

    // =========================================================================
    // 5. Predicate / getter invariants
    // =========================================================================

    #[test]
    fn predicates_always_agree_with_getters() {
        let mut m = ClipboardManager::new();
        let check = |m: &ClipboardManager| {
            assert_eq!(m.has_paste_content(), m.get_paste_content().is_some());
            assert_eq!(m.has_copy_content(), m.get_copy_content().is_some());
        };

        check(&m);
        m.set_paste_content(plain(""));
        check(&m);
        m.set_copy_content(plain("\0"));
        check(&m);
        m.clear_paste();
        check(&m);
        let _ = m.take_copy_content();
        check(&m);
        m.clear();
        check(&m);
    }

    #[test]
    fn getters_are_stable_across_repeated_reads() {
        let mut m = ClipboardManager::new();
        m.set_paste_content(plain("stable"));

        for _ in 0..1_000 {
            assert_eq!(m.get_paste_content().unwrap().plain_text.as_str(), "stable");
            assert!(m.has_paste_content());
        }
    }

    // =========================================================================
    // 6. Ownership: deep clone, no aliasing, no use-after-free
    // =========================================================================

    #[test]
    fn clone_is_deep_and_independent() {
        let mut original = ClipboardManager::new();
        original.set_paste_content(plain("original-paste"));
        original.set_copy_content(plain("original-copy"));

        let mut cloned = original.clone();
        // Mutating the clone must not reach back into the original.
        cloned.set_paste_content(plain("clone-paste"));
        let _ = cloned.take_copy_content();

        assert_eq!(
            original.get_paste_content().unwrap().plain_text.as_str(),
            "original-paste"
        );
        assert!(
            original.has_copy_content(),
            "taking from the clone must not consume the original's copy slot"
        );
        assert_eq!(
            cloned.get_paste_content().unwrap().plain_text.as_str(),
            "clone-paste"
        );
        assert!(!cloned.has_copy_content());
    }

    #[test]
    fn clone_survives_the_original_being_dropped() {
        // `ClipboardContent` owns FFI vectors with destructor pointers: a
        // shallow clone here would alias the heap buffer and this test would
        // read freed memory / double-free on drop.
        let payload = "🦀".repeat(10_000);
        let cloned = {
            let mut original = ClipboardManager::new();
            original.set_paste_content(plain(&payload));
            original.set_copy_content(rich("rich", vec![run("r", 1.0, Some("Arial"))]));
            let c = original.clone();
            drop(original);
            c
        };

        assert_eq!(
            cloned.get_paste_content().unwrap().plain_text.as_str(),
            payload.as_str()
        );
        assert_eq!(
            cloned.get_copy_content().unwrap().styled_runs.as_slice()[0]
                .text
                .as_str(),
            "r"
        );
        // Dropping the clone afterwards must not double-free.
        drop(cloned);
    }

    #[test]
    fn taken_content_outlives_the_manager() {
        let taken = {
            let mut m = ClipboardManager::new();
            m.set_copy_content(plain("outlives"));
            let t = m.take_copy_content();
            drop(m);
            t
        };
        assert_eq!(taken.unwrap().plain_text.as_str(), "outlives");
    }

    #[test]
    fn dropping_a_loaded_manager_is_clean() {
        for _ in 0..100 {
            let mut m = ClipboardManager::new();
            m.set_paste_content(plain(&"x".repeat(4096)));
            m.set_copy_content(rich("c", vec![run("c", f32::NAN, Some("F"))]));
            // Dropped fully loaded, without any clear() — the destructors must
            // run exactly once each.
            drop(m);
        }
    }

    #[test]
    fn debug_format_does_not_panic_on_hostile_content() {
        let mut m = ClipboardManager::new();
        m.set_paste_content(plain("a\0b\u{202e}\u{feff}🦀"));
        m.set_copy_content(rich("r", vec![run("r", f32::NAN, None)]));

        let s = format!("{m:?}");
        assert!(s.contains("ClipboardManager"));
    }

    // =========================================================================
    // 7. Churn + model-based state machine
    // =========================================================================

    #[test]
    fn ten_thousand_set_take_cycles_leave_no_residue() {
        let mut m = ClipboardManager::new();
        for i in 0..10_000_u32 {
            m.set_copy_content(plain(&format!("c{i}")));
            m.set_paste_content(plain(&format!("p{i}")));

            let taken = m.take_copy_content().expect("copy slot was just filled");
            assert_eq!(taken.plain_text.as_str(), format!("c{i}"));
            assert!(!m.has_copy_content());
            m.clear_paste();
            assert!(!m.has_paste_content());
        }
        assert!(!m.has_paste_content());
        assert!(!m.has_copy_content());
    }

    #[test]
    fn state_machine_matches_a_two_slot_option_model() {
        // Drive every mutator in a deterministic pseudo-random order and check
        // the manager against a trivial `(Option<String>, Option<String>)`
        // model after every single step.
        let mut m = ClipboardManager::new();
        let mut model: (Option<String>, Option<String>) = (None, None);
        let mut seed: u64 = 0x5eed_1234_dead_beef;

        for step in 0..5_000_u32 {
            // xorshift64 — no rand dependency, fully reproducible.
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;

            match seed % 7 {
                0 => {
                    let s = format!("p{step}");
                    m.set_paste_content(plain(&s));
                    model.0 = Some(s);
                }
                1 => {
                    let s = format!("c{step}");
                    m.set_copy_content(plain(&s));
                    model.1 = Some(s);
                }
                2 => {
                    let taken = m.take_copy_content();
                    let expected = model.1.take();
                    assert_eq!(
                        taken.map(|c| c.plain_text.as_str().to_string()),
                        expected,
                        "take_copy_content diverged from the model at step {step}"
                    );
                }
                3 => {
                    m.clear();
                    model = (None, None);
                }
                4 => {
                    m.clear_paste();
                    model.0 = None;
                }
                5 => {
                    m.clear_copy();
                    model.1 = None;
                }
                _ => {
                    // Pure reads must not mutate state.
                    let _ = m.get_paste_content();
                    let _ = m.get_copy_content();
                    let _ = m.has_paste_content();
                    let _ = m.has_copy_content();
                }
            }

            assert_eq!(
                m.get_paste_content().map(|c| c.plain_text.as_str().to_string()),
                model.0,
                "paste slot diverged at step {step}"
            );
            assert_eq!(
                m.get_copy_content().map(|c| c.plain_text.as_str().to_string()),
                model.1,
                "copy slot diverged at step {step}"
            );
            assert_eq!(m.has_paste_content(), model.0.is_some());
            assert_eq!(m.has_copy_content(), model.1.is_some());
        }
    }

    #[test]
    fn documented_paste_then_copy_flow() {
        // The module doc's contract: paste content is set by the platform,
        // read by the callback, then cleared; copy content is set by the
        // callback, taken by the platform, then gone.
        let mut m = ClipboardManager::new();

        // 1. Paste flow: system -> manager -> callback -> clear.
        m.set_paste_content(plain("from system"));
        assert_eq!(
            m.get_paste_content().unwrap().plain_text.as_str(),
            "from system"
        );
        m.clear_paste();
        assert!(!m.has_paste_content());

        // 2. Copy flow: callback overrides -> platform takes -> slot empties.
        m.set_copy_content(plain("default selection"));
        m.set_copy_content(plain("callback override"));
        assert_eq!(
            m.take_copy_content().unwrap().plain_text.as_str(),
            "callback override",
            "the callback's override must win over the default selection"
        );
        assert!(
            !m.has_copy_content(),
            "the copy slot must not leak into the next clipboard operation"
        );
    }
}
