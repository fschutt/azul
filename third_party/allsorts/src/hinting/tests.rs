//! Instruction-level golden tests for the TrueType bytecode interpreter.
//!
//! Ground truth is **FreeType v40** (`src/truetype/ttinterp.c`) plus the
//! Microsoft/Apple TrueType instruction-set specification.  Every expected
//! value is hand-computed in a comment using F26Dot6 arithmetic
//! (1px = 64 units; rounding is sign-magnitude: round the magnitude, reapply
//! the sign).
//!
//! These are **tests-first**: the interpreter currently has known bugs that a
//! separate agent fixes next, so several of these tests are EXPECTED to fail
//! today.  Each encodes the FreeType-correct outcome — do not weaken one to
//! match current behaviour.  Comments tag which confirmed bug each pins.
//!
//! Reachability: most tests drive the public API
//! (`Interpreter::hint_glyph`) on synthetic zone points and read back
//! `zones[1]`.  A few pin pure state math via the public `GraphicsState`
//! rounding API.  Where the public surface cannot express an input (e.g. the
//! phantom-point origin), the closest reachable behaviour is tested and the
//! gap is noted (see `phantom_pp1_x_*`).

use super::f26dot6::{compute_scale, F26Dot6};
use super::graphics_state::{GraphicsState, RoundState};
use super::interpreter::{HintError, Interpreter, Point, PointFlags};

// ── helpers ──────────────────────────────────────────────────────────

/// Interpreter with generous maxp limits (stack headroom for most tests).
/// Args: max_stack, max_storage, max_fdefs, max_idefs, max_twilight, upem.
fn interp() -> Interpreter {
    Interpreter::new(64, 32, 16, 4, 32, 1000)
}

/// Run `program` over `pts` (all on-curve, single contour) and return the
/// interpreter so callers can inspect `zones[1]`.
fn run(itp: &mut Interpreter, pts: &[(i32, i32)], program: &[u8]) -> Result<(), HintError> {
    let points: Vec<Point> = pts.iter().map(|&(x, y)| Point { x, y }).collect();
    let on = vec![true; points.len()];
    let ends = vec![points.len().saturating_sub(1) as u16];
    itp.hint_glyph(&points, &on, &ends, program)
}

fn cur(itp: &Interpreter, i: usize) -> (i32, i32) {
    let p = itp.zones[1].current[i];
    (p.x, p.y)
}

/// (touched_x, touched_y) for glyph-zone point `i`.
fn touched(itp: &Interpreter, i: usize) -> (bool, bool) {
    let f = itp.zones[1].flags[i];
    (
        f.contains(PointFlags::TOUCHED_X),
        f.contains(PointFlags::TOUCHED_Y),
    )
}

/// Round `d` (F26Dot6 bits) under `mode` via the public GraphicsState API.
fn round_with(mode: RoundState, d: i32) -> i32 {
    let mut gs = GraphicsState::default();
    gs.round_state = mode;
    gs.round(F26Dot6::from_bits(d)).to_bits()
}

// ── Test 1 — round-state table (gates bug #1) ────────────────────────
//
// FreeType Round_To_Grid / _Half_Grid / _Double_Grid / _Down / _Up with
// compensation = 0.  Magnitude is rounded then the sign reapplied.
//   RTG  : (|d|+32) & !63          RTHG : (|d| & !63) + 32
//   RTDG : (|d|+16) & !31          RDTG : |d| & !63   (floor)
//   RUTG : (|d|+63) & !63 (ceil)   ROFF : identity
// Inputs: 0 31 32 33 63 64 95 96  -31 -32 -33 -63 -64

