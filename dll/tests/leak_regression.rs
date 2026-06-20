//! Regression test for the `regenerate_layout` resize-loop memory leak
//! (rust-fontconfig v4.2 `request_fonts` / build_queue accumulation).
//!
//! The failure mode this guards against: during a long interactive session
//! the user resizes the window many times; each resize triggers
//! `regenerate_layout` which calls into `FcFontRegistry::request_fonts`.
//! A pre-fix version of rust-fontconfig would push one `FcBuildJob` per
//! unmatched family into `build_queue` on every call, even after the
//! builder pool had shut down (`build_complete == true`). Those jobs were
//! never drained — leaking ~13 KiB per call, ~100 MiB across a ~5-second
//! resize loop.
//!
//! This test reproduces the trigger (headless window, stress DOM, many
//! resize-driven `regenerate_layout` calls) and asserts that the heap
//! stays bounded. With the fix in place the heap oscillates inside a
//! few MiB band; without the fix it grows ~13 KiB/call.
//!
//! Gated behind both `build-dll` (to pull in the full layout pipeline)
//! and `e2e-test` (to expose `HeadlessWindow` and its deps), and only
//! on macOS where `mstats().bytes_used` gives an accurate libc heap
//! reading. Other platforms can fall back to RSS but we keep the test
//! strict on the platform where the leak was first observed.

#![cfg(all(
    test,
    feature = "build-dll",
    feature = "e2e-test",
    target_os = "macos"
))]

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::headless::HeadlessWindow;
use azul_core::{
    callbacks::{LayoutCallback, LayoutCallbackInfo},
    dom::Dom,
    geom::LogicalSize,
    icon::SharedIconProvider,
    refany::RefAny,
    resources::AppConfig,
};
use azul_layout::window_state::WindowCreateOptions;
use rust_fontconfig::{registry::FcFontRegistry, FcFontCache};

/// Generate ~500 divs so the DOM is non-trivial — the bug is in the
/// font registry path which fires regardless of DOM size, but a fat DOM
/// exercises every branch of `regenerate_layout` (StyledDom rebuild,
/// cache migration, runtime-state copy, layout + display-list build)
/// so a regression elsewhere in the pipeline would also surface here.
const STRESS_DOM_CHILDREN: usize = 500;

/// Iterations of the resize loop. Enough that a ~13 KiB/call leak
/// (the pre-fix rate) would produce >6 MiB of growth — far above any
/// plausible noise floor from transient allocations.
const STRESS_ITERATIONS: u32 = 500;

/// Warmup iterations before the baseline heap sample. Amortizes
/// first-layout costs (chain cache population, glyph cache warmup,
/// lazy initialisation) so the baseline reflects steady-state
/// behaviour rather than startup.
const WARMUP_ITERATIONS: u32 = 10;

/// Per-iteration heap-growth budget. A pre-fix build leaked
/// ~13,800 bytes/call (median 15,392, derived from phase-probe
/// analysis). We allow up to 4 KiB/iter — headroom for macOS libc
/// malloc's own fragmentation / buddy-allocator overshoot, which
/// routinely shows ~2 KiB/iter steady noise even when no Rust
/// allocation is actually retained. Any real regression of the
/// original bug will blow well past this (3×+ margin).
const MAX_BYTES_PER_ITER: u64 = 4096;

/// Absolute cap on the heap at the end of the run — the test must not
/// exit holding more than this many bytes regardless of per-iter rate.
///
/// This is the "did the user's 100+ MiB bug come back?" guard. The
/// pre-fix build reliably ended the 500-iter scenario around ~93 MiB
/// of mstats heap; the post-fix build finishes around ~20 MiB. 40 MiB
/// gives 2× headroom over the post-fix steady state and leaves a
/// clear gap before the pre-fix regression zone, so a real leak
/// returning to the old rate would trip both this check and the
/// per-iter rate check.
const MAX_FINAL_HEAP_BYTES: u64 = 40 * 1024 * 1024;

/// Layout callback that returns a body with [`STRESS_DOM_CHILDREN`]
/// child divs. `extern "C"` because `LayoutCallbackType` is a C-ABI
/// function pointer for cross-language compatibility.
extern "C" fn stress_layout_callback(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    let mut body = Dom::create_body();
    for _ in 0..STRESS_DOM_CHILDREN {
        body.add_child(Dom::create_div());
    }
    body
}

