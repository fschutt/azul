#[allow(unused_imports)]
pub use super::*;

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact-value assertions on interpolation results
mod tests {
    use super::*;

    fn tick(n: u64) -> Instant {
        Instant::Tick(SystemTick::new(n))
    }
    fn tick_dur(n: u64) -> Duration {
        Duration::Tick(SystemTickDiff { tick_diff: n })
    }
    fn sys_dur(secs: u64, nanos: u32) -> Duration {
        Duration::System(SystemTimeDiff { secs, nanos })
    }

    /// The property the parallel E2E runner depends on: a `tick_ms` on one
    /// thread must not shift any other thread's clock. This is what allows a
    /// scenario that ticks to run in parallel with every other scenario
    /// instead of being serialised behind a process-global offset.
    #[test]
    #[cfg(feature = "std")]
    fn test_clock_offset_is_per_thread_not_process_global() {
        reset_test_clock();
        assert_eq!(test_clock_offset_ms(), 0);

        let (tx, rx) = std::sync::mpsc::channel();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let other = std::thread::spawn(move || {
            // Observed AFTER the main thread has advanced its own clock by 5 s.
            go_rx.recv().expect("handshake");
            let seen_after_main_ticked = test_clock_offset_ms();
            let _ = advance_test_clock_ms(7);
            tx.send((seen_after_main_ticked, test_clock_offset_ms()))
                .expect("send");
        });

        assert_eq!(advance_test_clock_ms(5_000), 5_000);
        go_tx.send(()).expect("handshake");
        let (other_before, other_after) = rx.recv().expect("recv");
        other.join().expect("join");

        assert_eq!(
            other_before, 0,
            "a tick on the main thread leaked into another thread's clock"
        );
        assert_eq!(other_after, 7, "the other thread must own its own offset");
        assert_eq!(
            test_clock_offset_ms(),
            5_000,
            "another thread's tick leaked into the main thread's clock"
        );

        // And a reset really is a reset (worker threads are reused).
        reset_test_clock();
        assert_eq!(test_clock_offset_ms(), 0);
    }

    /// A frozen clock must advance ONLY by what a scenario asks for.
    ///
    /// Offsetting alone left `Instant::now()` as `StdInstant::now() + offset`, so
    /// real time still flowed: elapsed time was what the scenario asked for PLUS
    /// whatever the machine happened to spend. Under the 8-wide E2E runner that
    /// second term is large and varies per run — enough to flip an assertion on a
    /// blinking caret's phase while the same scenario passes in isolation.
    #[test]
    #[cfg(feature = "std")]
    fn a_frozen_clock_advances_only_by_what_the_scenario_asks_for() {
        reset_test_clock();
        assert!(!test_clock_is_frozen());

        freeze_test_clock();
        assert!(test_clock_is_frozen());

        let t0 = Instant::now();
        // Burn REAL time. A frozen clock must not notice.
        std::thread::sleep(core::time::Duration::from_millis(25));
        let t1 = Instant::now();
        assert_eq!(
            t1.duration_since(&t0),
            Duration::System(SystemTimeDiff { secs: 0, nanos: 0 }),
            "real time leaked into a frozen clock",
        );

        // Only an explicit advance moves it, and by exactly that much.
        let _ = advance_test_clock_ms(500);
        let t2 = Instant::now();
        assert_eq!(
            t2.duration_since(&t0),
            Duration::System(SystemTimeDiff {
                secs: 0,
                nanos: 500_000_000
            }),
            "a 500 ms tick must read back as exactly 500 ms",
        );

        // Freezing is idempotent: it must not re-base and lose the offset.
        freeze_test_clock();
        assert_eq!(
            Instant::now().duration_since(&t0),
            Duration::System(SystemTimeDiff {
                secs: 0,
                nanos: 500_000_000
            }),
            "re-freezing re-based the clock and discarded elapsed virtual time",
        );

        // A reset must unfreeze, or the next scenario on this reused worker
        // thread would start with time stopped.
        reset_test_clock();
        assert!(!test_clock_is_frozen());
        let r0 = Instant::now();
        std::thread::sleep(core::time::Duration::from_millis(15));
        assert!(
            Instant::now().duration_since(&r0)
                > Duration::System(SystemTimeDiff { secs: 0, nanos: 0 }),
            "reset_test_clock left the clock frozen",
        );
    }

    #[test]
    fn linear_interpolate_zero_interval_is_one_not_nan() {
        let t = tick(5);
        let v = t.linear_interpolate(tick(5), tick(5));
        assert!(v.is_finite());
        assert_eq!(v, 1.0);
    }

    #[test]
    fn linear_interpolate_midpoint() {
        let v = tick(5).linear_interpolate(tick(0), tick(10));
        assert!((v - 0.5).abs() < 1e-6);
    }

    #[test]
    fn duration_since_saturates_on_negative() {
        // earlier is actually later -> saturate to zero, no panic.
        let d = tick(1).duration_since(&tick(10));
        assert_eq!(d, tick_dur(0));
    }

    /// Cross-unit comparison must answer TRUTHFULLY, not saturate to `false`.
    ///
    /// The saturating version made a unit mismatch a permanent silent "not yet":
    /// every interval constant in the engine is a `Duration::System`, so a Tick
    /// elapsed value compared against one never expired and the UI simply stopped
    /// animating, with nothing to catch.
    #[test]
    fn duration_compare_is_unit_aware_across_ticks_and_wall_clock() {
        // 5 ticks at 60Hz is ~83ms, i.e. LESS than one second.
        let five_ticks = tick_dur(5);
        let one_second = sys_dur(1, 0);
        assert!(five_ticks.smaller_than(&one_second));
        assert!(!five_ticks.greater_than(&one_second));
        assert!(one_second.greater_than(&five_ticks));
        assert!(!one_second.smaller_than(&five_ticks));

        // 120 ticks is two seconds, i.e. MORE than one second.
        assert!(tick_dur(120).greater_than(&one_second));
        assert!(one_second.smaller_than(&tick_dur(120)));

        // Exactly 60 ticks IS one second: neither greater nor smaller.
        assert!(!tick_dur(60).greater_than(&one_second));
        assert!(!tick_dur(60).smaller_than(&one_second));
        assert!(!one_second.greater_than(&tick_dur(60)));
        assert!(!one_second.smaller_than(&tick_dur(60)));
    }

