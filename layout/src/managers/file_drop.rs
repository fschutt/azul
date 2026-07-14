//! **File** drag & drop management
//!
//! Manages hovered files (drag-and-drop).

use alloc::vec::Vec;

use azul_css::AzString;

/// Manager for file drop state and hovered file tracking.
///
/// MWA-B7: stores ALL files of a drag/drop (multi-file drops were silently
/// truncated to the first path at every OS ingress site — the manager could
/// only hold one). The single-file accessors remain as first-element views
/// for existing callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDropManager {
    /// Files being hovered during the drag operation (empty = no hover).
    hovered_files: Vec<AzString>,
    /// Files that were dropped (cleared after one frame).
    dropped_files: Vec<AzString>,
    /// One-shot flag set when a hover ends without a drop (a non-empty →
    /// empty transition). Read by `determine_all_events` to emit
    /// `EventType::FileHoverCancel`, then cleared by the platform drag
    /// handler (one-shot, mirrors the dropped-files reset).
    hover_cancelled: bool,
}

impl FileDropManager {
    /// Create a new file drop manager
    #[must_use] pub const fn new() -> Self {
        Self {
            hovered_files: Vec::new(),
            dropped_files: Vec::new(),
            hover_cancelled: false,
        }
    }

    /// Set ALL currently hovered files (MWA-B7). An empty vec behaves like a
    /// drag-leave (latches the hover-cancel flag).
    pub fn set_hovered_files(&mut self, files: Vec<AzString>) {
        if files.is_empty() {
            if !self.hovered_files.is_empty() {
                self.hover_cancelled = true;
            }
            self.hovered_files.clear();
        } else {
            self.hovered_files = files;
        }
    }

    /// Single-file compatibility shim over [`set_hovered_files`](Self::set_hovered_files).
    ///
    /// Platform backends call this with `Some(path)` on drag-enter
    /// (macOS `draggingEntered`, Windows OLE `IDropTarget::DragEnter`) and
    /// `None` on drag-leave (`draggingExited` / `DragLeave`). A `Some` -> `None`
    /// transition latches [`FileDropManager::hover_was_cancelled`] so the
    /// `FileHoverCancel` event can fire.
    pub fn set_hovered_file(&mut self, file: Option<AzString>) {
        match file {
            Some(f) => self.set_hovered_files(alloc::vec![f]),
            None => self.set_hovered_files(Vec::new()),
        }
    }

    /// Whether a hover ended without a drop since the last
    /// [`FileDropManager::clear_hover_cancelled`] (one-shot).
    #[must_use] pub const fn hover_was_cancelled(&self) -> bool {
        self.hover_cancelled
    }

    /// Clear the one-shot hover-cancel flag. Called by the platform drag
    /// handler after `determine_all_events` has emitted the `FileHoverCancel`
    /// event (mirrors the dropped-files reset after `FileDrop`).
    pub const fn clear_hover_cancelled(&mut self) {
        self.hover_cancelled = false;
    }

    /// First hovered file (single-file view; use
    /// [`get_hovered_files`](Self::get_hovered_files) for the full list).
    #[must_use] pub fn get_hovered_file(&self) -> Option<&AzString> {
        self.hovered_files.first()
    }

    /// ALL currently hovered files (MWA-B7).
    #[must_use] pub fn get_hovered_files(&self) -> &[AzString] {
        &self.hovered_files
    }

    /// First dropped file (single-file view; use
    /// [`get_dropped_files`](Self::get_dropped_files) for the full list).
    #[must_use] pub fn get_dropped_file(&self) -> Option<&AzString> {
        self.dropped_files.first()
    }

    /// ALL files of the drop this frame (MWA-B7; one-shot, cleared by the
    /// platform handler after event processing).
    #[must_use] pub fn get_dropped_files(&self) -> &[AzString] {
        &self.dropped_files
    }

    /// Set ALL dropped files (MWA-B7; cleared after one frame).
    pub fn set_dropped_files(&mut self, files: Vec<AzString>) {
        self.dropped_files = files;
    }

    /// Single-file compatibility shim over [`set_dropped_files`](Self::set_dropped_files).
    pub fn set_dropped_file(&mut self, file: Option<AzString>) {
        match file {
            Some(f) => self.dropped_files = alloc::vec![f],
            None => self.dropped_files.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> AzString {
        AzString::from(v.to_string())
    }

    #[test]
    fn multi_file_drop_keeps_every_path() {
        let mut m = FileDropManager::new();
        m.set_dropped_files(vec![s("/a"), s("/b"), s("/c")]);
        assert_eq!(m.get_dropped_files().len(), 3);
        assert_eq!(m.get_dropped_file().map(AzString::as_str), Some("/a"));
        m.set_dropped_file(None);
        assert!(m.get_dropped_files().is_empty());
    }

    #[test]
    fn hover_cancel_latches_on_empty_transition() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/a"), s("/b")]);
        assert_eq!(m.get_hovered_files().len(), 2);
        assert!(!m.hover_was_cancelled());
        m.set_hovered_files(Vec::new());
        assert!(m.hover_was_cancelled());
        m.clear_hover_cancelled();
        assert!(!m.hover_was_cancelled());
    }
}

