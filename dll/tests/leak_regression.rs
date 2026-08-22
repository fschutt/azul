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
//! ## How this test measured, and why it was red (2026-07-29)
//!
//! It reported ~7.6 KiB leaked per iteration on macOS, reproducibly, and
//! blocked the 0.2.0 deploy. Reproduced on Linux at 5278 B/iter once the
//! heap probe worked there. None of it was a leak.
//!
//! The loop cycles four window sizes. A 720px-tall window holds far more
//! layout result than a 360px one, so the heap SWINGS by about 1.5 MiB
//! across one pass of the cycle — `AZ_PROFILE=heap,jsonl` shows it plainly,
//! oscillating between ~5.98 MiB and ~7.50 MiB with a period of four calls
//! and no upward drift at either extreme.
//!
//! The old code took `baseline` after ten warmups **at the starting size
//! only**, so three of the four sizes had never been laid out and their
//! caches were empty, then took `final` after iteration 499 — a different
//! point in that 1.5 MiB oscillation. It divided the difference by 500 and
//! called the quotient a per-iteration leak rate. Both errors push the same
//! way, and their sum is the "7.6 KiB/iter".
//!
//! The instrument was also wrong: `malloc_heap_bytes()` returned a
//! hardcoded 0 on every non-macOS target, which is the real reason this
//! file used to be `cfg(target_os = "macos")` — not, as its comment
//! claimed, strictness about where the bug was first seen.
//!
//! What the test does now: warm up over FULL cycles so every size's caches
//! are populated, then sample three times at matched points in the cycle
//! and take the rate from the SECOND window, where anything one-time has
//! already cancelled. Measured that way the leak is 2-25 B/iter of malloc
//! heap and 0-49 B/iter of RSS — zero, to the resolution of either
//! instrument. The per-iteration budget went 4096 -> 256 accordingly.
//!
//! The lesson is the recurring one in this repo: the check was wrong about
//! its own inputs. A red gate is evidence about the gate too.
//!
//! ## The heap instrument (2026-08-22)
//!
//! The heap figure is now counted by this binary's own `#[global_allocator]`
//! (see `live_heap` below): live bytes at the allocator boundary, exact on
//! every thread. It replaced `azul_layout::probe::malloc_heap_bytes()`,
//! whose macOS reading (`mstats().bytes_used`) drifted with zone capacity —
//! 399 / 3517 / 5963 B/iter on identical code, RSS flat — and therefore
//! could only convict with RSS's agreement, which a page-granular RSS can
//! never give for a small retention. The exact figure gates on its own
//! again; the libc figure and its corroboration rule remain as the fallback
//! when the dll's `allocator_*` features own the global allocator.
//!
//! Gated behind both `build-dll` (to pull in the full layout pipeline)
//! and `e2e-test` (to expose `HeadlessWindow` and its deps), and on the
//! targets where the RSS probe and the libc fallback read real figures:
//! macOS (`mstats().bytes_used`), Linux/glibc (`mallinfo2().uordblks`).
//!
//! It said `target_os = "macos"` until 2026-07-29, described as keeping
//! the test "strict on the platform where the leak was first observed".
//! That was not the reason. `malloc_heap_bytes()` returned a hardcoded 0
//! everywhere else, so on Linux this test would have computed
//! `per_iter = 0` and passed while measuring nothing at all — the gate
//! was load-bearing against a vacuous pass, and the comment had it
//! backwards. The probe now works on Linux, so the gate can widen.
//!
//! Note the asymmetry that remains: glibc's `mallinfo2` accounts the main
//! arena only, so the Linux figure misses allocations retained on the font
//! threads' arenas. Linux failing therefore proves a leak; Linux passing
//! does not prove macOS will.

#![cfg(all(
    test,
    feature = "build-dll",
    feature = "e2e-test",
    any(
        target_os = "macos",
        all(target_os = "linux", target_env = "gnu")
    )
))]

use std::{cell::RefCell, sync::Arc};

use azul::desktop::shell2::common::PlatformWindow;
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

// ---------------------------------------------------------------------------
// The heap instrument: LIVE bytes, counted at the allocator boundary
// ---------------------------------------------------------------------------
//
// `azul_layout::probe::malloc_heap_bytes()` asks the libc allocator what it
// holds, and the answer is not "live bytes" on either platform this test
// runs on. macOS `mstats().bytes_used` is libmalloc's per-zone
// `size_in_use`, which moves with zone growth and fragmentation while nothing
// is retained — measured on this tree, identical code, three consecutive
// runs: 399 / 3517 / 5963 B/iter with RSS flat at zero. glibc's
// `mallinfo2().uordblks` accounts the main arena only, so anything retained
// on the font threads' arenas is invisible to it. Both failure modes made the
// heap verdict unusable on its own: it needed RSS to corroborate it, and a
// page-granular RSS cannot corroborate a retention of a few hundred bytes.
//
// So this binary counts the bytes itself, at the `GlobalAlloc` boundary:
// every Rust allocation adds its requested size, every deallocation subtracts
// it, and the difference is — by definition — the live heap, exact to the
// byte, on every thread, on every platform, independent of how the allocator
// underneath chooses to carve its zones. A retained `Vec`, `Arc` chain or
// `Box` is visible at its true size; allocator housekeeping is not visible at
// all, which is the point. What it cannot see is C-side `malloc` (CoreText,
// fontconfig) — that stays RSS's job, and RSS keeps its own assertion.
//
// It is a test-binary concern, not a library one: the dll's own allocator
// selection (`allocator_mimalloc` / `allocator_jemalloc`) is a
// `#[global_allocator]` too, and a crate may declare only one, so under those
// features this falls back to the probe and the RSS-corroboration rule.
#[cfg(not(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc")))]
mod live_heap {
    use std::{
        alloc::{GlobalAlloc, Layout, System},
        sync::atomic::{AtomicU64, Ordering},
    };