    /// `Duration::max()` is `System`, and it must still dominate every tick count
    /// — including `u64::MAX` ticks, which is a bigger *number* but a smaller
    /// span.
    #[test]
    fn duration_compare_across_units_at_the_extremes() {
        assert!(Duration::max().greater_than(&tick_dur(u64::MAX)));
        assert!(tick_dur(u64::MAX).smaller_than(&Duration::max()));
        assert!(!tick_dur(0).greater_than(&sys_dur(0, 0)));
        assert!(!sys_dur(0, 0).greater_than(&tick_dur(0)));
        // One nanosecond beats zero ticks.
        assert!(sys_dur(0, 1).greater_than(&tick_dur(0)));
    }

    #[test]
    fn add_optional_duration_converts_across_units() {
        let inst = tick(100);
        // A System duration on a Tick instant advances by WHOLE ticks: 1s = 60.
        assert_eq!(inst.add_optional_duration(Some(&sys_dur(1, 0))), tick(160));
        // Sub-frame durations advance nothing — one frame is the resolution.
        assert_eq!(inst.add_optional_duration(Some(&sys_dur(0, 1))), tick(100));
        // Matching kinds add and saturate.
        assert_eq!(inst.add_optional_duration(Some(&tick_dur(5))), tick(105));
        // Saturating add: near-max tick doesn't overflow-panic.
        let big = tick(u64::MAX);
        assert_eq!(
            big.add_optional_duration(Some(&tick_dur(10))),
            tick(u64::MAX)
        );
        // ...and the cross-unit arm saturates too.
        assert_eq!(
            big.add_optional_duration(Some(&Duration::max())),
            tick(u64::MAX)
        );
    }

    #[test]
    fn millis_saturates_on_overflow() {
        let huge = SystemTimeDiff {
            secs: u64::MAX,
            nanos: 0,
        };
        assert_eq!(huge.millis(), u64::MAX);
        let normal = SystemTimeDiff {
            secs: 2,
            nanos: 500_000_000,
        };
        assert_eq!(normal.millis(), 2500);
    }

    /// Cross-unit division goes through the canonical scale. Returning `0.0` (the
    /// old behaviour) reads downstream as "this animation is at 0% progress",
    /// i.e. a frozen animation that never reports an error.
    #[test]
    fn duration_div_is_unit_aware() {
        // 30 ticks is half a second.
        assert!((tick_dur(30).div(&sys_dur(1, 0)) - 0.5).abs() < 1e-6);
        // ...and one second is two lots of 30 ticks.
        assert!((sys_dur(1, 0).div(&tick_dur(30)) - 2.0).abs() < 1e-6);
        // Matching Tick kinds divide normally.
        assert!((tick_dur(5).div(&tick_dur(10)) - 0.5).abs() < 1e-6);
        // Matching System kinds divide normally.
        assert!((sys_dur(1, 0).div(&sys_dur(2, 0)) - 0.5).abs() < 1e-6);
    }

