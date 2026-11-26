//! The GPU path of the Android window: what the shell's events and its frame work call (`common::gles_gpu` is the
//! executor of the lifecycle reducer; this file is only the seam between it and `AndroidWindow`). One `impl AndroidWindow`
//! block, so that `mod.rs` changes by a few call sites.
//!
//! A window that asks for the CPU renderer never creates the executor: the loop's paths are exactly what they were. A window that asks
//! for the GPU (`AZ_BACKEND=gpu|auto`, `hw_accel`) takes the surface at `InitWindow` BEFORE any CPU frame is drawn on it (an
//! `ANativeWindow` has one producer: once `ANativeWindow_lock` posted, EGL cannot connect), makes its frames with WebRender
//! (`gpu_submit`), presents them when WebRender says a frame is built (`gpu_present`), and on any failure becomes a CPU window
//! with one diagnostic line (`after_gpu`).

use std::{ffi::c_void, path::PathBuf, sync::Arc};

use azul_layout::window_state::WindowCreateOptions;

#[cfg(feature = "ndk")]
use crate::desktop::shell2::common::gpu_lifecycle::Surface;

use super::{AndroidWindow, RenderBackend};
use crate::{
    desktop::shell2::common::{
        debug_server::LogCategory,
        gles_gpu::{
            request_for, trim_level_for_android, GlesGpu, GlesGpuConfig, GpuCtx, Outcome,
        },
        gpu_renderer,
        gpu_select::{decide, FailPlan, GpuRequest},
    },
    log_info,
};

impl AndroidWindow {
    /// `AndroidWindow::new`: what the window asks of the renderer. A CPU request writes the contract line at once and changes nothing
    /// else (the path is exactly today's); a GPU request builds the executor, which brings the path up at `InitWindow`.
    pub(super) fn init_gpu(&mut self, options: &WindowCreateOptions) {
        let request = request_for(&self.common.app.settings().shell, options);
        if request == GpuRequest::Cpu {
            log_info!(LogCategory::Rendering, "{}", decide(GpuRequest::Cpu, None).render_line());
            return;
        }
        let plan = FailPlan::parse(azul_core::run_mode::test_hooks().gpu_fail.as_deref());
        for note in plan.notes() {
            log_info!(LogCategory::Rendering, "[render] {}", note);
        }
        // WebRender's backend thread wakes the loop when a frame is built: the loop presents then.
        let bridge = Arc::clone(&self.bridge);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(move || bridge.wake());
        self.gpu = Some(GlesGpu::new(
            GlesGpuConfig::android(),
            request,
            plan,
            options.clone(),
            None,
            Some(wake),
        ));
        self.backend = RenderBackend::Gpu;
    }

    /// Where the compiled programs are kept between runs: the app's `cache` directory beside its private files directory (the system
    /// empties it when storage runs low), else a directory of its own in `files`. Called by `android_main` with
    /// `AndroidApp::internal_data_path()`.
    pub(super) fn set_shader_cache_dir(&mut self, internal_data_path: Option<PathBuf>) {
        let Some(files) = internal_data_path else {
            return;
        };
        let dir = crate::desktop::shader_cache::android_cache_candidates(&files)
            .into_iter()
            .find(|candidate| {
                // The first whose parent exists (`cache` always does); the other is the fallback.
                candidate
                    .ancestors()
                    .nth(2)
                    .is_some_and(|app_dir| std::fs::metadata(app_dir).is_ok_and(|m| m.is_dir()))
            });
        if let (Some(gpu), Some(dir)) = (self.gpu.as_mut(), dir) {
            gpu.set_shader_cache_dir(Some(dir));
        }
    }

    /// `InitWindow`, after `native_window` was set: the GPU path takes the new surface, BEFORE any CPU frame is drawn on it.
    #[cfg(feature = "ndk")]
    pub(super) fn gpu_surface_created(&mut self) {
        let Some(surface) = self.gpu_surface() else {
            return;
        };
        let outcome = self.with_gpu(|gpu, ctx| gpu.surface_created(ctx, surface));
        self.after_gpu(outcome);
        // A new surface wants its first frame.
        self.gpu_present_owed = true;
    }

    /// `TerminateWindow`, BEFORE `native_window` is dropped: the EGL surface must be gone first.
    pub(super) fn gpu_surface_destroyed(&mut self) {
        let outcome = self.with_gpu(GlesGpu::surface_destroyed);
        self.after_gpu(outcome);
    }

    /// A new size or density of the surface the window has.
    #[cfg(feature = "ndk")]
    pub(super) fn gpu_resized(&mut self) {
        let Some(surface) = self.gpu_surface() else {
            return;
        };
        let outcome = self.with_gpu(|gpu, ctx| gpu.resized(ctx, surface));
        self.after_gpu(outcome);
        self.gpu_present_owed = true;
    }

    /// `onTrimMemory(level)` / `onLowMemory`: the renderer's caches, and with no window on screen the renderer and the context.
    pub(super) fn gpu_trim(&mut self, level: i32) {
        if let Some(level) = trim_level_for_android(level) {
            let outcome = self.with_gpu(|gpu, ctx| gpu.trim(ctx, level));
            self.after_gpu(outcome);
        }
    }