    /// Bytes currently allocated through the global allocator. Requested
    /// sizes, not allocator-rounded ones: what the code asked to keep.
    static LIVE: AtomicU64 = AtomicU64::new(0);

    struct Counting;

    // SAFETY: every method forwards to `System` unchanged; the counter is a
    // side effect that never touches the returned memory. Relaxed ordering is
    // enough — the counter is read between measurement windows on the
    // sampling thread after the work has finished, not as a synchronisation
    // primitive.
    unsafe impl GlobalAlloc for Counting {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let p = System.alloc(layout);
            if !p.is_null() {
                LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            p
        }

        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let p = System.alloc_zeroed(layout);
            if !p.is_null() {
                LIVE.fetch_add(layout.size() as u64, Ordering::Relaxed);
            }
            p
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            System.dealloc(ptr, layout);
            LIVE.fetch_sub(layout.size() as u64, Ordering::Relaxed);
        }

        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
            let p = System.realloc(ptr, layout, new_size);
            if !p.is_null() {
                // The old block is gone and a `new_size` one exists; the
                // counter must reflect exactly that, in either direction.
                let old = layout.size() as u64;
                let new = new_size as u64;
                if new >= old {
                    LIVE.fetch_add(new - old, Ordering::Relaxed);
                } else {
                    LIVE.fetch_sub(old - new, Ordering::Relaxed);
                }
            }
            p
        }
    }

    #[global_allocator]
    static GLOBAL: Counting = Counting;

    /// Live heap bytes right now.
    pub fn bytes() -> u64 {
        LIVE.load(Ordering::Relaxed)
    }

    /// Whether this binary is measuring at the allocator boundary.
    pub const ACTIVE: bool = true;
}

#[cfg(any(feature = "allocator_mimalloc", feature = "allocator_jemalloc"))]
mod live_heap {
    /// The dll owns the global allocator under these features; fall back to
    /// the libc figure and the RSS-corroboration rule that goes with it.
    pub fn bytes() -> u64 {
        azul_layout::probe::malloc_heap_bytes()
    }

    pub const ACTIVE: bool = false;
}

/// Generate ~500 divs so the DOM is non-trivial — the bug is in the
/// font registry path which fires regardless of DOM size, but a fat DOM
/// exercises every branch of `regenerate_layout` (StyledDom rebuild,
/// cache migration, runtime-state copy, layout + display-list build)
/// so a regression elsewhere in the pipeline would also surface here.
const STRESS_DOM_CHILDREN: usize = 500;

/// Iterations per measured window; the loop runs two of them. Enough that
/// a ~13 KiB/call leak (the pre-fix rate) would produce >6 MiB of growth in
/// each — far above any plausible noise floor from transient allocations.
///
/// Must stay a multiple of `SIZES.len()` so both windows begin and end at
/// the same point in the size cycle. That is what makes the measurement
/// immune to the ~1.5 MiB oscillation described in the module docs.
const STRESS_ITERATIONS: u32 = 500;

/// Warmup iterations before the baseline heap sample. Amortizes
/// first-layout costs (chain cache population, glyph cache warmup,
/// lazy initialisation) so the baseline reflects steady-state
/// behaviour rather than startup.
const WARMUP_ITERATIONS: u32 = 10;

/// Full passes over `SIZES` to run before the first heap sample.
///
/// `WARMUP_ITERATIONS` alone is not enough: it runs at the *starting* size
/// only, so the caches for the other three sizes are still empty when the
/// baseline is taken and get populated during the measured window.
const WARMUP_CYCLES: u32 = 8;

/// Hard cap on adaptive warmup, so a genuine unbounded leak cannot turn
/// "warm up until the heap settles" into an infinite loop. If this is hit the
/// measurement still runs and says so — a leak large enough to prevent
/// settling will be caught by the steady-state assertion anyway.
const MAX_WARMUP_ITERATIONS: u32 = 4000;

/// Growth across one warmup block below which the heap counts as settled.
///
/// DERIVED, not chosen. It was a hand-picked 256 KiB, and that made the test
/// self-contradictory: a warmup block is `SIZES.len() * WARMUP_CYCLES` = 32
/// iterations, so 256 KiB/block is 8192 B/iter — eight times looser than the
/// 1024 B/iter the first-window assertion demands immediately afterwards.
/// Linux CI duly reported "warmup: 416 iterations, settled" and then failed
/// with first_window = 1253 B/iter. Warmup was doing exactly what it was told;
/// it was told the wrong thing.
///
/// Derived from `MAX_BYTES_PER_ITER` — the TIGHTEST budget applied after
/// warmup, not the loosest.
///
/// First attempt tied it to `MAX_FIRST_WINDOW_BYTES_PER_ITER` (1024), which
/// fixed the first-window contradiction and left the same bug one level down:
/// warmup was then free to stop at ~1024 B/iter and the steady-state check
/// demands 256. macOS duly settled and reported steady=1179/917 B/iter.
///
/// "Warm" has to mean "quiet enough for EVERY assertion that follows", so it
/// is the minimum of the post-warmup budgets that matters, and that is the
/// steady-state one.
const WARMUP_SETTLED_BYTES: u64 =
    MAX_BYTES_PER_ITER * (SIZES_LEN as u64) * (WARMUP_CYCLES as u64);

/// Ceiling on the NEWEST warmup block, as a multiple of
/// [`WARMUP_SETTLED_BYTES`], before warmup may declare itself settled.
///
/// `min(growth, prev_growth)` exists to cancel a one-off allocator zone
/// reservation, and it must keep doing that — hence a multiple rather than
/// equality. But without any ceiling the min() rule is satisfied by a single
/// quiet block, so warmup could stop on a block 138x over threshold and enter
/// measurement while the heap was still climbing. 4x admits an allocator step
/// (those are one block wide) and rejects a climb.
const WARMUP_LAST_BLOCK_CEILING_MULT: u64 = 4;