#[test]
fn regenerate_layout_does_not_leak_under_resize_stress() {
    // --- Construct the AppConfig + icon provider ---
    //
    // `AppConfig::create()` sets up logging, icon provider handle,
    // bundled fonts, routes, etc. We then hoist the icon provider out
    // into a `SharedIconProvider` exactly the way the real entry point
    // does in `run.rs` — this is the supported construction dance.
    let mut config = AppConfig::create();
    let icon_provider_handle = core::mem::take(&mut config.icon_provider);
    let shared_icon_provider = SharedIconProvider::from_handle(icon_provider_handle);

    // --- WindowCreateOptions with our stress layout callback ---
    let cb_ptr: azul_core::callbacks::LayoutCallbackType = stress_layout_callback;
    let cb: LayoutCallback = cb_ptr.into();
    let options = WindowCreateOptions::create(cb);

    // --- App data (unused — our callback ignores it) ---
    let app_data = Arc::new(RefCell::new(RefAny::new(())));

    // --- Font stack ---
    //
    // We start a full `FcFontRegistry` with the scout + builder threads
    // so that the build-complete transition fires during warmup. The
    // leak only manifests *after* the builder finishes; if we short-
    // circuit with an empty cache the regression wouldn't surface.
    let fc_cache = Arc::new(FcFontCache::default());
    let registry = FcFontRegistry::new();
    registry.spawn_scout_and_builders();
    let registry_opt = Some(registry);

    let mut window = HeadlessWindow::new(
        options,
        app_data,
        azul::desktop::shell2::common::event::SharedUndoManager::new(),
        config,
        shared_icon_provider,
        fc_cache,
        registry_opt,
    )
    .expect("HeadlessWindow::new() failed — cargo test harness cannot construct a window");

    // --- Warmup ---
    //
    // Several layout passes at the starting size. This lets:
    // - The font builder pool finish parsing the common families
    //   (FcFontRegistry transitions to build_complete == true), which
    //   is the state the fix is guarding.
    // - The glyph cache / StyledDom cache populate so subsequent
    //   iterations hit the "LayoutUnchanged" equivalence path (the
    //   path where the leak was observed).
    for _ in 0..WARMUP_ITERATIONS {
        window.regenerate_layout().expect("warmup regenerate_layout failed");
    }

    // Drain the probe thread-local event buffer: without a consumer
    // it grows unboundedly and would masquerade as a real leak in our
    // heap sampling. This mirrors what `e2e_test::run_e2e_scenario`
    // does between iterations.
    azul_layout::probe::Probe::drop_events();

    let baseline = azul_layout::probe::malloc_heap_bytes();

    // --- Stress loop ---
    //
    // Cycle through four sizes so every iteration actually changes
    // the window rect (prevents early-return on "same size" paths).
    const SIZES: &[(f32, f32)] = &[
        (280.0, 360.0),
        (600.0, 480.0),
        (400.0, 720.0),
        (280.0, 360.0),
    ];

    for i in 0..STRESS_ITERATIONS {
        let (w, h) = SIZES[(i as usize) % SIZES.len()];
        let dim = LogicalSize { width: w, height: h };

        // Push new size into both mirrors (layout window mirror +
        // current_window_state used by the scenario).
        if let Some(lw) = window.common.layout_window.as_mut() {
            lw.current_window_state.size.dimensions = dim;
        }
        window.common.current_window_state.size.dimensions = dim;

        // Force a full relayout so the callback fires and
        // `request_fonts` is called — this is the exact trigger the
        // fix guards against.
        window.common.frame_needs_regeneration = true;
        window
            .regenerate_layout()
            .expect("stress-loop regenerate_layout failed");

        // Drain per-iter so probe-buffer growth doesn't pollute
        // the heap measurement on the final sample.
        azul_layout::probe::Probe::drop_events();
    }

    let final_heap = azul_layout::probe::malloc_heap_bytes();
    let growth = final_heap.saturating_sub(baseline);
    let per_iter = growth / u64::from(STRESS_ITERATIONS);

    eprintln!(
        "[leak_regression] baseline={} KiB  final={} KiB  growth={} KiB  per_iter={} B",
        baseline / 1024,
        final_heap / 1024,
        growth / 1024,
        per_iter,
    );

    assert!(
        per_iter < MAX_BYTES_PER_ITER,
        "regenerate_layout resize loop leaked {} bytes/iter (>{} allowed): \
         baseline={} KiB, final={} KiB, total_growth={} KiB across {} iterations. \
         This is the rust-fontconfig build_queue-accumulation leak or an \
         equivalent regression.",
        per_iter,
        MAX_BYTES_PER_ITER,
        baseline / 1024,
        final_heap / 1024,
        growth / 1024,
        STRESS_ITERATIONS,
    );

    // Absolute final-heap guard: "does the run end holding >40 MiB of
    // libc heap?" This is the original user-visible failure mode
    // (~100 MiB reported by the reporter). A leak slow enough to slip
    // under the per-iter cap could still drift over 40 MiB across
    // 500 iters; this assertion catches that case directly.
    assert!(
        final_heap < MAX_FINAL_HEAP_BYTES,
        "regenerate_layout resize loop ended holding {} KiB of libc heap \
         (>{} KiB cap): baseline={} KiB, per_iter={} B across {} iterations. \
         The per-iter rate is under budget but the absolute heap crossed \
         the failure-mode threshold — likely a slower or non-linear leak.",
        final_heap / 1024,
        MAX_FINAL_HEAP_BYTES / 1024,
        baseline / 1024,
        per_iter,
        STRESS_ITERATIONS,
    );
}