#[test]
fn round_state_table_matches_freetype() {
    const IN: [i32; 13] = [0, 31, 32, 33, 63, 64, 95, 96, -31, -32, -33, -63, -64];
    // Hand-computed per the formulas above.
    const RTG: [i32; 13] = [0, 0, 64, 64, 64, 64, 64, 128, 0, -64, -64, -64, -64];
    const RTHG: [i32; 13] = [32, 32, 32, 32, 32, 96, 96, 96, -32, -32, -32, -32, -96];
    const RTDG: [i32; 13] = [0, 32, 32, 32, 64, 64, 96, 96, -32, -32, -32, -64, -64];
    const RDTG: [i32; 13] = [0, 0, 0, 0, 0, 64, 64, 64, 0, 0, 0, 0, -64];
    const RUTG: [i32; 13] = [0, 64, 64, 64, 64, 64, 128, 128, -64, -64, -64, -64, -64];

    let cases: [(RoundState, &[i32; 13]); 6] = [
        (RoundState::Grid, &RTG),
        (RoundState::HalfGrid, &RTHG),
        (RoundState::DoubleGrid, &RTDG),
        (RoundState::DownToGrid, &RDTG),
        (RoundState::UpToGrid, &RUTG),
        (RoundState::Off, &IN), // ROFF is the identity
    ];

    for (mode, expected) in cases {
        for (k, &d) in IN.iter().enumerate() {
            assert_eq!(
                round_with(mode, d),
                expected[k],
                "mode {:?} input {}",
                mode,
                d
            );
        }
    }
}

// ── Test 2 — SROUND period/phase decode (gates bug #2) ───────────────
//
// FreeType SetSuperRound(GridPeriod=0x40) for SROUND:
//   period  = GridPeriod {>>1, ==, <<1} for period bits {0,1,2}
//   phase   = period * {0, 1/4, 1/2, 3/4} for phase bits {0,1,2,3}
//            (NOT a hardcoded 0/16/32/48 — that only happens to be right
//             when period == 64).
// A hardcoded-16 phase impl passes period=64/phase=1 but FAILS period=128.

#[test]
fn sround_phase_scales_with_period() {
    // selector bits: [period:7..6][phase:5..4][threshold:3..0]
    let decode = |n: u32| -> (i32, i32) {
        let mut gs = GraphicsState::default();
        gs.set_super_round(n, false);
        (
            gs.super_round_period.to_bits(),
            gs.super_round_phase.to_bits(),
        )
    };

    // period bits=1 -> 64 (1px). phase bits=1 -> 64/4 = 16.
    assert_eq!(decode(0x50), (64, 16), "period=64 phase=1/4");
    // period bits=2 -> 128 (2px). phase bits=1 -> 128/4 = 32  (NOT 16).
    assert_eq!(decode(0x90), (128, 32), "period=128 phase=1/4");
    // period=128, phase bits=2 -> 128/2 = 64  (NOT 32).
    assert_eq!(decode(0xA0), (128, 64), "period=128 phase=1/2");
    // period=128, phase bits=3 -> 128*3/4 = 96  (NOT 48).
    assert_eq!(decode(0xB0), (128, 96), "period=128 phase=3/4");
}

// ── Test 3 — DELTAP1 argument order (gates bug #3) ───────────────────
//
// FreeType Ins_DELTAP: after popping the count, each pair is
// stack[args] = spec byte (ppem/delta), stack[args+1] = POINT number, i.e.
// the point index is on TOP and is popped first, the spec byte second.
// A swapped-order impl pokes the wrong index and leaves the real point put.

#[test]
fn deltap1_pops_point_then_arg() {
    let mut itp = interp();
    itp.ppem = 9; // delta_base defaults to 9 -> ppem offset 0 targets ppem 9

    // Stack bottom->top must be [spec, point, count]:
    //   count = 1 (popped first)
    //   point = 0 (popped next, from TOP of the pair)
    //   spec  = 0x08 -> ppem offset 0 (=> target ppem 9), magnitude 8 => +1 step
    // delta_shift default 3 => move = +1 * 64 / 8 = +8 F26Dot6 along x.
    // Point 0 starts at x=100 => expected 108.
    let prog = [0xB2, 0x08, 0x00, 0x01, 0x5D]; // PUSHB[2] 08 00 01 ; DELTAP1
    run(&mut itp, &[(100, 0)], &prog).unwrap();

    assert_eq!(
        cur(&itp, 0).0,
        108,
        "DELTAP1 must move point 0 by +8 (point index popped from top)"
    );
}