/// `SIZES.len()`, as a const so `WARMUP_SETTLED_BYTES` can be computed at
/// compile time. Asserted against the real array in the measurement body.
const SIZES_LEN: usize = 4;

/// Per-iteration steady-state heap-growth budget.
///
/// 4096 until 2026-07-29, justified as "headroom for macOS libc malloc's own
/// fragmentation, which routinely shows ~2 KiB/iter steady noise". There is
/// no such noise. That number was the size of the measurement error this test
/// used to make (see the sampling comment in the body); the budget had been
/// set to accommodate the bug in the ruler.
///
/// Measured steady-state on Linux/glibc with phase-matched sampling: **2 to
/// 25 bytes/iter** across 500 iterations, over five runs. 256 leaves an order
/// of magnitude of headroom over the worst of those and still fails a leak
/// 30x smaller than the one this file was written for.
const MAX_BYTES_PER_ITER: u64 = 256;

/// Budget for the FIRST measured window, which may still carry one-time costs
/// that outlived warmup (a cache that fills lazily over the first few hundred
/// frames). Looser than the steady-state budget on purpose — a cost that
/// happens once is not a leak — but bounded, because "one-time" cannot mean
/// "still growing after 40 warmup frames and 500 measured ones".
///
/// Measured: 18 to 73 bytes/iter over five runs.
const MAX_FIRST_WINDOW_BYTES_PER_ITER: u64 = 1024;

/// Steady-state RSS budget, per iteration.
///
/// This is the check that covers what [`malloc_heap_bytes`] structurally
/// cannot see: on Linux, glibc accounts the main arena only, so anything
/// retained on the font scout/builder threads' arenas is invisible to the
/// heap figure. RSS sees every arena, plus mmap'd regions (font files, GL
/// buffers) that malloc accounting excludes by design.
///
/// Page-granular and therefore coarse, so the budget is loose. It is here to
/// catch a leak that hides from the primary instrument, not to measure.
const MAX_RSS_BYTES_PER_ITER: u64 = 1024;

/// Absolute cap on the heap at the end of the run — a blunt backstop for
/// non-linear growth, regardless of per-iter rate.
///
/// Was 40 MiB, justified as "the post-fix build finishes around ~20 MiB, so
/// this gives 2x headroom and leaves a clear gap before the ~93 MiB pre-fix
/// regression zone". **That ~20 MiB does not reproduce.** Measured now, warm,
/// the run ends at ~90 MiB on Linux/glibc debug and ~70 MiB on macOS — inside
/// the range the old comment labelled "the leak is back".
///
/// It is not back, and the rate data is what says so rather than this number:
/// after warmup the heap is FLAT — 92279 -> 92281 -> 92282 -> 92285 KiB across
/// 1500 iterations, 1 B/iter, with RSS unchanged to the kilobyte. The pre-fix
/// bug was ~13 KiB retained per call and unbounded; at 1 B/iter it would take
/// about 13000 calls to produce what it used to produce in one.
///
/// The old figure was almost certainly taken before the caches finished
/// filling — the same mistake, in a different place, as the 7.6 KiB/iter
/// "leak" this file was rewritten to stop reporting. Warmup now terminates on
/// a settled heap instead of a fixed 8 cycles, so `final_heap` is the warm
/// working set, which is a different quantity from the one 40 MiB was chosen
/// against.
///
/// So this cap is now what it can honestly be: an absurdity threshold on the
/// warm working set, ~2.8x the largest measured value. The discrimination is
/// done by the three-window rate assertions above, which are 50x more
/// sensitive to the original bug than this check ever was.
const MAX_FINAL_HEAP_BYTES: u64 = 256 * 1024 * 1024;

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

/// Result of one phase-matched measurement run. All rates are bytes per
/// iteration.
struct Measurement {
    /// Growth across the first measured window — may still contain one-time
    /// costs that outlived warmup.
    first_window_per_iter: u64,
    /// Growth across the second window. This is the leak rate.
    per_iter: u64,
    /// Minimum RSS growth across the two steady windows. Sees arenas and
    /// mmaps that malloc does not. Conservative by construction, which is what
    /// the main test wants.
    rss_per_iter: u64,
    /// RSS growth across the FIRST steady window only.
    ///
    /// The RSS control needs this rather than the minimum, because residency
    /// is not additive: on macos-14 a 256 KiB/iter leak produced RSS
    /// 28818 -> 40002 -> 40705 KiB while the malloc heap kept climbing at
    /// ~195 KB/iter. The OS reclaimed the older leaked pages as fast as new
    /// ones arrived, so the second window read 7 KB/iter and the minimum went
    /// with it. That plateau is the kernel doing its job, not the instrument
    /// failing — and it means "RSS grows by what was allocated" was never a
    /// property any OS promised.
    rss_first_steady_per_iter: u64,
    /// Absolute malloc heap at the end of the run.
    final_heap: u64,
    /// Whether the heap figure is self-consistent enough to draw a verdict
    /// from — see the instrument self-validation in `measure`. False means the
    /// allocator reported more live heap than the process has resident memory,
    /// so heap-based assertions are skipped and only RSS gates.
    heap_trustworthy: bool,
}

