//! C14, the half that needs a counting allocator: a limit holds BEFORE anything is allocated for a
//! claim (planning/architecture.md section 1.4 rule 4: "Everything has a limit, stated up front"; the
//! rule of the source gates: a size read from bytes other people wrote reaches an allocation only after
//! it has been compared with something the reader holds).
//!
//! The test binary installs an allocator that records, per thread, the largest single allocation asked
//! for. The inputs are at most a kilobyte, and claim sizes of up to 4 GiB: a header that claims a body
//! near 2^32, a count above the cap, and an AUTHENTIC trailer (sealed by the test's independent
//! reference) that declares 2^31, 2^32 - 1 and 2^64 - 1 lines. No call may ask for more than 64 KiB at
//! once. The control at the end allocates 3 MB on purpose and must be seen, so the counter is known to
//! count. The other half of C14, the typed refusals, is in `pack_read.rs`.
//!
//! This file holds the one `unsafe impl` of the lane's tests (a `GlobalAlloc` cannot be written without
//! it); the library has none (`#![forbid(unsafe_code)]`).

mod pack_common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use azlin_content::pack::{
    check_header, check_new_pack, read_trailer, rebuild_entries, PackError, PackHeader, Padding,
    MAX_CHUNKS,
};
use azlin_content::{PackClass, PackLen};
use pack_common::{
    build, cbor_head, chunk, k_chunk, ref_seal, seal_key, small_geometry, space, RefParams,
};

thread_local! {
    /// The largest single allocation this thread has asked for since the last `measure`.
    static LARGEST: Cell<usize> = const { Cell::new(0) };
}

fn note(size: usize) {
    // A constant thread-local without a destructor is always there; if it is not (a thread that is
    // ending), the allocation is simply not counted.
    let _ = LARGEST.try_with(|largest| largest.set(largest.get().max(size)));
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        System.dealloc(ptr, layout)
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note(layout.size());
        System.alloc_zeroed(layout)
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note(new_size);
        System.realloc(ptr, layout, new_size)
    }
}

#[global_allocator]
static COUNTING: Counting = Counting;

/// Runs `f`; the result and the largest single allocation it asked for.
fn measure<T>(f: impl FnOnce() -> T) -> (T, usize) {
    LARGEST.with(|largest| largest.set(0));
    let out = f();
    (out, LARGEST.with(Cell::get))
}

/// What no call on an input of a kilobyte has a reason to ask for at once.
const LIMIT: usize = 64 * 1024;