#[cfg(test)]
mod autotest_generated {
    use super::*;

    fn s(v: &str) -> AzString {
        AzString::from(v.to_string())
    }

    /// Every getter is consistent with the manager's internal vecs: the
    /// single-file views are exactly `.first()` of the corresponding slice.
    fn assert_invariants(m: &FileDropManager) {
        assert_eq!(m.get_hovered_file(), m.get_hovered_files().first());
        assert_eq!(m.get_dropped_file(), m.get_dropped_files().first());
        assert_eq!(
            m.get_hovered_file().is_none(),
            m.get_hovered_files().is_empty()
        );
        assert_eq!(
            m.get_dropped_file().is_none(),
            m.get_dropped_files().is_empty()
        );
    }

    // ---------------------------------------------------------------
    // constructor
    // ---------------------------------------------------------------

    #[test]
    fn new_is_empty_and_matches_default() {
        let m = FileDropManager::new();
        assert!(m.get_hovered_files().is_empty());
        assert!(m.get_dropped_files().is_empty());
        assert_eq!(m.get_hovered_file(), None);
        assert_eq!(m.get_dropped_file(), None);
        assert!(!m.hover_was_cancelled());
        assert_eq!(m, FileDropManager::default());
        assert_invariants(&m);
    }

    #[test]
    fn new_is_usable_in_const_context() {
        const M: FileDropManager = FileDropManager::new();
        assert!(!M.hover_was_cancelled());
        assert!(M.get_hovered_files().is_empty());
        assert!(M.get_dropped_files().is_empty());
    }

    #[test]
    fn getters_on_default_instance_do_not_panic() {
        let m = FileDropManager::default();
        assert_invariants(&m);
        // Repeated reads of the one-shot flag are pure (no state change).
        assert!(!m.hover_was_cancelled());
        assert!(!m.hover_was_cancelled());
    }

    // ---------------------------------------------------------------
    // hover-cancel latch semantics (the one-shot flag)
    // ---------------------------------------------------------------

    #[test]
    fn empty_hover_set_on_fresh_manager_does_not_latch_cancel() {
        // No non-empty -> empty transition happened, so nothing to cancel.
        let mut m = FileDropManager::new();
        m.set_hovered_files(Vec::new());
        assert!(!m.hover_was_cancelled());
        m.set_hovered_files(Vec::new());
        assert!(!m.hover_was_cancelled());
        m.set_hovered_file(None);
        assert!(!m.hover_was_cancelled());
        assert_invariants(&m);
    }