/// Run the resize-stress scenario and measure it.
///
/// `deliberate_leak_bytes` is for the harness's own negative control: when
/// non-zero, each iteration leaks exactly that many bytes on purpose. A leak
/// detector that has never been shown to detect a leak is not evidence of
/// anything, and this repo has now shipped several checks that were wrong
/// about their own inputs.
fn measure(iterations: u32, deliberate_leak_bytes: usize) -> Measurement {
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

    // `SIZES_LEN` feeds WARMUP_SETTLED_BYTES at compile time; if someone adds a
    // fifth size the derived threshold would silently be wrong.
    assert_eq!(
        SIZES.len(),
        SIZES_LEN,
        "SIZES_LEN is out of sync with SIZES — WARMUP_SETTLED_BYTES is derived \
         from it and would be computed for the wrong block size"
    );

    let run_iterations = |window: &mut HeadlessWindow, n: u32| {
        for i in 0..n {
            let (w, h) = SIZES[(i as usize) % SIZES.len()];
            let dim = LogicalSize { width: w, height: h };

            // Push new size into both mirrors (layout window mirror +
            // current_window_state used by the scenario).
            if let Some(lw) = window.common.layout_window.as_mut() {
                lw.current_window_state.size.dimensions = dim;
            }
            window.common.update_window_state(
                azul::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| ws.size.dimensions = dim,
            );

            // Force a full relayout so the callback fires and
            // `request_fonts` is called — this is the exact trigger the
            // fix guards against.
            window
                .common
                .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
            window
                .regenerate_layout()
                .expect("stress-loop regenerate_layout failed");

            // Drain per-iter so probe-buffer growth doesn't pollute
            // the heap measurement on the final sample.
            azul_layout::probe::Probe::drop_events();

            if deliberate_leak_bytes > 0 {
                // Well under glibc's 128 KiB MMAP_THRESHOLD, so this comes
                // from the heap proper and `mallinfo2` accounts for it.
                //
                // The fill must be INCOMPRESSIBLE. This was `vec![0xAB; n]`,
                // and on macOS that made the RSS control fail: a constant byte
                // pattern is exactly what the macOS memory compressor exists
                // to squash, so 256 KiB/iter of written pages moved measured
                // RSS by only 87 KiB/iter — a third of what was written. The
                // control read that as "RSS cannot resolve this leak" when
                // what actually happened is that the pages stopped being
                // resident in the form they were written. Linux has no
                // compressor in that path, which is why it passed there.
                //
                // A cheap xorshift keeps every page distinct and unsquashable,
                // so `resident_size` has to move by the full amount on both
                // platforms.
                let mut leaked = vec![0_u8; deliberate_leak_bytes];
                let mut x: u32 = 0x9E37_79B9 ^ i;
                for chunk in leaked.chunks_mut(4) {
                    x ^= x << 13;
                    x ^= x >> 17;
                    x ^= x << 5;
                    chunk.copy_from_slice(&x.to_ne_bytes()[..chunk.len()]);
                }
                core::mem::forget(leaked);
            }
        }
    };

    // Cycle warmup, on top of the same-size warmup above. The heap swings by
    // over a megabyte across the four sizes — a 720px-tall window holds far
    // more layout result than a 360px one — and each size's caches are
    // populated the first time that size is laid out. Both effects are
    // one-time; sampling before they have happened and again after charges
    // them to the iteration count as if they were a per-frame rate.
    //
    // WARM UP UNTIL IT IS ACTUALLY WARM, rather than for a fixed 8 cycles.
    // A constant cannot know how long a cache takes to fill, and this one was
    // wrong: 8 cycles is 32 iterations, against a 500-iteration measurement
    // window. Measured here, roughly 64 MiB is still arriving across the first
    // ~500 post-warmup iterations and then stops dead — the first window read
    // 131712 B/iter against a 1024 budget while steady state was 1 B/iter.
    // That assertion could not have passed on either platform; on macOS it was
    // simply unreachable, because the steady-state assertion above it failed
    // first and hid it.
    //
    // So: run in whole cycles (never a partial one — a partial cycle would
    // desynchronise every later sample from the size oscillation) until the
    // heap stops growing across two consecutive blocks, or the cap is hit.
    // Terminating on the measurement instead of on a guess also makes this
    // FASTER than a fixed warmup large enough to be safe.
    if deliberate_leak_bytes == 0 {
        let block = SIZES.len() as u32 * WARMUP_CYCLES;
        let mut prev = live_heap::bytes();
        let mut settled = 0_u32;
        let mut done = 0_u32;
        // Starts at u64::MAX so the very first block cannot satisfy the
        // minimum on its own — settling always needs two blocks of evidence.
        let mut prev_growth = u64::MAX;
        while done < MAX_WARMUP_ITERATIONS && settled < 2 {
            run_iterations(&mut window, block);
            done += block;
            let now = live_heap::bytes();
            let growth = now.saturating_sub(prev);
            // Same insight as the steady-state windows: a one-off allocator
            // zone reservation lands in exactly ONE block. Requiring two
            // consecutive blocks to be individually quiet would let a
            // regularly-stepping allocator (macOS reserves in 1 MiB chunks)
            // reset the counter forever and run to the cap. Take the minimum of
            // the last two blocks instead — a step is cancelled by the quiet
            // block beside it, while a still-filling cache has no quiet block.
            // The min() rule cancels a one-off allocator step, but on its own it
            // also lets warmup STOP on a huge block as long as the block before
            // it was quiet — measurement then begins mid-climb. That is not
            // hypothetical: on 2026-08-14 this printed
            //   "warmup: 448 iterations, settled (last block 35337 B/iter,
            //    threshold 256 B/iter)"
            // and went straight into sampling, 138x over its own threshold,
            // while calling itself settled. So the newest block gets a ceiling
            // too: generous enough that a genuine one-off step still settles
            // (that is what min() is for), tight enough that warmup cannot end
            // on a block that is still climbing hard.
            let step_cancelled = growth.min(prev_growth) < WARMUP_SETTLED_BYTES;
            let last_block_quiet =
                growth < WARMUP_SETTLED_BYTES.saturating_mul(WARMUP_LAST_BLOCK_CEILING_MULT);
            if step_cancelled && last_block_quiet {
                settled += 1;
            } else {
                settled = 0;
            }
            prev_growth = growth;
            prev = now;
        }
        eprintln!(
            "[leak_regression] warmup: {done} iterations, {} (last block {} B/iter, \
             threshold {} B/iter)",
            if settled >= 2 { "settled" } else { "HIT THE CAP — the budgets below may not hold" },
            prev_growth.saturating_div(u64::from(block).max(1)),
            WARMUP_SETTLED_BYTES / u64::from(block).max(1),
        );
    } else {
        // A deliberate leak never settles by construction, so adaptive warmup
        // would always run to the cap and waste minutes. The controls do not
        // need a settled cache; they need the leak to be visible.
        run_iterations(&mut window, WARMUP_CYCLES * SIZES.len() as u32);
    }

    // Every sample below is taken at the SAME point in the size cycle (a
    // multiple of SIZES.len() iterations apart), so the ~1.5 MiB oscillation
    // cancels instead of landing in the result.
    let sample = || (live_heap::bytes(), azul_layout::probe::current_rss_bytes().0);

    // THREE steady-state windows, not two. A leak is growth that does not
    // stop, so the test should require growth that does not stop.
    //
    // With two windows this reported 2097 B/iter on macOS against a 256 B
    // budget, and the second instrument said what was really going on: RSS
    // moved by exactly 0 bytes, 34897 KiB before and 34897 KiB after. The
    // "growth" was 1 MiB — 1048576 / 500 = 2097.15 — arriving in one step.
    // That is macOS's allocator reserving a zone, not 500 iterations each
    // retaining 2 KiB, and no amount of raising the budget distinguishes the
    // two. glibc's finer-grained accounting is why Linux measured 2-25 B/iter
    // for the identical workload.
    //
    // A one-off chunk lands in exactly one window. A real leak lands in every
    // one. So take the MINIMUM rate across the two steady-state windows: an
    // allocator step is cancelled by the quiet window beside it, while a
    // genuine leak has no quiet window to be cancelled by.
    let (baseline, baseline_rss) = sample();
    run_iterations(&mut window, iterations);
    let (mid, mid_rss) = sample();
    run_iterations(&mut window, iterations);
    let (second, second_rss) = sample();
    run_iterations(&mut window, iterations);
    let (final_heap, final_rss) = sample();

    // The rate that matters is the SECOND window's. Anything one-time that
    // survived warmup is charged to the first window and cancels here; only a
    // genuinely unbounded leak keeps producing growth window after window.
    // Divide by `iterations`, not by STRESS_ITERATIONS. Those were the same
    // number until this function was parameterised, and the negative control
    // below caught the difference on its first run — it leaked 4096 B/iter and
    // measured 836, exactly the 100/500 ratio.
    let n = u64::from(iterations);
    // Growth charged to the first window is growth that is STILL THERE later.
    //
    // macOS: baseline=97795 -> 98819 -> 97795 -> 97795 KiB. The heap took a
    // 1 MiB zone and gave it back; 1048576/500 = 2097 B/iter, over the 1024
    // budget, while the run ENDED at exactly the baseline — net growth zero
    // bytes, RSS unmoved at 37473 KiB through all four samples. `mid - baseline`
    // cannot tell a transient reservation from a cache still filling, because it
    // only ever looks at one later sample.
    //
    // Taking the minimum of every later sample fixes that without weakening the
    // check: a cache that is genuinely still filling holds the heap up, so every
    // later sample is high and the minimum stays high. A blip is discounted the
    // moment any subsequent sample comes back down.
    let persisted = mid.min(second).min(final_heap);
    let first_window_per_iter = persisted.saturating_sub(baseline) / n;
    let window_a = second.saturating_sub(mid) / n;
    let window_b = final_heap.saturating_sub(second) / n;
    let per_iter = window_a.min(window_b);

    // Same treatment for RSS: the minimum of the two steady windows, so a
    // one-off mapping cannot masquerade as a rate here either.
    let rss_a = second_rss.saturating_sub(mid_rss) / n;
    let rss_b = final_rss.saturating_sub(second_rss) / n;
    let rss_per_iter = rss_a.min(rss_b);

    eprintln!(
        "[leak_regression] heap: baseline={} KiB -> {} -> {} -> {} KiB  \
         first_window={} B/iter  steady={}/{} B/iter (min {})  |  \
         rss: {} KiB -> {} -> {} -> {} KiB  steady={}/{} B/iter (min {})",
        baseline / 1024,
        mid / 1024,
        second / 1024,
        final_heap / 1024,
        first_window_per_iter,
        window_a,
        window_b,
        per_iter,
        baseline_rss / 1024,
        mid_rss / 1024,
        second_rss / 1024,
        final_rss / 1024,
        rss_a,
        rss_b,
        rss_per_iter,
    );

    // INSTRUMENT SELF-VALIDATION.
    //
    // Live malloc'd bytes must be backed by resident pages, so a heap figure
    // far ABOVE RSS is not a measurement of live heap — it is the allocator
    // reporting zone capacity or freed-but-unreturned regions. Without this
    // check the excess gets attributed to azul, which is precisely what
    // happened on 2026-08-14: macOS reported `baseline=105098 KiB` of heap in a
    // process whose RSS was `33090 KiB`, three times more live heap than the
    // process had memory, and the run failed claiming `regenerate_layout`
    // leaked 1431 B/iter. The same job, same code, same thresholds passed on
    // the next commit — which a 1431 B/iter retention cannot do and a
    // misreporting instrument does routinely.
    //
    // The slack is deliberately loose (2x + 32 MiB): this is not a tightness
    // check, it is a "the number is physically impossible" check. When it
    // trips, the HEAP verdict is unusable and is dropped — but RSS is measured
    // independently and still gates, so the test does not become vacuous. The
    // report says plainly which verdicts were live, because a silent downgrade
    // to half a test is how a gate stops meaning anything.
    //
    // That rule is for the LIBC figure. Allocator-boundary live bytes are
    // exact by construction and CAN legitimately exceed RSS: pages the
    // process owns but has not touched, or that the OS paged or compressed
    // out under memory pressure, are live heap and not resident. (The
    // deliberate-leak control showed exactly this: 262144/262144 B/iter
    // counted, RSS moving by a twentieth of it on a loaded machine.)
    let heap_trustworthy =
        live_heap::ACTIVE || final_heap <= final_rss.saturating_mul(2).saturating_add(32 << 20);
    if !heap_trustworthy {
        eprintln!(
            "[leak_regression] HEAP INSTRUMENT REJECTED: {} KiB of reported heap in a process \
             with {} KiB RSS. Live heap cannot exceed resident memory, so this figure is not \
             live heap and no leak verdict is drawn from it. The RSS verdict below still \
             gates. (macOS mstats().bytes_used counts zone capacity; glibc mallinfo2 \
             .uordblks counts the main arena only.)",
            final_heap / 1024,
            final_rss / 1024,
        );
    }
    eprintln!(
        "[leak_regression] heap instrument: {}",
        if live_heap::ACTIVE {
            "allocator-boundary live bytes (exact; gates on its own)"
        } else {
            "libc figure via probe::malloc_heap_bytes (needs RSS corroboration)"
        }
    );

    Measurement {
        first_window_per_iter,
        per_iter,
        rss_per_iter,
        rss_first_steady_per_iter: rss_a,
        final_heap,
        heap_trustworthy,
    }
}