#[test]
fn c14_nothing_is_allocated_for_a_claim() {
    let sp = space(51);
    let k = k_chunk(52);
    let key = seal_key(3, 0x44);

    // A real pack, as the frame of a kilobyte of input.
    let (id, blob) = chunk(&sp, &k, 1, 60);
    let pack = build(
        &sp,
        PackClass::Content,
        3,
        0x44,
        small_geometry(),
        Padding::None,
        true,
        &[(id, blob)],
        1,
    );
    let mut kib = pack.bytes().to_vec();
    kib.resize(1024, 0);
    let total = PackLen::new(1024);

    // 1. A header that claims a body near 2^32; a count above the cap; the cap on a short trailer.
    let mut claims_body = kib.clone();
    claims_body[16..20].copy_from_slice(&(u32::MAX - 7).to_le_bytes());
    let mut claims_count = kib.clone();
    claims_count[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
    let mut claims_cap = kib.clone();
    claims_cap[12..16].copy_from_slice(&MAX_CHUNKS.to_le_bytes());
    let mut claims_padding = kib.clone();
    claims_padding[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
    for (what, bytes, want) in [
        (
            "a body of 2^32 - 8 bytes",
            &claims_body,
            PackError::SizesInconsistent,
        ),
        ("2^32 - 1 chunks", &claims_count, PackError::TooManyChunks),
        (
            "16,384 chunks and a trailer for none",
            &claims_cap,
            PackError::SizesInconsistent,
        ),
        (
            "2^32 - 1 bytes of padding",
            &claims_padding,
            PackError::SizesInconsistent,
        ),
    ] {
        let (parsed, largest) = measure(|| PackHeader::parse(bytes, total));
        assert_eq!(parsed, Err(want), "{what}");
        assert_eq!(
            largest, 0,
            "{what}: parsing a header allocates nothing ({largest} bytes at once)"
        );
        let (checked, largest) = measure(|| check_header(bytes, total));
        assert!(checked.is_err(), "{what}");
        assert_eq!(
            largest, 0,
            "{what}: the node's header check allocates nothing"
        );
        let (new_pack, largest) = measure(|| check_new_pack(bytes, total));
        assert!(new_pack.is_err(), "{what}");
        assert_eq!(largest, 0, "{what}: the node's check allocates nothing");
        let (rebuilt, largest) = measure(|| rebuild_entries(bytes, &sp, &key));
        assert_eq!(rebuilt, Err(want), "{what}");
        assert!(
            largest < LIMIT,
            "{what}: rebuild_entries asked for {largest} bytes at once"
        );
    }
    // `read_trailer` under a header made by hand that claims the most lines the cap allows and a body
    // that could hold them, on a trailer of 100 bytes.
    let mut by_hand = PackHeader::parse(pack.bytes(), pack.len()).expect("a header");
    by_hand.chunk_count = MAX_CHUNKS;
    by_hand.body_len = PackLen::new(1_000_000);
    let (read, largest) = measure(|| read_trailer(&by_hand, &kib[..100], &sp, &key));
    assert_eq!(read, Err(PackError::SizesInconsistent));
    assert!(
        largest < LIMIT,
        "read_trailer asked for {largest} bytes at once"
    );

    // 2. An AUTHENTIC trailer that declares a number of lines the pack cannot hold.
    let (_, body_blob) = chunk(&sp, &k, 2, 60);
    let body = body_blob.as_bytes().to_vec();
    let params = RefParams {
        space: [51; 16],
        epoch: 3,
        k_seal: [0x44; 32],
        class: 0,
        pad_len: 0,
        nonce: [3; 24],
    };
    let hash = *blake3::hash(&body).as_bytes();
    let claims: [(&str, Vec<u8>); 5] = [
        ("2^31 lines", vec![0x9a, 0x80, 0, 0, 0]),
        ("2^32 - 1 lines", vec![0x9a, 0xff, 0xff, 0xff, 0xff]),
        (
            "2^64 - 1 lines",
            vec![0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
        ),
        ("one line past the cap", vec![0x99, 0x40, 0x01]),
        ("5 lines and only a few items", vec![0x85]),
    ];
    for (what, claim) in claims {
        let mut plain = vec![0xa3, 0x01];
        plain.extend_from_slice(&claim);
        plain.extend_from_slice(&[0x02, 0x58, 0x20]);
        plain.extend_from_slice(&hash);
        plain.push(0x09);
        // A filler byte string brings the plaintext to 63 bytes: a trailer of 103, which fits the header's one line.
        let mut n = 0usize;
        loop {
            let head = cbor_head(2, n as u64);
            if plain.len() + head.len() + n == 63 {
                plain.extend_from_slice(&head);
                plain.extend(std::iter::repeat_n(0u8, n));
                break;
            }
            assert!(
                plain.len() + head.len() + n < 63,
                "{what}: the claim is too long to fill to 63 bytes"
            );
            n += 1;
        }
        let made = ref_seal(&params, 1, &body, &plain);
        assert!(made.bytes.len() < 300);
        let (rebuilt, largest) = measure(|| rebuild_entries(&made.bytes, &sp, &key));
        assert_eq!(
            rebuilt,
            Err(PackError::BadTrailer),
            "an authentic trailer that claims {what}"
        );
        assert!(
            largest < LIMIT,
            "an authentic trailer that claims {what}: {largest} bytes asked for at once"
        );
    }

    // 3. The control: the counter does count.
    let (_, largest) = measure(|| std::hint::black_box(vec![1u8; 3_000_000]));
    assert!(
        largest >= 3_000_000,
        "the counter saw {largest} bytes for an allocation of 3,000,000"
    );
}