    /// Where `render_frame` posts a CPU frame: the display lists of the layout go to WebRender, as every desktop GL shell sends them. A
    /// full frame after a layout (or a display list that changed without one, or a view that was re-invoked), else the lightweight
    /// transaction (scroll offsets, property values, images) which asks for a frame only when something changed.
    pub(super) fn gpu_submit(&mut self, regenerated: bool) {
        if self.gpu.is_none() {
            return;
        }
        let owes_full = regenerated
            || self.common.display_list_dirty
            || self
                .common
                .layout_window
                .as_ref()
                .is_some_and(|lw| !lw.pending_virtual_view_updates.is_empty());
        if owes_full {
            self.common.display_list_dirty = false;
            gpu_renderer::submit_full_frame(&mut self.common);
            return;
        }
        // Advance easing-based scroll animations, as the desktop GL shells do before a lightweight frame.
        if let Some(lw) = self.common.layout_window.as_mut() {
            let now = azul_core::task::Instant::System(std::time::Instant::now().into());
            if lw.scroll_manager.tick(now).needs_repaint {
                lw.scroll_manager.calculate_scrollbar_states();
            }
        }
        let _ = gpu_renderer::submit_lightweight(&mut self.common, "Android");
    }

    /// Every loop iteration: when WebRender has built a frame, or the system asked for the window's content again, render the
    /// frame and swap it. Nothing on the CPU path (its present is `render_frame`).
    pub(super) fn gpu_present(&mut self) {
        if self.backend != RenderBackend::Gpu || self.gpu.is_none() {
            return;
        }
        let ready = self.gpu.as_ref().is_some_and(GlesGpu::take_frame_ready);
        // The content again (a new surface, `RedrawNeeded`) is presented once the renderer has a frame to show: before that
        // WebRender draws nothing and the swap would show an empty buffer.
        let owed = self.gpu_present_owed && self.gpu.as_ref().is_some_and(GlesGpu::has_frame);
        if ready || owed {
            self.gpu_present_owed = false;
            let outcome = self.with_gpu(GlesGpu::present);
            self.after_gpu(outcome);
        }
    }

    /// A CPU frame was posted: noted with the engine's frame statistics, as the GPU path notes its own (`GlesGpu` does it at its swap).
    /// The draw time is the raster since the last post plus the present that started at `present_started` (the lock, the copy and
    /// the post).
    #[cfg(all(feature = "ndk", feature = "cpurender"))]
    pub(super) fn note_posted_frame(&mut self, present_started: std::time::Instant) {
        let present_us = u64::try_from(present_started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let draw_us = std::mem::take(&mut self.raster_since_post_us).saturating_add(present_us);
        let at_us = u64::try_from(self.frame_clock.elapsed().as_micros()).unwrap_or(u64::MAX);
        if let Some(line) = self.frame_stats.record(at_us, Some(draw_us)) {
            log_info!(LogCategory::Rendering, "{}", line);
        }
    }

    /// `close()`: the GPU path goes (the surface, the renderer, the context, in that order) before the common state's renderer is
    /// asked to deinit.
    pub(super) fn gpu_shutdown(&mut self) {
        let outcome = self.with_gpu(GlesGpu::shutdown);
        self.after_gpu(outcome);
        self.gpu = None;
    }

    /// The surface the window has, in device pixels, and the engine's factor.
    #[cfg(feature = "ndk")]
    fn gpu_surface(&self) -> Option<Surface> {
        let window = self.native_window.as_ref()?;
        let scale = self.common.current_window_state().size.dpi as f32 / 96.0;
        Some(Surface::new(
            u32::try_from(window.width()).unwrap_or(0),
            u32::try_from(window.height()).unwrap_or(0),
            scale,
        ))
    }

    /// The `ANativeWindow*` of the surface the window has.
    fn native_window_ptr(&self) -> Option<*mut c_void> {
        #[cfg(feature = "ndk")]
        {
            self.native_window
                .as_ref()
                .map(|window| window.ptr().as_ptr().cast::<c_void>())
        }
        #[cfg(not(feature = "ndk"))]
        {
            None
        }
    }

    /// Runs `call` with the executor and the window's common state; `None` when there is no GPU path.
    fn with_gpu(
        &mut self,
        call: impl FnOnce(&mut GlesGpu, &mut GpuCtx<'_>) -> Outcome,
    ) -> Option<Outcome> {
        let mut gpu = self.gpu.take()?;
        let native_window = self.native_window_ptr();
        let outcome = {
            let mut ctx = GpuCtx {
                common: &mut self.common,
                native_window,
            };
            call(&mut gpu, &mut ctx)
        };
        self.gpu = Some(gpu);
        Some(outcome)
    }

    /// What a GPU call decided: when the path ended, the window is a CPU window from here on, and the CPU path takes the surface as a
    /// new one (a whole frame) with a layout pass that paints it.
    fn after_gpu(&mut self, outcome: Option<Outcome>) {
        if let Some(Outcome::FellBack(_)) = outcome {
            self.backend = RenderBackend::Cpu;
            self.gpu = None;
            self.gpu_present_owed = false;
            self.cpu_backend.force_full_repaint = true;
            self.present.surface_changed();
            self.common
                .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        }
    }
}