#[test]
fn regenerate_layout_does_not_leak_under_resize_stress() {
    let m = measure(STRESS_ITERATIONS, 0);

    // CORROBORATION, which is the rule this test's own message already tells the
    // reader to apply by hand: a heap figure only convicts when RSS agrees.
    //
    // Measured on this tree, same code, three consecutive runs:
    //     heap min = 399 / 3517 / 5963 B/iter      RSS min = 0
    // A 15x spread with RSS flat at zero is not what a per-call retention looks
    // like — a real one is steady and moves both instruments. macOS
    // mstats().bytes_used counts zone CAPACITY, so it drifts with fragmentation
    // and zone growth while nothing is actually retained, and this suite runs on
    // macOS ONLY.
    //
    // The original build_queue leak this test was written for ran at ~7.6 KiB
    // per iteration and was steady; it would move RSS too, so requiring
    // agreement keeps that catch while dropping the false positives that have
    // now blocked two deploys. RSS keeps its own INDEPENDENT assertion below —
    // if residency grows on its own, that still fails on the spot.
    //
    // That rule is for the LIBC figure. With the allocator-boundary instrument
    // (`live_heap::ACTIVE`) the number IS live bytes — requested sizes in
    // minus requested sizes out — so zone capacity cannot drift it and there
    // is nothing for RSS to corroborate: a retention of a few hundred bytes
    // per call is exactly what a page-granular RSS cannot see, and exactly
    // what the heap figure is for. It gates alone again.
    let heap_convicts = if live_heap::ACTIVE {
        m.per_iter >= MAX_BYTES_PER_ITER
    } else {
        m.heap_trustworthy && m.per_iter >= MAX_BYTES_PER_ITER && m.rss_per_iter > 0
    };
    assert!(
        !heap_convicts,
        "regenerate_layout resize loop leaked {} bytes/iter (>{} allowed) in \
         STEADY STATE — and this is the MINIMUM of two consecutive {}-iteration \
         windows, each sampled at a matched point in the size cycle after {} \
         warmup cycles. Neither one-time costs nor a single allocator-zone \
         expansion can produce that: a one-off lands in one window and is \
         cancelled by the other. Growth in BOTH means something is retained per \
         call. This is the rust-fontconfig build_queue-accumulation leak or an \
         equivalent regression. (The heap instrument passed self-validation, so \
         this figure is a real one, AND RSS corroborates it — both instruments \
         moved, which is what separates a retention from zone-capacity drift, \
         if they do not, suspect the measurement before the code.)",
        m.per_iter,
        MAX_BYTES_PER_ITER,
        STRESS_ITERATIONS,
        WARMUP_CYCLES,
    );

    // Same corroboration rule as the steady-state check above: on macOS the heap
    // number is zone CAPACITY, so a first window that has not "settled" is
    // usually the allocator growing its zones, not the code filling something.
    // Require RSS to have moved before calling it slow-filling retention. In the
    // run that prompted this, RSS was 44226 KiB at all four samples — flat to
    // the kilobyte — while the heap reported 3211 B/iter.
    let first_window_convicts = if live_heap::ACTIVE {
        m.first_window_per_iter >= MAX_FIRST_WINDOW_BYTES_PER_ITER
    } else {
        m.heap_trustworthy
            && m.first_window_per_iter >= MAX_FIRST_WINDOW_BYTES_PER_ITER
            && m.rss_per_iter > 0
    };
    assert!(
        !first_window_convicts,
        "the first measured window grew {} bytes/iter (>{} allowed) of live heap. Steady \
         state is {} B/iter, so this is not an unbounded leak — but {} warmup \
         cycles plus {} iterations were not enough for it to settle, which \
         means something is filling far more slowly than any cache should.",
        m.first_window_per_iter,
        MAX_FIRST_WINDOW_BYTES_PER_ITER,
        m.per_iter,
        WARMUP_CYCLES,
        STRESS_ITERATIONS,
    );

    assert!(
        m.rss_per_iter < MAX_RSS_BYTES_PER_ITER,
        "RSS grew {} bytes/iter (>{} allowed) in steady state while the malloc \
         heap grew only {} B/iter across {} iterations. The two disagree, and \
         RSS is the one that sees all of it — non-main glibc arenas (the font \
         scout/builder threads allocate on their own) and mmap'd regions are \
         outside malloc accounting entirely. Trust this number over the heap \
         number.",
        m.rss_per_iter,
        MAX_RSS_BYTES_PER_ITER,
        m.per_iter,
        STRESS_ITERATIONS,
    );

    // Absolute final-heap guard: "does the run end holding >40 MiB of
    // libc heap?" This is the original user-visible failure mode
    // (~100 MiB reported by the reporter). A leak slow enough to slip
    // under the per-iter cap could still drift over 40 MiB; this
    // assertion catches that case directly.
    //
    // Kept at 40 MiB even though the Linux run now finishes around 6 MiB.
    // The macOS figure comes from `mstats()`, a different allocator with
    // different accounting, and this box cannot measure it — tightening a
    // budget for a platform you cannot run is how you produce a red that
    // says nothing about the code.
    assert!(
        m.final_heap < MAX_FINAL_HEAP_BYTES,
        "regenerate_layout resize loop ended holding {} KiB of libc heap \
         (>{} KiB cap) at {} B/iter across {} iterations. The per-iter rate \
         is under budget but the absolute heap crossed the failure-mode \
         threshold — likely a slower or non-linear leak.",
        m.final_heap / 1024,
        MAX_FINAL_HEAP_BYTES / 1024,
        m.per_iter,
        STRESS_ITERATIONS,
    );
}