    #[test]
    fn second_empty_hover_set_after_a_latch_keeps_the_flag() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/a")]);
        m.set_hovered_files(Vec::new()); // latches
        assert!(m.hover_was_cancelled());
        // The vec is already empty, so no *new* transition occurs — but the
        // latched flag must survive until it is explicitly cleared.
        m.set_hovered_files(Vec::new());
        assert!(m.hover_was_cancelled());
        m.clear_hover_cancelled();
        assert!(!m.hover_was_cancelled());
    }

    #[test]
    fn nonempty_to_nonempty_hover_never_latches_cancel() {
        let mut m = FileDropManager::new();
        for i in 0..64 {
            m.set_hovered_files(vec![s(&format!("/f{i}"))]);
            assert!(!m.hover_was_cancelled());
        }
        assert_eq!(m.get_hovered_files().len(), 1, "sets replace, never append");
        assert_eq!(m.get_hovered_file().map(AzString::as_str), Some("/f63"));
    }

    #[test]
    fn re_entering_hover_does_not_clear_a_pending_cancel() {
        // Documented one-shot semantics: only `clear_hover_cancelled` clears
        // the flag. A leave -> re-enter sequence within one frame therefore
        // leaves the cancel pending *and* a live hover.
        let mut m = FileDropManager::new();
        m.set_hovered_file(Some(s("/a")));
        m.set_hovered_file(None);
        assert!(m.hover_was_cancelled());
        m.set_hovered_file(Some(s("/b")));
        assert!(m.hover_was_cancelled());
        assert_eq!(m.get_hovered_file().map(AzString::as_str), Some("/b"));
    }

    #[test]
    fn clear_hover_cancelled_is_idempotent_and_leaves_files_alone() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/a"), s("/b")]);
        m.set_dropped_files(vec![s("/d")]);
        m.clear_hover_cancelled();
        m.clear_hover_cancelled();
        assert!(!m.hover_was_cancelled());
        assert_eq!(m.get_hovered_files().len(), 2);
        assert_eq!(m.get_dropped_files().len(), 1);
        assert_invariants(&m);
    }

    #[test]
    fn dropped_file_setters_never_touch_the_hover_cancel_flag() {
        let mut m = FileDropManager::new();
        m.set_dropped_files(vec![s("/a")]);
        m.set_dropped_files(Vec::new());
        m.set_dropped_file(Some(s("/b")));
        m.set_dropped_file(None);
        assert!(!m.hover_was_cancelled());
        // ... and a latched flag is not cleared by drop activity either.
        m.set_hovered_files(vec![s("/h")]);
        m.set_hovered_files(Vec::new());
        assert!(m.hover_was_cancelled());
        m.set_dropped_files(vec![s("/c")]);
        m.set_dropped_file(None);
        assert!(m.hover_was_cancelled());
    }

    #[test]
    fn drop_does_not_implicitly_end_the_hover() {
        // A real OS sequence: drag-enter, drop, then drag-exit. The manager
        // keeps the hovered paths across the drop; only the explicit exit
        // (empty hover set) latches the cancel.
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/a")]);
        m.set_dropped_files(vec![s("/a")]);
        assert_eq!(m.get_hovered_files().len(), 1);
        assert!(!m.hover_was_cancelled());
        m.set_hovered_files(Vec::new());
        assert!(m.hover_was_cancelled());
        assert!(m.get_hovered_files().is_empty());
        assert_eq!(m.get_dropped_files().len(), 1, "drop survives the exit");
    }

    // ---------------------------------------------------------------
    // round-trips: set == get, order + duplicates preserved
    // ---------------------------------------------------------------

    #[test]
    fn hovered_round_trip_preserves_order_and_duplicates() {
        let files = vec![s("/z"), s("/a"), s("/z"), s(""), s("/a")];
        let mut m = FileDropManager::new();
        m.set_hovered_files(files.clone());
        assert_eq!(m.get_hovered_files(), &files[..]);
        assert_eq!(m.get_hovered_file(), Some(&files[0]));
        assert_invariants(&m);
    }

    #[test]
    fn dropped_round_trip_preserves_order_and_duplicates() {
        let files = vec![s("/z"), s("/a"), s("/z"), s(""), s("/a")];
        let mut m = FileDropManager::new();
        m.set_dropped_files(files.clone());
        assert_eq!(m.get_dropped_files(), &files[..]);
        assert_eq!(m.get_dropped_file(), Some(&files[0]));
        assert_invariants(&m);
    }

    #[test]
    fn single_file_shims_agree_with_the_multi_file_setters() {
        let mut shim = FileDropManager::new();
        let mut multi = FileDropManager::new();

        shim.set_hovered_file(Some(s("/p")));
        multi.set_hovered_files(vec![s("/p")]);
        shim.set_dropped_file(Some(s("/q")));
        multi.set_dropped_files(vec![s("/q")]);
        assert_eq!(shim, multi);

        shim.set_hovered_file(None);
        multi.set_hovered_files(Vec::new());
        shim.set_dropped_file(None);
        multi.set_dropped_files(Vec::new());
        assert_eq!(shim, multi);
        assert!(shim.hover_was_cancelled() && multi.hover_was_cancelled());
    }

    #[test]
    fn hovered_and_dropped_state_are_independent() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/h1"), s("/h2")]);
        m.set_dropped_files(vec![s("/d1")]);
        assert_eq!(m.get_hovered_files().len(), 2);
        assert_eq!(m.get_dropped_files().len(), 1);

        m.set_dropped_file(None); // one-shot drop reset
        assert!(m.get_dropped_files().is_empty());
        assert_eq!(m.get_hovered_files().len(), 2, "hover untouched by reset");
        assert_invariants(&m);
    }

    // ---------------------------------------------------------------
    // adversarial payloads: empty / unicode / NUL / huge / many
    // ---------------------------------------------------------------

    #[test]
    fn a_single_empty_path_is_a_hover_not_a_drag_leave() {
        // `vec![""]` is NON-empty, so it must NOT latch the cancel flag — an
        // empty *path* and an empty *list* are different things.
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("")]);
        assert!(!m.hover_was_cancelled());
        assert_eq!(m.get_hovered_files().len(), 1);
        assert_eq!(m.get_hovered_file().map(AzString::as_str), Some(""));

        let mut shim = FileDropManager::new();
        shim.set_hovered_file(Some(s("")));
        assert!(!shim.hover_was_cancelled());
        assert_eq!(shim.get_hovered_file().map(AzString::as_str), Some(""));
        assert_eq!(m, shim);
    }

    #[test]
    fn unicode_paths_round_trip_byte_exact() {
        let paths = [
            "/tmp/файл.txt",                       // Cyrillic
            "/tmp/文件/資料.csv",                  // CJK
            "/tmp/🦀/emoji 💾.bin",                // astral plane
            "/tmp/\u{202E}gpj.exe",                // RTL override (spoofing)
            "/tmp/e\u{0301}\u{0301}\u{0301}.txt",  // stacked combining marks
            "/tmp/zero\u{200B}width.txt",          // zero-width space
            "/tmp/\u{FFFD}replacement.txt",        // U+FFFD
            "  /tmp/ leading and trailing  ",      // whitespace kept verbatim
            "/tmp/newline\nand\ttab.txt",          // control chars
        ];
        let files: Vec<AzString> = paths.iter().map(|p| s(p)).collect();

        let mut m = FileDropManager::new();
        m.set_hovered_files(files.clone());
        m.set_dropped_files(files.clone());
        for (i, p) in paths.iter().enumerate() {
            assert_eq!(m.get_hovered_files()[i].as_str(), *p);
            assert_eq!(m.get_dropped_files()[i].as_str(), *p);
            assert_eq!(m.get_hovered_files()[i].as_str().len(), p.len());
        }
        assert_invariants(&m);
    }

    #[test]
    fn path_with_interior_nul_byte_is_preserved() {
        // Rust strings may contain NUL; nothing here goes through a C string,
        // so the byte must survive the round-trip untruncated.
        let path = "/tmp/a\0b/c\0.txt";
        let mut m = FileDropManager::new();
        m.set_dropped_file(Some(s(path)));
        let got = m.get_dropped_file().expect("dropped file present");
        assert_eq!(got.as_str(), path);
        assert_eq!(got.as_str().len(), path.len());
        assert_eq!(got.as_str().matches('\0').count(), 2);
    }

    #[test]
    fn huge_path_round_trips_without_truncation() {
        // 256 KiB of path, mixed ASCII + multi-byte.
        let mut path = String::from("/tmp/");
        for _ in 0..32_768 {
            path.push_str("aä🦀/"); // 1 + 2 + 4 + 1 = 8 bytes per iteration
        }
        path.push_str("end.txt");
        let byte_len = path.len();
        assert!(byte_len > 256 * 1024);

        let mut m = FileDropManager::new();
        m.set_hovered_file(Some(AzString::from(path.clone())));
        let got = m.get_hovered_file().expect("hovered file present");
        assert_eq!(got.as_str().len(), byte_len);
        assert!(got.as_str().ends_with("end.txt"));
        assert_eq!(got.as_str(), path.as_str());
        assert!(!m.hover_was_cancelled());
    }

    #[test]
    fn ten_thousand_dropped_files_keep_length_and_order() {
        let n = 10_000usize;
        let files: Vec<AzString> = (0..n).map(|i| s(&format!("/f/{i}"))).collect();
        let mut m = FileDropManager::new();
        m.set_dropped_files(files);

        assert_eq!(m.get_dropped_files().len(), n);
        assert_eq!(m.get_dropped_file().map(AzString::as_str), Some("/f/0"));
        assert_eq!(m.get_dropped_files()[n - 1].as_str(), "/f/9999");
        assert_invariants(&m);

        m.set_dropped_files(Vec::new());
        assert!(m.get_dropped_files().is_empty());
        assert_eq!(m.get_dropped_file(), None);
        assert!(!m.hover_was_cancelled(), "drops never latch hover-cancel");
    }

    #[test]
    fn many_hover_enter_leave_cycles_stay_consistent() {
        let mut m = FileDropManager::new();
        for i in 0..1_000 {
            m.set_hovered_files(vec![s(&format!("/f{i}")), s("/other")]);
            assert_eq!(m.get_hovered_files().len(), 2, "no accumulation");
            assert!(!m.hover_was_cancelled(), "cleared at the end of each cycle");
            m.set_hovered_files(Vec::new());
            assert!(m.hover_was_cancelled());
            assert!(m.get_hovered_files().is_empty());
            assert_eq!(m.get_hovered_file(), None);
            m.clear_hover_cancelled();
        }
        assert_eq!(m, FileDropManager::new());
    }

    // ---------------------------------------------------------------
    // derived-trait invariants
    // ---------------------------------------------------------------

    #[test]
    fn clone_equals_original_and_is_independent() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/a"), s("/b")]);
        m.set_dropped_files(vec![s("/c")]);
        m.set_hovered_files(Vec::new()); // latch the flag too

        let c = m.clone();
        assert_eq!(c, m);

        // Mutating the clone must not touch the original.
        let mut c2 = c.clone();
        c2.clear_hover_cancelled();
        c2.set_dropped_files(Vec::new());
        assert!(m.hover_was_cancelled());
        assert_eq!(m.get_dropped_files().len(), 1);
        assert_ne!(c2, m);
    }

    #[test]
    fn equality_accounts_for_the_hover_cancel_flag() {
        let mut latched = FileDropManager::new();
        latched.set_hovered_files(vec![s("/a")]);
        latched.set_hovered_files(Vec::new());

        let fresh = FileDropManager::new();
        // Same (empty) file lists, different flag => not equal.
        assert_eq!(latched.get_hovered_files(), fresh.get_hovered_files());
        assert_eq!(latched.get_dropped_files(), fresh.get_dropped_files());
        assert_ne!(latched, fresh);

        let mut cleared = latched.clone();
        cleared.clear_hover_cancelled();
        assert_eq!(cleared, fresh);
    }

    #[test]
    fn debug_output_contains_the_paths() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/tmp/🦀.txt")]);
        let dbg = alloc::format!("{m:?}");
        assert!(dbg.contains("/tmp/🦀.txt"), "unexpected Debug output: {dbg}");
        assert!(dbg.contains("hover_cancelled"));
    }
}
