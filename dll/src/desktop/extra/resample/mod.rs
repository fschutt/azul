//! Platform-accelerated whole-frame resampling behind the
//! `capture_common::register_frame_resampler` seam.
//!
//! The portable scaler (`azul_layout::image_scale::resample_rgba`) is the
//! reference: every backend here must produce the same picture within
//! rounding, because the capture fan-out may call it per consumer on any
//! thread and the tests pin the portable one. A backend is a PURE function
//! of its inputs — no state, no threads of its own — so the caller decides
//! what runs in parallel.
//!
//! macOS: Accelerate's vImage (`vImageScale_ARGB8888`), Apple's vectorised
//! (NEON / AVX) and internally multithreaded scaler — the supported fast
//! path short of a Metal pipeline, and present on every macOS. Other
//! platforms keep the portable scaler until a backend lands (Windows: WIC /
//! D2D; Linux: pixman / GL).

#[cfg(target_os = "macos")]
pub mod macos;

/// Register the platform scaler once (`OnceLock`-guarded like the capture
/// backends). A no-op where no backend exists.
pub fn ensure_frame_resampler() {
    #[cfg(target_os = "macos")]
    {
        static DONE: std::sync::OnceLock<()> = std::sync::OnceLock::new();
        DONE.get_or_init(|| {
            crate::plog_info!("[resample] registering Accelerate/vImage frame scaler");
            azul_layout::widgets::capture_common::register_frame_resampler(macos::resample_rgba);
        });
    }
}