// ── Test 4 — WCVTP out-of-range index (gates bug #4) ─────────────────
//
// FreeType Ins_WCVTP bounds-checks the index against cvtSize and silently
// returns on an out-of-range write (errors only under pedantic hinting).
// The buggy impl `resize(idx+1)` grows the CVT unboundedly -> OOM/alloc.

#[test]
fn wcvtp_oob_index_does_not_grow_cvt() {
    let mut itp = interp();
    itp.cvt = vec![0i32; 4]; // a 4-entry CVT

    // PUSHW[1] 20000 0 ; WCVTP  (idx=20000 popped under val=0; 20000 >= 4 => OOB)
    let prog = [0xB9, 0x4E, 0x20, 0x00, 0x00, 0x44];
    let _ = run(&mut itp, &[(0, 0)], &prog); // ignore/err both acceptable

    assert_eq!(
        itp.cvt.len(),
        4,
        "WCVTP with an out-of-range index must not resize/alloc the CVT"
    );
}

// ── Test 5 — per-glyph graphics-state reset (gates bug #5) ───────────
//
// FreeType TT_Run_Context resets GS.round_state = RTG before every glyph, so
// a round mode selected in `prep` must NOT leak into the glyph program.

#[test]
fn glyph_program_resets_round_state_to_rtg() {
    let mut itp = interp();
    // Stand in for prep having run RDTG (round-down): set the per-size default.
    itp.default_gs.round_state = RoundState::DownToGrid;

    // MDAP[1] rounds point 0 (x = 48 = 0.75px) to grid.
    //   RTG(48)  = (48+32) & !63 = 80 & !63 = 64   (correct)
    //   RDTG(48) = 48 & !63      = 0               (if prep's mode leaks)
    let prog = [0x01 /*SVTCA[x]*/, 0xB0, 0x00, 0x2F /*MDAP[1]*/];
    run(&mut itp, &[(48, 0)], &prog).unwrap();

    assert_eq!(
        cur(&itp, 0).0,
        64,
        "glyph must round with RTG (state reset), not prep's leaked RDTG"
    );
}

// ── Test 6 — move_point when freedom . projection == 0 (gates bug #6) ─
//
// proj = x-axis, freedom = y-axis => F.P == 0.  FreeType clamps F_dot_P to a
// nonzero minimum and STILL moves the point along freedom + marks it touched
// (Direct_Move).  The buggy impl returns early, dropping the move and touch.
// The exact displacement depends on the clamp constant, so we assert only
// that the point moved and is touched (both are dropped by the bug).

#[test]
fn move_point_survives_zero_fdotp() {
    let mut itp = interp();
    // SVTCA[x] (proj+free=x), SFVTCA[y] (free=y only) => F.P = 0.
    // MDAP[1] on (48,10): proj=x => cur_dist=48, RTG(48)=64, distance=+16.
    let prog = [0x01, 0x04, 0xB0, 0x00, 0x2F];
    run(&mut itp, &[(48, 10)], &prog).unwrap();

    let (_tx, ty) = touched(&itp, 0);
    assert!(ty, "point must be TOUCHED_Y even when F.P==0");
    assert_ne!(
        cur(&itp, 0).1,
        10,
        "point Y must move (nonzero displacement), not be dropped"
    );
}

// ── Test 7 — SVTCA + MDAP[1] grid math (per axis) ────────────────────
//
// MDAP[1] rounds the point's projected coordinate to grid and touches it.
// RTG(100) = (100+32) & !63 = 132 & !63 = 128.  The off-axis coord is left
// untouched, proving the projection/freedom vectors are honoured per axis.