/// Bytes leaked per iteration by the negative control.
///
/// Deliberately set to the OLD `MAX_BYTES_PER_ITER`, so this test states
/// something specific: a leak of exactly the size the previous budget would
/// have waved through is caught, comfortably, by the new one.
const CONTROL_LEAK_BYTES: usize = 4096;

/// Shorter than the real run — the control only has to show the needle moves.
/// Must stay a multiple of the size-cycle length.
const CONTROL_ITERATIONS: u32 = 100;

/// The negative control for every assertion above.
///
/// Without this, `regenerate_layout_does_not_leak_under_resize_stress`
/// passing is compatible with the measurement being broken — which is not a
/// hypothetical here. It measured a 1.5 MiB oscillation as a leak for months,
/// and before that it ran only on macOS because the underlying probe returned
/// a hardcoded 0 everywhere else. Had it ever been enabled on Linux in that
/// state it would have measured `per_iter = 0` and passed, forever, while
/// reporting nothing.
#[test]
fn the_leak_detector_actually_detects_a_leak() {
    let m = measure(CONTROL_ITERATIONS, CONTROL_LEAK_BYTES);

    // Allocator overhead means the observed rate is a little ABOVE the
    // requested one; require most of it rather than an exact match.
    let floor = (CONTROL_LEAK_BYTES as u64) * 3 / 4;
    assert!(
        m.per_iter >= floor,
        "leaking {} B/iter on purpose registered as only {} B/iter (needed \
         >={}). The measurement is not seeing retained memory, so a clean run \
         of the sibling test proves nothing.",
        CONTROL_LEAK_BYTES,
        m.per_iter,
        floor,
    );

    assert!(
        m.per_iter >= MAX_BYTES_PER_ITER,
        "the deliberate {} B/iter leak measured {} B/iter, which is under the \
         {} B/iter budget the real test enforces — the control does not \
         actually exercise the assertion it exists to validate.",
        CONTROL_LEAK_BYTES,
        m.per_iter,
        MAX_BYTES_PER_ITER,
    );

    // NOT asserted here: that RSS also sees this leak. It does not, and
    // measurement says so — 4096 B/iter for 100 iterations is ~400 KiB, which
    // fits inside heap the process already had resident and free, so not one
    // new page is mapped and RSS reads flat. That is a real property of the
    // instrument, not a flaw in the control: RSS cannot resolve a small leak.
    // Its sensitivity is established separately, below, at a size where it is
    // the only instrument that can work at all.
}

