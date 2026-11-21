//! A5, the label flicker on the GPU path.
//!
//! The kept halos themselves are private to `cpurender/text_halo.rs`: their guards
//! (`audit_a5_tests` there, and in `image_memo.rs`) sit next to them.

#[cfg(all(feature = "cpurender", feature = "text_layout"))]
mod halo_counters {
    use std::time::Duration;

    use rust_fontconfig::FcFontCache;

    use crate::{cpurender::TextHaloStats, window::LayoutWindow};

    /// A5-09. What the scans of a window did with the text halos is in the frame
    /// report (`get_frame_report`, and `CallbackInfo::get_text_halo_stats` for an
    /// app's own recorder): a window that has scanned nothing reports nothing, and a
    /// scan that finds no shadow is one scan with nothing drawn, dropped or left.
    #[test]
    fn a5_the_frame_report_carries_the_windows_halo_counters() {
        let window = LayoutWindow::new(FcFontCache::default()).expect("a window");
        assert_eq!(
            window.text_halo_store.text_halo_stats(),
            TextHaloStats::default(),
            "a new window has scanned nothing"
        );
        assert_eq!(window.frame_report_synced().text_halos, TextHaloStats::default());

        let ((), left) = window.text_halo_store.within_halo_budget(2, Duration::ZERO, || {});
        assert_eq!(left, 0, "nothing was left for a later frame");
        let stats = window.frame_report_synced().text_halos;
        assert_eq!(
            stats,
            window.text_halo_store.text_halo_stats(),
            "the report reads the window's counters"
        );
        assert_eq!(stats, window.text_halo_stats());
        assert_eq!(
            stats,
            TextHaloStats {
                scans: 1,
                ..TextHaloStats::default()
            }
        );
    }

    /// The halo counters are a window's own: two windows of one thread (and of one
    /// process) scan, and each report counts the scans of its window alone. (They
    /// were a thread's: the second window's report carried the first one's scans.)
    ///
    /// Mutation: the store of `LayoutWindow::new` made once and shared (a `static`, a
    /// thread-local) gives the second window the first one's scan.
    #[test]
    fn a5_two_windows_report_their_own_halo_counters() {
        let (first, second) = (
            LayoutWindow::new(FcFontCache::default()).expect("a window"),
            LayoutWindow::new(FcFontCache::default()).expect("a window"),
        );
        for _ in 0..3 {
            let _ = first.text_halo_store.within_halo_budget(2, Duration::ZERO, || {});
        }
        let _ = second.text_halo_store.within_halo_budget(2, Duration::ZERO, || {});

        assert_eq!(first.frame_report_synced().text_halos.scans, 3);
        assert_eq!(second.frame_report_synced().text_halos.scans, 1);
        assert_eq!(
            first.text_halo_stats().scans,
            3,
            "and the accessor of the window says the same as its report"
        );
    }
}