    // Exercises the `unsafe` pointer work in `std_instant_clone` (`&*ptr`) and
    // the `ManuallyDrop::drop` guard in `InstantPtr::drop`: build an InstantPtr,
    // clone it (goes through the FFI clone callback + raw-ptr deref), then let
    // both drop. Under Miri this asserts the clone/drop path is UB-free and the
    // owned `Box` is freed exactly once per value (no double-free).
    #[cfg(feature = "std")]
    #[test]
    fn instant_ptr_clone_and_drop_no_ub() {
        let base = StdInstant::now();
        let a: InstantPtr = base.into();
        let b = a.clone();
        // The clone must observe the same underlying instant.
        assert_eq!(a, b);
        // Both `a` and `b` own independent Boxes; dropping both must not
        // double-free (each has run_destructor == true).
        drop(a);
        drop(b);
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact-value assertions on ratios / interpolation results
mod autotest_generated {
    use super::*;

    // ---- helpers -----------------------------------------------------------

    fn tick(n: u64) -> Instant {
        Instant::Tick(SystemTick::new(n))
    }
    fn tick_dur(n: u64) -> Duration {
        Duration::Tick(SystemTickDiff { tick_diff: n })
    }
    fn sys_dur(secs: u64, nanos: u32) -> Duration {
        Duration::System(SystemTimeDiff { secs, nanos })
    }

    // ========================================================================
    // TimerId::unique / ThreadId::unique  (monotonic, never hits reserved IDs)
    // ========================================================================

    #[test]
    fn timer_id_unique_is_strictly_increasing_and_above_reserved_range() {
        let a = TimerId::unique();
        let b = TimerId::unique();
        assert_ne!(a, b);
        assert!(
            b.id > a.id,
            "unique() must strictly increase: {a:?} -> {b:?}"
        );
        // User IDs must never land inside the reserved system-timer block.
        for id in [a, b] {
            assert!(
                id.id >= USER_TIMER_ID_START,
                "unique() handed out a reserved system ID: {id:?}"
            );
            assert_ne!(id, CURSOR_BLINK_TIMER_ID);
            assert_ne!(id, SCROLL_MOMENTUM_TIMER_ID);
            assert_ne!(id, DRAG_AUTOSCROLL_TIMER_ID);
            assert_ne!(id, TOOLTIP_DELAY_TIMER_ID);
            assert_ne!(id, CAPABILITY_PUMP_TIMER_ID);
            assert_ne!(id, LONG_PRESS_TIMER_ID);
        }
    }

    #[test]
    fn thread_id_unique_is_strictly_increasing_and_above_reserved_range() {
        let a = ThreadId::unique();
        let b = ThreadId::unique();
        assert_ne!(a, b);
        assert!(b.id > a.id);
        assert!(a.id >= RESERVED_THREAD_ID_COUNT);
    }

    // The counters are `AtomicUsize` + `fetch_add`, so concurrent callers must
    // never be handed the same ID. 8 threads x 64 IDs => 512 distinct values.
    #[cfg(feature = "std")]
    #[test]
    fn unique_ids_do_not_collide_across_threads() {
        use alloc::collections::BTreeSet;

        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    let mut out = Vec::new();
                    for _ in 0..64 {
                        out.push((TimerId::unique().id, ThreadId::unique().id));
                    }
                    out
                })
            })
            .collect();

        let mut timer_ids = BTreeSet::new();
        let mut thread_ids = BTreeSet::new();
        for h in handles {
            for (t, th) in h.join().expect("worker thread panicked") {
                assert!(timer_ids.insert(t), "duplicate TimerId handed out: {t}");
                assert!(thread_ids.insert(th), "duplicate ThreadId handed out: {th}");
            }
        }
        assert_eq!(timer_ids.len(), 8 * 64);
        assert_eq!(thread_ids.len(), 8 * 64);
    }

    // ========================================================================
    // Instant::now / get_system_time_libstd
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn instant_now_is_system_and_monotonic() {
        let a = Instant::now();
        let b = Instant::now();
        assert!(matches!(a, Instant::System(_)));
        assert!(a <= b, "Instant::now() went backwards");
        // A later instant is never "before" an earlier one.
        assert_eq!(a.duration_since(&b), sys_dur(0, 0));
    }

    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    #[test]
    fn get_system_time_libstd_is_monotonic_system_instant() {
        let a = get_system_time_libstd();
        let b = get_system_time_libstd();
        assert!(matches!(a, Instant::System(_)));
        assert!(matches!(b, Instant::System(_)));
        assert!(a <= b);
    }

    #[cfg(any(not(feature = "std"), target_arch = "wasm32"))]
    #[test]
    fn get_system_time_libstd_wasm_fallback_is_zero_tick() {
        // On WASM / no_std `StdInstant::now()` would panic, so the fallback must
        // hand back a tick instant instead of exploding.
        assert_eq!(get_system_time_libstd(), tick(0));
    }

    // ========================================================================
    // Instant::linear_interpolate  (must never return NaN / escape [0.0, 1.0])
    // ========================================================================

    #[test]
    fn linear_interpolate_clamps_outside_the_interval() {
        // before start -> 0.0, after end -> 1.0 (never negative / >1).
        assert_eq!(tick(0).linear_interpolate(tick(10), tick(20)), 0.0);
        assert_eq!(tick(999).linear_interpolate(tick(10), tick(20)), 1.0);
        // exactly on the boundaries
        assert_eq!(tick(10).linear_interpolate(tick(10), tick(20)), 0.0);
        assert_eq!(tick(20).linear_interpolate(tick(10), tick(20)), 1.0);
    }

    #[test]
    fn linear_interpolate_reversed_interval_is_normalized() {
        // `end < start` is swapped internally, so the ratio is the same as the
        // correctly-ordered call rather than a garbage / negative value.
        let forwards = tick(5).linear_interpolate(tick(0), tick(10));
        let backwards = tick(5).linear_interpolate(tick(10), tick(0));
        assert_eq!(forwards, backwards);
        assert!((backwards - 0.5).abs() < 1e-6);
    }

    #[test]
    fn linear_interpolate_saturating_extremes_stay_in_range() {
        // Full u64 span: the tick diff hits u64::MAX and the f64->f32 narrowing
        // must not produce inf/NaN.
        let v = tick(u64::MAX / 2).linear_interpolate(tick(0), tick(u64::MAX));
        assert!(
            v.is_finite(),
            "interpolation over the full u64 span went non-finite"
        );
        assert!((0.0..=1.0).contains(&v));
        assert!((v - 0.5).abs() < 1e-3, "expected ~0.5, got {v}");

        // Degenerate zero-length interval at the extremes -> 1.0, not 0/0 = NaN.
        let z = tick(u64::MAX).linear_interpolate(tick(u64::MAX), tick(u64::MAX));
        assert_eq!(z, 1.0);
        let z0 = tick(0).linear_interpolate(tick(0), tick(0));
        assert_eq!(z0, 1.0);
    }

    #[cfg(feature = "std")]
    #[test]
    fn linear_interpolate_mismatched_kinds_never_nan() {
        // Every mismatched (System / Tick) permutation feeds a 0/0 division
        // internally; the guard must turn that into a finite value in [0, 1].
        let sys = Instant::now();
        let cases = [
            (tick(5), sys.clone(), tick(10)),
            (sys.clone(), tick(0), tick(10)),
            (tick(5), tick(0), sys.clone()),
            (sys.clone(), sys.clone(), tick(10)),
            (tick(5), sys.clone(), sys.clone()),
        ];
        for (this, start, end) in cases {
            let v = this.linear_interpolate(start, end);
            assert!(v.is_finite(), "mismatched-kind interpolation returned {v}");
            assert!(
                (0.0..=1.0).contains(&v),
                "mismatched-kind interpolation escaped [0,1]: {v}"
            );
        }
    }

    // ========================================================================
    // Instant::add_optional_duration
    // ========================================================================

    #[test]
    fn add_optional_duration_none_is_identity() {
        let t = tick(42);
        assert_eq!(t.add_optional_duration(None), t);
        assert_eq!(tick(u64::MAX).add_optional_duration(None), tick(u64::MAX));
    }

    #[test]
    fn add_optional_duration_tick_saturates_at_u64_max() {
        // saturating_add: u64::MAX-1 + huge must clamp, not wrap or panic.
        let near_max = tick(u64::MAX - 1);
        assert_eq!(
            near_max.add_optional_duration(Some(&tick_dur(u64::MAX))),
            tick(u64::MAX)
        );
        assert_eq!(tick(0).add_optional_duration(Some(&tick_dur(0))), tick(0));
    }

    #[cfg(feature = "std")]
    #[test]
    fn add_optional_duration_system_advances_by_the_duration() {
        let base = Instant::now();
        let later =
            base.add_optional_duration(Some(&Duration::System(SystemTimeDiff::from_secs(1))));
        assert!(later > base);
        let delta = later.duration_since(&base);
        assert_eq!(delta, sys_dur(1, 0));
        // ... and the reverse span saturates to zero rather than going negative.
        assert_eq!(base.duration_since(&later), sys_dur(0, 0));
    }

    /// A `Tick` interval on a wall-clock instant has to ADVANCE that instant, not
    /// leave it alone. `Timer::instant_of_next_run` is exactly `last_run +
    /// delay + interval`; when this returned `self`, a tick-unit timer's next run
    /// was always "now" — permanently overdue, and `time_until_next_timer_ms`
    /// answered `Some(0)` for it.
    #[cfg(feature = "std")]
    #[test]
    fn add_optional_duration_converts_between_units_in_both_directions() {
        let sys = Instant::now();
        // System instant + Tick duration: 60 ticks is exactly one second.
        let later = sys.add_optional_duration(Some(&tick_dur(60)));
        assert!(
            later > sys,
            "a tick interval must advance a wall-clock instant"
        );
        assert_eq!(later.duration_since(&sys), sys_dur(1, 0));

        // 0 ticks is genuinely no time at all.
        assert_eq!(sys.add_optional_duration(Some(&tick_dur(0))), sys);

        // Tick instant + System duration: 3s is 180 whole frames.
        assert_eq!(
            tick(7).add_optional_duration(Some(&sys_dur(3, 0))),
            tick(187)
        );
    }

    // A `System` instant plus an enormous `System` duration overflows the
    // platform clock representation: `StdInstant + StdDuration` panics with
    // "overflow when adding duration to instant". Unlike the mismatched-kind
    // case (documented to saturate), this arm has no guard -- characterized
    // here so a future saturating fix flips this test loudly.
    #[cfg(all(feature = "std", not(target_arch = "wasm32")))]
    #[test]
    #[should_panic(expected = "overflow")]
    fn add_optional_duration_system_overflow_panics_today() {
        let base = Instant::now();
        let _ = base.add_optional_duration(Some(&Duration::max()));
    }

    // ========================================================================
    // Instant::duration_since / into_std_instant
    // ========================================================================

    #[test]
    fn duration_since_tick_saturates_and_is_exact() {
        assert_eq!(tick(10).duration_since(&tick(4)), tick_dur(6));
        // self == earlier -> zero span
        assert_eq!(tick(10).duration_since(&tick(10)), tick_dur(0));
        // earlier is later -> saturate to zero, no underflow panic
        assert_eq!(tick(0).duration_since(&tick(u64::MAX)), tick_dur(0));
        // full-range span does not overflow
        assert_eq!(tick(u64::MAX).duration_since(&tick(0)), tick_dur(u64::MAX));
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_since_mismatched_kinds_is_zero_tick_both_directions() {
        let sys = Instant::now();
        assert_eq!(sys.duration_since(&tick(5)), tick_dur(0));
        assert_eq!(tick(5).duration_since(&sys), tick_dur(0));
    }

    #[cfg(feature = "std")]
    #[test]
    fn into_std_instant_round_trips_a_system_instant() {
        let base = StdInstant::now();
        let wrapped: Instant = base.into();
        assert_eq!(wrapped.into_std_instant(), base);
    }

    #[cfg(feature = "std")]
    #[test]
    #[should_panic(expected = "internal error: entered unreachable code")]
    fn into_std_instant_on_tick_variant_panics() {
        // Documented: `into_std_instant` is `unreachable!()` for Tick instants.
        let _ = tick(1).into_std_instant();
    }

    // ========================================================================
    // SystemTick::new
    // ========================================================================

    #[test]
    fn system_tick_new_stores_the_counter_verbatim() {
        for n in [0_u64, 1, 0x0100, u64::MAX / 2, u64::MAX] {
            assert_eq!(SystemTick::new(n).tick_counter, n);
        }
        // Ordering follows the counter (used by Instant's derived Ord).
        assert!(SystemTick::new(0) < SystemTick::new(u64::MAX));
        assert_eq!(SystemTick::new(7), SystemTick::new(7));
    }

    // ========================================================================
    // InstantPtr: get / std_instant_clone / std_instant_drop
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn instant_ptr_get_returns_the_wrapped_instant() {
        let base = StdInstant::now();
        let p: InstantPtr = base.into();
        assert_eq!(p.get(), base);
        // `get` is a copy, not a move: repeated reads stay stable.
        assert_eq!(p.get(), p.get());
        assert!(p.run_destructor);
        // Debug must not panic and must not be empty.
        assert!(!alloc::format!("{p:?}").is_empty());
    }

    #[cfg(feature = "std")]
    #[test]
    fn std_instant_clone_deep_copies_and_arms_the_destructor() {
        let base = StdInstant::now();
        let a: InstantPtr = base.into();
        let cloned = std_instant_clone(core::ptr::from_ref(&a));
        assert_eq!(cloned.get(), base);
        // The clone owns its OWN box (freeing both must not double-free).
        assert!(!core::ptr::eq(&**a.ptr, &**cloned.ptr));
        assert!(
            cloned.run_destructor,
            "clone handed back a disarmed destructor"
        );
        drop(cloned);
        // The source survives its clone being dropped.
        assert_eq!(a.get(), base);
    }

    #[cfg(feature = "std")]
    #[test]
    fn std_instant_drop_is_a_noop_even_for_null() {
        // The libstd destructor callback is deliberately empty: the Box is freed
        // by `InstantPtr::drop` under the `run_destructor` guard. Calling it with
        // a null pointer must therefore be harmless.
        std_instant_drop(core::ptr::null_mut());

        let mut p: InstantPtr = StdInstant::now().into();
        let before = p.get();
        std_instant_drop(core::ptr::from_mut(&mut p));
        // Value is untouched and still owned afterwards.
        assert_eq!(p.get(), before);
        assert!(p.run_destructor);
    }

    // ========================================================================
    // Duration::fmt (Display)
    // ========================================================================

    #[test]
    fn duration_display_tick_edge_values() {
        assert_eq!(alloc::format!("{}", tick_dur(0)), "0 ticks");
        assert_eq!(alloc::format!("{}", tick_dur(1)), "1 ticks");
        assert_eq!(
            alloc::format!("{}", tick_dur(u64::MAX)),
            "18446744073709551615 ticks"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_display_system_edge_values_do_not_panic() {
        // zero, sub-second, denormalized nanos and the absolute maximum all have
        // to format without panicking and without producing an empty string.
        for d in [
            sys_dur(0, 0),
            sys_dur(1, 500_000_000),
            sys_dur(0, u32::MAX),
            sys_dur(u64::MAX, NANOS_PER_SEC - 1),
            Duration::max(),
        ] {
            let s = alloc::format!("{d}");
            assert!(!s.is_empty());
            assert!(
                !s.ends_with("ticks"),
                "System duration formatted as ticks: {s}"
            );
        }
    }

    // ========================================================================
    // Duration::max / div / min / greater_than / smaller_than
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn duration_max_is_the_upper_bound() {
        let m = Duration::max();
        assert_eq!(m, sys_dur(u64::MAX, NANOS_PER_SEC - 1));
        // Nothing of the same kind is greater than it...
        assert!(m.greater_than(&sys_dur(u64::MAX, NANOS_PER_SEC - 2)));
        assert!(m.greater_than(&sys_dur(0, 0)));
        // ... and it is not greater/smaller than itself.
        assert!(!m.greater_than(&m));
        assert!(!m.smaller_than(&m));
        // Converting the maximum back to std must not overflow-panic.
        let Duration::System(inner) = m else {
            panic!("Duration::max() is not a System duration under std")
        };
        assert_eq!(inner.get(), StdDuration::new(u64::MAX, NANOS_PER_SEC - 1));
    }

    #[test]
    fn duration_div_by_zero_yields_inf_or_nan_not_a_panic() {
        // 0/0 -> NaN, x/0 -> +inf. Neither may panic.
        assert!(tick_dur(0).div(&tick_dur(0)).is_nan());
        let inf = tick_dur(5).div(&tick_dur(0));
        assert!(inf.is_infinite() && inf.is_sign_positive());

        assert!(sys_dur(0, 0).div(&sys_dur(0, 0)).is_nan());
        let sinf = sys_dur(1, 0).div(&sys_dur(0, 0));
        assert!(sinf.is_infinite() && sinf.is_sign_positive());
    }

    #[test]
    fn duration_div_extremes_stay_finite_in_f32() {
        // u64::MAX / 1 ~= 1.8e19, comfortably inside f32 range: the f64 -> f32
        // narrowing must not produce inf.
        let r = tick_dur(u64::MAX).div(&tick_dur(1));
        assert!(r.is_finite(), "u64::MAX tick ratio overflowed f32: {r}");
        assert!(r > 1e19);
        // Identity ratios are exactly 1.0 for both kinds.
        assert_eq!(tick_dur(u64::MAX).div(&tick_dur(u64::MAX)), 1.0);
        assert_eq!(sys_dur(3, 0).div(&sys_dur(2, 0)), 1.5);
    }

    /// Cross-unit division converts instead of collapsing to `0.0`. 10 ticks is
    /// one sixth of a second, so the two ratios are reciprocals of each other —
    /// which is the property `0.0` both ways could never satisfy.
    #[test]
    fn duration_div_across_kinds_converts_both_ways() {
        assert!((sys_dur(1, 0).div(&tick_dur(10)) - 6.0).abs() < 1e-5);
        assert!((tick_dur(10).div(&sys_dur(1, 0)) - (1.0 / 6.0)).abs() < 1e-5);
    }

    #[test]
    fn duration_min_picks_the_smaller_of_the_same_kind() {
        assert_eq!(tick_dur(5).min(tick_dur(10)), tick_dur(5));
        assert_eq!(tick_dur(10).min(tick_dur(5)), tick_dur(5));
        assert_eq!(tick_dur(7).min(tick_dur(7)), tick_dur(7));
        assert_eq!(tick_dur(0).min(tick_dur(u64::MAX)), tick_dur(0));
        // Comparison no longer needs std: it is u128 nanosecond arithmetic.
        assert_eq!(sys_dur(1, 0).min(sys_dur(1, 1)), sys_dur(1, 0));
    }

    /// `min` is built on `smaller_than`, so it picks the genuinely shorter span
    /// across units and is COMMUTATIVE. It used to just return `other` whenever
    /// the units differed — so `a.min(b)` and `b.min(a)` disagreed, and the
    /// answer depended on argument order rather than on the durations.
    #[test]
    fn duration_min_across_kinds_picks_the_genuinely_shorter_span() {
        // 5 ticks is ~83ms, so it is shorter than a second either way round.
        assert_eq!(tick_dur(5).min(sys_dur(1, 0)), tick_dur(5));
        assert_eq!(sys_dur(1, 0).min(tick_dur(5)), tick_dur(5));
        // ...and 120 ticks is 2s, so the second wins either way round.
        assert_eq!(tick_dur(120).min(sys_dur(1, 0)), sys_dur(1, 0));
        assert_eq!(sys_dur(1, 0).min(tick_dur(120)), sys_dur(1, 0));
    }

    #[test]
    fn duration_comparison_is_a_strict_total_order_within_a_kind() {
        let mut pairs = alloc::vec![
            (tick_dur(0), tick_dur(u64::MAX)),
            (tick_dur(1), tick_dur(2))
        ];
        // System ordering no longer needs std: the comparison is u128 nanosecond
        // arithmetic. It used to defer to `StdDuration`, so on no_std it answered
        // `false` for every System pair — a total order that ordered nothing.
        pairs.extend_from_slice(&[
            (sys_dur(0, 0), sys_dur(u64::MAX, 0)),
            (sys_dur(1, 999_999_999), sys_dur(2, 0)),
        ]);

        for (a, b) in pairs {
            assert!(a.smaller_than(&b));
            assert!(b.greater_than(&a));
            assert!(!a.greater_than(&b));
            assert!(!b.smaller_than(&a));
        }
        // Equal values: neither greater nor smaller (holds for both kinds).
        let eq = tick_dur(4);
        assert!(!eq.greater_than(&eq));
        assert!(!eq.smaller_than(&eq));
        let eq_sys = sys_dur(4, 2);
        assert!(!eq_sys.greater_than(&eq_sys));
        assert!(!eq_sys.smaller_than(&eq_sys));
    }

    #[cfg(feature = "std")]
    #[test]
    fn duration_comparison_normalizes_denormalized_nanos() {
        // nanos == u32::MAX (> 1e9) is denormalized; the std conversion carries
        // it into secs, so {0, u32::MAX} == 4.294967295s > 4s.
        let denorm = sys_dur(0, u32::MAX);
        assert!(denorm.greater_than(&sys_dur(4, 0)));
        assert!(denorm.smaller_than(&sys_dur(5, 0)));
    }

    // ========================================================================
    // SystemTickDiff::div / SystemTimeDiff::div + as_secs_f64
    // ========================================================================

    #[test]
    fn system_tick_diff_div_edge_cases() {
        let zero = SystemTickDiff { tick_diff: 0 };
        let one = SystemTickDiff { tick_diff: 1 };
        let max = SystemTickDiff {
            tick_diff: u64::MAX,
        };

        assert!(zero.div(&zero).is_nan());
        assert!(one.div(&zero).is_infinite());
        assert_eq!(zero.div(&one), 0.0);
        assert_eq!(max.div(&max), 1.0);
        assert!(max.div(&one).is_finite());
        assert_eq!(
            SystemTickDiff { tick_diff: 5 }.div(&SystemTickDiff { tick_diff: 10 }),
            0.5
        );
    }

    #[test]
    fn system_time_diff_as_secs_f64_is_exact_for_representable_values() {
        assert_eq!(SystemTimeDiff { secs: 0, nanos: 0 }.as_secs_f64(), 0.0);
        assert_eq!(
            SystemTimeDiff {
                secs: 1,
                nanos: 500_000_000
            }
            .as_secs_f64(),
            1.5
        );
        assert_eq!(
            SystemTimeDiff {
                secs: 0,
                nanos: 500_000_000
            }
            .as_secs_f64(),
            0.5
        );
        // Extremes stay finite (u64::MAX secs ~= 1.8e19, well inside f64).
        let huge = SystemTimeDiff {
            secs: u64::MAX,
            nanos: NANOS_PER_SEC - 1,
        };
        assert!(huge.as_secs_f64().is_finite());
        assert!(huge.as_secs_f64() > 1e19);
        // Monotone in secs.
        assert!(
            SystemTimeDiff::from_secs(2).as_secs_f64() > SystemTimeDiff::from_secs(1).as_secs_f64()
        );
    }

    #[test]
    fn system_time_diff_div_edge_cases() {
        let zero = SystemTimeDiff { secs: 0, nanos: 0 };
        let one = SystemTimeDiff::from_secs(1);
        let half = SystemTimeDiff {
            secs: 0,
            nanos: 500_000_000,
        };

        assert!(zero.div(&zero).is_nan());
        assert!(one.div(&zero).is_infinite());
        assert_eq!(zero.div(&one), 0.0);
        assert_eq!(one.div(&one), 1.0);
        assert_eq!(one.div(&half), 2.0);
        let max = SystemTimeDiff {
            secs: u64::MAX,
            nanos: NANOS_PER_SEC - 1,
        };
        assert_eq!(max.div(&max), 1.0);
        assert!(max.div(&one).is_finite());
    }

    // ========================================================================
    // SystemTimeDiff constructors: from_secs / from_millis / from_nanos
    // ========================================================================

    #[test]
    fn from_secs_invariants() {
        for s in [0_u64, 1, 1_000, u64::MAX] {
            let d = SystemTimeDiff::from_secs(s);
            assert_eq!(d.secs, s);
            assert_eq!(d.nanos, 0, "from_secs must leave nanos at zero");
        }
    }

    #[test]
    fn from_millis_normalizes_and_keeps_nanos_in_range() {
        assert_eq!(
            SystemTimeDiff::from_millis(0),
            SystemTimeDiff { secs: 0, nanos: 0 }
        );
        assert_eq!(
            SystemTimeDiff::from_millis(999),
            SystemTimeDiff {
                secs: 0,
                nanos: 999_000_000
            }
        );
        assert_eq!(
            SystemTimeDiff::from_millis(1_000),
            SystemTimeDiff { secs: 1, nanos: 0 }
        );
        assert_eq!(
            SystemTimeDiff::from_millis(1_500),
            SystemTimeDiff {
                secs: 1,
                nanos: 500_000_000
            }
        );
        // u64::MAX millis must not overflow the u32 nanos field.
        let max = SystemTimeDiff::from_millis(u64::MAX);
        assert!(
            max.nanos < NANOS_PER_SEC,
            "from_millis produced denormalized nanos"
        );
        assert_eq!(max.secs, u64::MAX / MILLIS_PER_SEC);
    }

    #[test]
    fn from_nanos_normalizes_and_keeps_nanos_in_range() {
        assert_eq!(
            SystemTimeDiff::from_nanos(0),
            SystemTimeDiff { secs: 0, nanos: 0 }
        );
        assert_eq!(
            SystemTimeDiff::from_nanos(999_999_999),
            SystemTimeDiff {
                secs: 0,
                nanos: 999_999_999
            }
        );
        assert_eq!(
            SystemTimeDiff::from_nanos(1_000_000_000),
            SystemTimeDiff { secs: 1, nanos: 0 }
        );
        for n in [0_u64, 1, 999_999_999, 1_000_000_001, u64::MAX] {
            let d = SystemTimeDiff::from_nanos(n);
            assert!(
                d.nanos < NANOS_PER_SEC,
                "from_nanos({n}) produced denormalized nanos"
            );
            // Lossless round-trip: secs * 1e9 + nanos == n (checked in u128).
            let back = u128::from(d.secs) * u128::from(NANOS_PER_SEC) + u128::from(d.nanos);
            assert_eq!(back, u128::from(n), "from_nanos({n}) lost information");
        }
    }

    // ========================================================================
    // Round-trip: from_millis <-> millis
    // ========================================================================

    #[test]
    fn millis_round_trips_through_from_millis() {
        // Exact for every whole-millisecond value, INCLUDING u64::MAX (where
        // `secs * 1000 + 615` lands exactly on u64::MAX without saturating).
        for m in [0_u64, 1, 999, 1_000, 1_500, 86_400_000, u64::MAX] {
            assert_eq!(
                SystemTimeDiff::from_millis(m).millis(),
                m,
                "from_millis({m}).millis() is not lossless"
            );
        }
    }

    #[test]
    fn millis_truncates_and_saturates_instead_of_panicking() {
        // Sub-millisecond nanos truncate towards zero.
        assert_eq!(
            SystemTimeDiff {
                secs: 0,
                nanos: 999_999
            }
            .millis(),
            0
        );
        assert_eq!(
            SystemTimeDiff {
                secs: 0,
                nanos: 999_999_999
            }
            .millis(),
            999
        );
        // secs * 1000 overflows u64 -> saturate at u64::MAX, no panic.
        assert_eq!(
            SystemTimeDiff {
                secs: u64::MAX,
                nanos: 0
            }
            .millis(),
            u64::MAX
        );
        assert_eq!(
            SystemTimeDiff {
                secs: u64::MAX,
                nanos: NANOS_PER_SEC - 1
            }
            .millis(),
            u64::MAX
        );
        assert_eq!(
            SystemTimeDiff::from_secs(u64::MAX / 1_000).millis(),
            (u64::MAX / 1_000) * 1_000
        );
    }

    // ========================================================================
    // SystemTimeDiff::checked_add
    // ========================================================================

    #[test]
    fn checked_add_carries_nanos_into_secs() {
        let a = SystemTimeDiff {
            secs: 0,
            nanos: 999_999_999,
        };
        let sum = a.checked_add(a).expect("0.999s + 0.999s must not overflow");
        assert_eq!(
            sum,
            SystemTimeDiff {
                secs: 1,
                nanos: 999_999_998
            }
        );
        // Exactly one second of nanos carries cleanly.
        let b = SystemTimeDiff {
            secs: 1,
            nanos: 500_000_000,
        };
        assert_eq!(b.checked_add(b), Some(SystemTimeDiff { secs: 3, nanos: 0 }));
    }

    #[test]
    fn checked_add_returns_none_on_overflow_instead_of_panicking() {
        let max_secs = SystemTimeDiff {
            secs: u64::MAX,
            nanos: 0,
        };
        // secs overflow
        assert_eq!(max_secs.checked_add(SystemTimeDiff::from_secs(1)), None);
        // secs at max, nanos still fit -> Some
        assert_eq!(
            max_secs.checked_add(SystemTimeDiff {
                secs: 0,
                nanos: NANOS_PER_SEC - 1
            }),
            Some(SystemTimeDiff {
                secs: u64::MAX,
                nanos: NANOS_PER_SEC - 1
            })
        );
        // overflow that only happens because of the nanos CARRY
        let brim = SystemTimeDiff {
            secs: u64::MAX,
            nanos: NANOS_PER_SEC - 1,
        };
        assert_eq!(brim.checked_add(SystemTimeDiff { secs: 0, nanos: 1 }), None);
    }

    #[test]
    fn checked_add_identity_and_commutativity() {
        let zero = SystemTimeDiff { secs: 0, nanos: 0 };
        for d in [
            SystemTimeDiff::from_secs(0),
            SystemTimeDiff::from_millis(1_500),
            SystemTimeDiff::from_nanos(u64::MAX),
            SystemTimeDiff {
                secs: u64::MAX,
                nanos: 0,
            },
        ] {
            assert_eq!(d.checked_add(zero), Some(d));
            assert_eq!(zero.checked_add(d), Some(d));
            // a + b == b + a for well-formed operands
            let other = SystemTimeDiff::from_millis(750);
            assert_eq!(d.checked_add(other), other.checked_add(d));
        }
    }

    // ========================================================================
    // SystemTimeDiff::get  (std::time::Duration conversion round-trip)
    // ========================================================================

    #[cfg(feature = "std")]
    #[test]
    fn system_time_diff_get_round_trips_std_duration() {
        for std_d in [
            StdDuration::ZERO,
            StdDuration::from_millis(1_500),
            StdDuration::from_nanos(1),
            StdDuration::new(u64::MAX, NANOS_PER_SEC - 1),
        ] {
            let mid: SystemTimeDiff = std_d.into();
            assert_eq!(
                mid.get(),
                std_d,
                "StdDuration -> SystemTimeDiff -> StdDuration lost data"
            );
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn system_time_diff_get_on_edge_values_does_not_panic() {
        assert_eq!(
            SystemTimeDiff { secs: 0, nanos: 0 }.get(),
            StdDuration::ZERO
        );
        // secs at max with zero nanos: no carry, so no overflow in Duration::new.
        assert_eq!(
            SystemTimeDiff::from_secs(u64::MAX).get(),
            StdDuration::new(u64::MAX, 0)
        );
        // Denormalized nanos (>= 1e9) are carried by Duration::new, not rejected.
        assert_eq!(
            SystemTimeDiff {
                secs: 0,
                nanos: u32::MAX
            }
            .get(),
            StdDuration::new(0, u32::MAX)
        );
    }

    // ========================================================================
    // ThreadReceiver: new / get_ctx / recv / clone
    // ========================================================================

    #[cfg(feature = "std")]
    extern "C" fn test_thread_recv(ptr: *const c_void) -> OptionThreadSendMsg {
        // Mirrors the real callback: `ThreadReceiver::recv` hands over a pointer
        // to the boxed `Receiver<ThreadSendMsg>` inside `ThreadReceiverInner`.
        let receiver = unsafe { &*(ptr.cast::<Receiver<ThreadSendMsg>>()) };
        receiver.try_recv().ok().into()
    }

    #[cfg(feature = "std")]
    const extern "C" fn test_thread_recv_destructor(_: *mut ThreadReceiverInner) {}

    #[cfg(feature = "std")]
    fn test_receiver() -> (Sender<ThreadSendMsg>, ThreadReceiver) {
        let (tx, rx) = std::sync::mpsc::channel::<ThreadSendMsg>();
        let inner = ThreadReceiverInner {
            ptr: Box::new(rx),
            recv_fn: ThreadRecvCallback {
                cb: test_thread_recv,
            },
            destructor: ThreadReceiverDestructorCallback {
                cb: test_thread_recv_destructor,
            },
        };
        (tx, ThreadReceiver::new(inner))
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_new_arms_destructor_and_has_no_ctx() {
        let (_tx, r) = test_receiver();
        assert!(
            r.run_destructor,
            "ThreadReceiver::new left the destructor disarmed"
        );
        assert!(
            r.get_ctx().is_none(),
            "a fresh receiver must have no FFI context"
        );
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_recv_on_empty_and_disconnected_channel_is_none() {
        let (tx, mut r) = test_receiver();
        // Empty channel -> None (must not block / panic).
        assert!(r.recv().is_none());
        // Disconnected channel -> still None, not a panic.
        drop(tx);
        assert!(r.recv().is_none());
        assert!(r.recv().is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_recv_delivers_messages_in_order() {
        let (tx, mut r) = test_receiver();
        tx.send(ThreadSendMsg::Tick).unwrap();
        tx.send(ThreadSendMsg::Custom(RefAny::new(42_u32))).unwrap();
        tx.send(ThreadSendMsg::TerminateThread).unwrap();

        assert_eq!(r.recv(), OptionThreadSendMsg::Some(ThreadSendMsg::Tick));
        assert!(matches!(
            r.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::Custom(_))
        ));
        assert_eq!(
            r.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        );
        // Drained.
        assert!(r.recv().is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_clone_shares_the_same_channel() {
        let (tx, mut a) = test_receiver();
        let mut b = a.clone();
        assert!(b.run_destructor);

        tx.send(ThreadSendMsg::Tick).unwrap();
        // The clone shares the Arc<Mutex<..>>: whichever half receives first
        // consumes the message; the other must see an empty channel, not a
        // duplicate and not a deadlock.
        assert_eq!(a.recv(), OptionThreadSendMsg::Some(ThreadSendMsg::Tick));
        assert!(b.recv().is_none());

        tx.send(ThreadSendMsg::TerminateThread).unwrap();
        assert_eq!(
            b.recv(),
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
        );
        assert!(a.recv().is_none());
    }

    #[cfg(feature = "std")]
    #[test]
    fn thread_receiver_get_ctx_clones_rather_than_takes() {
        let (_tx, mut r) = test_receiver();
        r.ctx = OptionRefAny::Some(RefAny::new(7_u64));
        // Repeated reads must all succeed -- `get_ctx` clones the RefAny (refcount
        // bump); a take/move would leave the second call empty.
        assert!(r.get_ctx().is_some());
        assert!(r.get_ctx().is_some());
        let held = r.get_ctx();
        drop(r);
        // The cloned handle outlives the receiver it came from.
        assert!(held.is_some());
    }
}