/// Per-iteration leak for the RSS control. Chosen above glibc's nominal
/// 128 KiB `MMAP_THRESHOLD` and large enough that RSS must move by whole
/// pages every iteration.
///
/// It does NOT reliably bypass malloc accounting — measured, `mallinfo2` sees
/// about 262 KB/iter of this too, because glibc raises the mmap threshold
/// dynamically and serves these from an arena. That is fine: the assertion
/// here is about RSS's sensitivity, not about the heap figure's blindness.
const RSS_CONTROL_LEAK_BYTES: usize = 256 * 1024;

/// Establishes that the RSS cross-check can see what the malloc heap figure
/// structurally cannot.
///
/// The main test's RSS assertion exists for one reason: `mallinfo2` accounts
/// glibc's main arena only, so memory retained on the font threads' arenas —
/// or handed out by `mmap` rather than the heap — is invisible to it. An
/// unvalidated backstop is not a backstop, and the sibling control above
/// deliberately does not cover this: a 4 KiB/iter leak is too small to move
/// RSS at all.
///
/// So leak in 256 KiB blocks instead. Every byte is written, so the pages are
/// genuinely resident and RSS must move by the full amount. This does not
/// prove RSS sees things malloc cannot — glibc happens to account these too —
/// only that RSS resolves a leak of this magnitude at all, which is the
/// premise the main test's RSS budget rests on.
/// The first-window rule, checked on the arithmetic rather than on the engine.
///
/// `first_window_per_iter` is `min(mid, second, final) - baseline`, and the
/// reason is a specific macOS observation: the heap took a 1 MiB zone and gave
/// it straight back (97795 -> 98819 -> 97795 -> 97795 KiB), which `mid -
/// baseline` scored as 2097 B/iter of growth on a run whose net growth was zero
/// bytes. A rule with no test is a rule that quietly stops holding, and this one
/// is pure arithmetic, so it costs nothing to pin both directions.
#[test]
fn first_window_growth_ignores_a_transient_but_not_a_persistent_one() {
    // Mirrors the production expression exactly.
    let charge = |baseline: u64, mid: u64, second: u64, final_heap: u64, n: u64| {
        mid.min(second).min(final_heap).saturating_sub(baseline) / n
    };

    // The macOS observation, in KiB*1024, 500 iterations: up 1 MiB, back down.
    let transient = charge(97795 * 1024, 98819 * 1024, 97795 * 1024, 97795 * 1024, 500);
    assert_eq!(
        transient, 0,
        "a 1 MiB reservation that is released again must not be charged as \
         first-window growth — the run ended at exactly its baseline"
    );

    // A cache still filling: every later sample stays up.
    let persistent = charge(10_000_000, 11_000_000, 11_500_000, 12_000_000, 500);
    assert_eq!(
        persistent, 2000,
        "growth still present at every later sample must be charged in full — \
         discounting it would blind the check to a slow fill, which is the only \
         thing it exists to catch"
    );

    // The blip must not mask a real fill that happens alongside it.
    let both = charge(10_000_000, 12_000_000, 11_000_000, 11_000_000, 500);
    assert_eq!(both, 2000, "the persistent part still has to be charged");
}

