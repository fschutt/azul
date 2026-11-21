//! A5, the label flicker on the GPU path.
//!
//! The kept halos themselves are private to `cpurender/text_halo.rs`: their guards
//! (`audit_a5_tests` there, and in `image_memo.rs`) sit next to them.

#[cfg(all(feature = "cpurender", feature = "text_layout"))]
mod halo_counters {
    use std::time::Duration;

    use rust_fontconfig::FcFontCache;

    use crate::{
        cpurender::{text_halo_stats, within_halo_budget, TextHaloStats},
        window::LayoutWindow,
    };

    /// A5-09. What the scans of the thread that builds the frames did with the
    /// text halos is in the frame report (`get_frame_report`, and
    /// `CallbackInfo::get_text_halo_stats` for an app's own recorder): a thread
    /// that has scanned nothing reports nothing, and a scan that finds no shadow
    /// is one scan with nothing drawn, dropped or left.
    #[test]
    fn a5_the_frame_report_carries_the_threads_halo_counters() {
        assert_eq!(text_halo_stats(), TextHaloStats::default(), "a new thread has scanned nothing");
        let window = LayoutWindow::new(FcFontCache::default()).expect("a window");
        assert_eq!(window.frame_report_synced().text_halos, TextHaloStats::default());

        let ((), left) = within_halo_budget(2, Duration::ZERO, || {});
        assert_eq!(left, 0, "nothing was left for a later frame");
        let stats = window.frame_report_synced().text_halos;
        assert_eq!(stats, text_halo_stats(), "the report reads the thread's counters");
        assert_eq!(
            stats,
            TextHaloStats {
                scans: 1,
                ..TextHaloStats::default()
            }
        );
    }
}