#[test]
fn svtca_mdap_rounds_per_axis() {
    // SVTCA[x]; PUSHB[0] 0; MDAP[1]  -> x rounds, y fixed.
    let mut itp = interp();
    run(&mut itp, &[(100, 100)], &[0x01, 0xB0, 0x00, 0x2F]).unwrap();
    assert_eq!(cur(&itp, 0), (128, 100), "MDAP[1] on x-axis rounds x only");

    // SVTCA[y]; PUSHB[0] 0; MDAP[1]  -> y rounds, x fixed.
    let mut itp = interp();
    run(&mut itp, &[(100, 100)], &[0x00, 0xB0, 0x00, 0x2F]).unwrap();
    assert_eq!(cur(&itp, 0), (100, 128), "MDAP[1] on y-axis rounds y only");
}

// ── Test 8 — MDRP minimum distance, sign preserved ───────────────────
//
// MDRP[round+min] (0xCC): measure original rp0->p distance, round RTG, then
// clamp its magnitude up to minimum_distance (default 1px = 64) keeping sign.
//   |orig| = 26 (0.40625px): RTG(26) = (26+32)&!63 = 0  ->  bumped to +/-64.

#[test]
fn mdrp_enforces_minimum_distance_with_sign() {
    for &(orig_x, expect) in &[(26i32, 64i32), (-26, -64)] {
        let mut itp = interp();
        // PUSHB[0] 0; SRP0; PUSHB[0] 1; MDRP[round+min]
        let prog = [0xB0, 0x00, 0x10, 0xB0, 0x01, 0xCC];
        run(&mut itp, &[(0, 0), (orig_x, 0)], &prog).unwrap();
        assert_eq!(
            cur(&itp, 1).0,
            expect,
            "orig {} must clamp to min-dist {}",
            orig_x,
            expect
        );
    }
}

// ── Test 9 — MIRP control-value cut-in (gates bug #9) ────────────────
//
// default control_value_cut_in = 68 (17/16 px).  MIRP[round] = 0xE4.
// rp0 = point 0 at x=0; point 1 original distance = 90 (1.40625px).
//   Case A: cvt = 64 (1px).  |64-90| = 26 < 68  => keep CVT; RTG(64)=64.
//   Case B: cvt = 192 (3px). |192-90| = 102 >= 68 => use original; RTG(90)=64.
// Both land at 64; case B is the discriminator (a no-cut-in impl gives 192).

#[test]
fn mirp_control_value_cut_in() {
    // PUSHB[0] 0; SRP0; PUSHB[1] 1 0; MIRP[round]   (pops cvt_idx=0 then p=1)
    let prog = [0xB0, 0x00, 0x10, 0xB1, 0x01, 0x00, 0xE4];

    let mut a = interp();
    a.cvt = vec![64i32];
    run(&mut a, &[(0, 0), (90, 0)], &prog).unwrap();
    assert_eq!(cur(&a, 1).0, 64, "diff < cut-in: snap to rounded CVT (1px)");

    let mut b = interp();
    b.cvt = vec![192i32];
    run(&mut b, &[(0, 0), (90, 0)], &prog).unwrap();
    assert_eq!(
        cur(&b, 1).0,
        64,
        "diff >= cut-in: use rounded original (1px), not the 3px CVT"
    );
}

// ── Test 10 — IP proportional interpolation (+ degenerate) ───────────
//
// IP scales a point's rp1->p distance by (cur_range / orig_range) where the
// ranges span rp1..rp2.  SCFS is used to displace a reference beforehand.

#[test]
fn ip_interpolates_proportionally() {
    // rp1 = pt0 (orig 0, cur 0), rp2 = pt1 (orig 10, cur moved to 12),
    // pt2 orig 5 -> new_dist = MulDiv(cur_range 12, orig_dist 5, orig_range 10)
    //            = (60 + 5)/10 = 6  ->  pt2.x = 6.
    let mut itp = interp();
    let prog = [
        0xB1, 0x01, 0x0C, 0x48, // PUSHB[1] 1 12 ; SCFS  -> pt1.x = 12
        0xB0, 0x00, 0x11, // PUSHB[0] 0 ; SRP1 0
        0xB0, 0x01, 0x12, // PUSHB[0] 1 ; SRP2 1
        0xB0, 0x02, 0x39, // PUSHB[0] 2 ; IP
    ];
    run(&mut itp, &[(0, 0), (10, 0), (5, 0)], &prog).unwrap();
    assert_eq!(cur(&itp, 2).0, 6, "IP maps orig 5 -> cur 6.0");
}