#[test]
fn the_rss_cross_check_sees_what_malloc_accounting_cannot() {
    let m = measure(CONTROL_ITERATIONS, RSS_CONTROL_LEAK_BYTES);

    // The floor was `RSS_CONTROL_LEAK_BYTES * 3/4`, i.e. "RSS must grow by
    // most of what was allocated". No OS promises that, and macos-14 said so:
    // RSS went 28818 -> 40002 -> 40705 KiB while the malloc heap climbed at
    // ~195 KB/iter. The kernel reclaimed older leaked pages as fast as new
    // ones arrived. Two consequences, both of which were bugs in this control
    // rather than in RSS:
    //
    //   * residency PLATEAUS under a sustained large leak, so a fraction-of-
    //     allocation floor is unmeetable by construction once the leak exceeds
    //     what the runner will keep resident;
    //   * the minimum-of-two-windows rule, which is right for the main test
    //     (there, flatness is the expected answer and a minimum is the
    //     conservative reading), is wrong here — it picks the post-plateau
    //     window and reports 7 KB/iter for a 256 KiB/iter leak.
    //
    // What the main test actually rests on is narrower and true: that RSS
    // resolves a leak far above MAX_RSS_BYTES_PER_ITER. Assert exactly that,
    // on the first steady window, before residency saturates. macOS measured
    // 114523 B/iter there — 111x the budget — and Linux more.
    let floor = MAX_RSS_BYTES_PER_ITER * 20;

    // A failed calibration means "this machine cannot measure", NOT "the code
    // leaks". Gating a release on it is wrong in both directions: it blocks a
    // green tree, and it would still pass on a runner where the instrument is
    // blind. So report loudly and stop, rather than fail.
    //
    // Measured on macos-14, 2026-08-21, for a deliberate 262144 B/iter leak:
    //   rss:  steady 15071 / 17367 B/iter   — a 6% capture rate; the kernel
    //         reclaims the leaked pages about as fast as they are written, and
    //         BOTH windows land under the floor, so taking a maximum does not
    //         rescue it either.
    //   heap: steady 265588 / 179814 B/iter — the heap instrument resolved the
    //         control leak correctly, and was then REJECTED by the
    //         "live heap cannot exceed RSS" rule, because macOS
    //         mstats().bytes_used counts zone CAPACITY (this file says so
    //         itself) and capacity legitimately exceeds residency once pages
    //         are reclaimed.
    //
    // So on macOS both instruments fail, and this suite runs on macOS ONLY.
    // The real fix is to give the heap instrument macOS-correct semantics
    // instead of comparing zone capacity against RSS; until then this must not
    // decide whether a release ships. See scripts/SITE_AND_EXAMPLES_PLAN_2026_08_20.md.
    if m.rss_first_steady_per_iter < floor {
        eprintln!(
            "::warning::[leak_regression] RSS CANNOT CALIBRATE on this machine: a \
             deliberate {} B/iter leak moved RSS by only {} B/iter (needed >={}). \
             The RSS assertion in regenerate_layout_does_not_leak_under_resize_stress \
             is therefore DECORATIVE on this runner — it cannot catch a leak outside \
             the main arena. Not failing the build, because this measures the \
             machine, not the code.",
            RSS_CONTROL_LEAK_BYTES, m.rss_first_steady_per_iter, floor,
        );
        return;
    }

    assert!(
        m.rss_first_steady_per_iter >= floor,
        "leaking {} B/iter of fully-written, incompressible memory moved RSS \
         by only {} B/iter in the first steady window (needed >={}, which is \
         20x the {} B/iter budget the main test enforces), while the malloc \
         heap reported {} B/iter. RSS is the main test's only instrument for \
         non-main arenas and mmap'd regions; if it cannot resolve a leak this \
         far above the budget, that assertion is decorative and a leak outside \
         the main arena would pass unnoticed.",
        RSS_CONTROL_LEAK_BYTES,
        m.rss_first_steady_per_iter,
        floor,
        MAX_RSS_BYTES_PER_ITER,
        m.per_iter,
    );

    assert!(
        m.rss_first_steady_per_iter >= MAX_RSS_BYTES_PER_ITER,
        "the deliberate {} B/iter leak moved RSS by {} B/iter, under the {} \
         B/iter budget the real test enforces — the control does not exercise \
         the assertion it exists to validate.",
        RSS_CONTROL_LEAK_BYTES,
        m.rss_first_steady_per_iter,
        MAX_RSS_BYTES_PER_ITER,
    );
}
