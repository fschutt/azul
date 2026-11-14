//! C8 and C7, the part that needs a measurement: a hostile pointer and a hostile chunk-list claim are
//! refused BEFORE anything is allocated for what they claim. A counting allocator (per thread, so the
//! tests of this file do not disturb one another) measures the peak of live bytes during one call.

mod pointer_common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use azlin_content::limits::MAX_FILE_CHUNKS;
use azlin_content::pointer::{decode_chunk_list, ChunkListRef, PointerError, PointerV2};
use azlin_content::FileSize;
use pointer_common::*;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

fn note_alloc(n: usize) {
    LIVE.with(|live| {
        let now = live.get().saturating_add(n);
        live.set(now);
        PEAK.with(|peak| {
            if now > peak.get() {
                peak.set(now);
            }
        });
    });
}

fn note_free(n: usize) {
    LIVE.with(|live| live.set(live.get().saturating_sub(n)));
}

// SAFETY: every call is forwarded to the system allocator unchanged; the counters are thread-local
// cells with a constant initialiser, so counting allocates nothing.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc(layout);
        if !p.is_null() {
            note_alloc(layout.size());
        }
        p
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = System.alloc_zeroed(layout);
        if !p.is_null() {
            note_alloc(layout.size());
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout);
        note_free(layout.size());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let p = System.realloc(ptr, layout, new_size);
        if !p.is_null() {
            note_free(layout.size());
            note_alloc(new_size);
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// The result of `f` and the most bytes it held live at once beyond what was live when it started.
fn peak_of<R>(f: impl FnOnce() -> R) -> (R, usize) {
    let base = LIVE.with(Cell::get);
    PEAK.with(|peak| peak.set(base));
    let result = f();
    let peak = PEAK.with(Cell::get);
    (result, peak.saturating_sub(base))
}

#[test]
fn c8_a_claim_of_a_million_previews_allocates_nothing_for_them() {
    // {1: 2, 9: an array that claims a million elements}: the codec's item limit refuses the claim.
    let hostile = from_hex("a20102099a000f4240");
    let (result, peak) = peak_of(|| PointerV2::from_cbor(&hostile, &OBJECT));
    assert!(matches!(result, Err(PointerError::Cbor(_))));
    assert!(peak < 8 * 1024, "the refusal allocated {peak} bytes");

    // {1: 2, 5: a byte string that claims 4 GiB}: refused by the length against the bytes that are left.
    let hostile = from_hex("a20102055affffffff");
    let (result, peak) = peak_of(|| PointerV2::from_cbor(&hostile, &OBJECT));
    assert!(result.is_err());
    assert!(peak < 8 * 1024, "the refusal allocated {peak} bytes");
}

#[test]
fn c7_a_claim_of_a_huge_chunk_list_allocates_nothing_for_it() {
    let sealer = TestSealer::new();
    let refs = huge_chunks(1025);
    let (sealed, list) =
        azlin_content::pointer::encode_chunk_list(&refs, &space(), &sealer).expect("encodes");
    drop(refs);

    // The declared count is above the cap, or the object is not the one named: refused on the numbers and
    // the hash, before the sealer is asked and before a single chunk reference is made.
    for (label, claim) in [
        (
            "more than the cap",
            ChunkListRef {
                count: MAX_FILE_CHUNKS + 1,
                ..list
            },
        ),
        (
            "the cap, another object",
            ChunkListRef {
                name: [0xAB; 32],
                count: MAX_FILE_CHUNKS,
                total_raw: FileSize::new(u64::from(MAX_FILE_CHUNKS) * 4 * 1024 * 1024),
            },
        ),
        (
            "u32::MAX",
            ChunkListRef {
                count: u32::MAX,
                ..list
            },
        ),
    ] {
        let (result, peak) = peak_of(|| decode_chunk_list(&sealed, &claim, &space(), &sealer));
        assert!(result.is_err(), "{label}");
        assert!(
            peak < 4 * 1024,
            "{label}: the refusal allocated {peak} bytes"
        );
    }
    assert_eq!(sealer.opens.get(), 0, "the sealer was never asked");

    // For comparison, the honest object costs memory in proportion to its chunks.
    let (result, peak) = peak_of(|| decode_chunk_list(&sealed, &list, &space(), &sealer));
    assert_eq!(result.expect("decodes").len(), 1025);
    assert!(
        peak > 64 * 1024,
        "the honest list allocated only {peak} bytes"
    );
}