#[test]
fn ip_degenerate_range_shifts_by_rp1_delta() {
    // rp1 = pt0 (orig 5, moved to cur 8 => delta +3), rp2 = pt1 (orig 5) so
    // orig_range = 0.  IP then keeps the original distance and the point ends
    // up shifted by rp1's delta:  pt2 orig 20 -> 20 + 3 = 23.
    let mut itp = interp();
    let prog = [
        0xB1, 0x00, 0x08, 0x48, // PUSHB[1] 0 8 ; SCFS -> pt0.x = 8
        0xB0, 0x00, 0x11, // SRP1 0
        0xB0, 0x01, 0x12, // SRP2 1
        0xB0, 0x02, 0x39, // IP pt2
    ];
    run(&mut itp, &[(5, 0), (5, 0), (20, 0)], &prog).unwrap();
    assert_eq!(cur(&itp, 2).0, 23, "degenerate IP shifts pt by rp1 delta (+3)");
}

// ── Test 11 — IUP[x]: interpolate, edge-shift, per-axis flags (bug #11) ─
//
// One contour, 4 points.  Touch (via SCFS along x) pt0 -> cur 8 (delta +8)
// and pt2 -> cur 40 (delta +20).  Then IUP[x]:
//   pt1 (orus 10, between 0 and 20): interpolate halfway between cur 8 and 40
//        => 24.
//   pt3 (orus 30, OUTSIDE the [0,20] span): shift by the nearest edge (pt2)
//        delta +20 => 30 + 20 = 50.
// IUP touches nothing and works per axis: pt0 is TOUCHED_X but not TOUCHED_Y;
// interpolated pt1 stays untouched.

#[test]
fn iup_x_interpolates_and_edge_shifts() {
    let mut itp = interp();
    let prog = [
        0x01, // SVTCA[x]
        0xB1, 0x00, 0x08, 0x48, // PUSHB[1] 0 8 ; SCFS  -> pt0.x = 8
        0xB1, 0x02, 0x28, 0x48, // PUSHB[1] 2 40; SCFS  -> pt2.x = 40
        0x31, // IUP[x]
    ];
    // run() makes one contour ending at the last point (3).
    run(&mut itp, &[(0, 0), (10, 0), (20, 0), (30, 0)], &prog).unwrap();

    assert_eq!(cur(&itp, 1).0, 24, "pt1 interpolated between touched neighbours");
    assert_eq!(cur(&itp, 3).0, 50, "pt3 (outside span) shifted by nearest edge delta");
    assert_eq!(touched(&itp, 0), (true, false), "touch flags are per-axis");
    assert_eq!(touched(&itp, 1).0, false, "IUP must not touch interpolated points");
}

// ── Test 12 — FLIPRGON/FLIPRGOFF bounded (gates bug #12) ─────────────
//
// FLIPRGOFF/ON toggle ON_CURVE over lo..=hi.  An out-of-range hi must be
// bounded (no unbounded loop / no zone growth / no panic).  A 2^31 range is
// intentionally NOT fed — it would hang the current unchecked loop; the fixer
// must clamp the range to the zone point count.

#[test]
fn fliprg_is_bounded_and_toggles_correct_points() {
    let mut itp = interp();
    let prog = [
        0xB1, 0x01, 0x03, 0x82, // PUSHB[1] 1 3 ; FLIPRGOFF (lo=1..=hi=3)
        0xB9, 0x00, 0x04, 0x03, 0xE8, 0x81, // PUSHW[1] 4 1000 ; FLIPRGON (4..=1000)
    ];
    let r = run(&mut itp, &[(0, 0); 6], &prog);
    assert!(r.is_ok(), "FLIPRG must not error/panic on an out-of-range hi");
    assert_eq!(itp.zones[1].flags.len(), 6, "FLIPRG must not grow the zone");

    let on = |i: usize| itp.zones[1].flags[i].contains(PointFlags::ON_CURVE);
    assert!(
        on(0) && !on(1) && !on(2) && !on(3),
        "FLIPRGOFF cleared ON_CURVE for 1..=3"
    );
    assert!(on(4) && on(5), "in-range FLIPRGON points stay on-curve");
}

// ── Test 13 — phantom pp1.x = (xMin - lsb) scaled (documents bug #13) ─
//
// FreeType sets phantom point pp1.x = (bbox.xMin - lsb) scaled to F26Dot6,
// NOT a hardcoded 0.  `HintInstance::hint_glyph_full` currently hardcodes
// `phantom[0].x = 0` (hinting.rs, "phantom[0]: origin") and takes no
// xMin/lsb, so a non-zero pp1.x is not expressible through that public path.
// This pins the scaling arithmetic (the crate's own primitive) the fixer must
// apply once xMin/lsb are threaded through.  (Cannot fail via today's API.)

#[test]
fn phantom_pp1_x_is_xmin_minus_lsb_scaled() {
    // upem=1000, ppem=16 -> scale = ((16<<22) + 500)/1000 = 67109 (16.16)
    let scale = compute_scale(16, 1000);
    assert_eq!(scale, 67109);
    // xMin - lsb = 50 - 30 = 20 funits
    // pp1.x = FT_MulFix(20, 67109) = (20*67109 + 0x8000) >> 16
    //       = (1342180 + 32768) >> 16 = 1374948 >> 16 = 20
    let pp1_x = F26Dot6::from_funits(50 - 30, scale).to_bits();
    assert_eq!(pp1_x, 20, "pp1.x = (xMin - lsb) scaled");
    assert_ne!(pp1_x, 0, "pp1.x must not be hardcoded 0 when lsb != xMin");
}

// ── Test 14 — stack headroom under a small maxStackElements (bug #14) ─
//
// FreeType allocates stack headroom beyond maxp.maxStackElements, so a glyph
// that transiently uses a few extra slots still hints.  The buggy impl errors
// StackOverflow and (callers swallow errors) drops ALL hinting.

#[test]
fn small_max_stack_keeps_headroom() {
    // max_stack declared = 4; program peak depth = 6.
    let mut itp = Interpreter::new(4, 32, 16, 4, 32, 1000);
    let prog = [
        0x01, // SVTCA[x]
        0xB5, 0, 0, 0, 0, 0, 0, // PUSHB[5]: 6 bytes -> peak stack depth 6
        0x2F, 0x2F, 0x2F, 0x2F, 0x2F, 0x2F, // MDAP[1] x6 (round point 0)
    ];
    let r = run(&mut itp, &[(100, 0)], &prog);
    assert!(r.is_ok(), "a few extra stack slots must not drop all hinting");
    // RTG(100) = (100+32)&!63 = 128 (further MDAPs are idempotent).
    assert_eq!(cur(&itp, 0).0, 128, "point must still be grid-rounded");
}

// ── Test 15 — error guards return Err, never panic (bug #15) ─────────

#[test]
fn stack_underflow_returns_err_not_panic() {
    let mut itp = interp();
    // SRP0 with an empty stack -> pop() underflow.
    let r = run(&mut itp, &[(0, 0)], &[0x10]);
    assert!(r.is_err(), "stack underflow must return Err, not panic");
}

#[test]
fn cvt_index_out_of_bounds_returns_err_not_panic() {
    let mut itp = interp();
    itp.cvt = vec![0i32; 2];
    // PUSHB[0] 200 ; RCVT  -> read_cvt(200) is out of bounds.
    let r = run(&mut itp, &[(0, 0)], &[0xB0, 0xC8, 0x45]);
    assert!(r.is_err(), "CVT index OOB must return Err, not panic");
}

