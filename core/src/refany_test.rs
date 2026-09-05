#[allow(unused_imports)]
pub use super::*;
#[cfg(test)]
#[allow(
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    trivial_casts,
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::unused_self,
    unused_qualifications,
    unreachable_pub,
    private_interfaces
)] // pedantic lints are noise in unsafe-exercising test code
mod audit_tests {
    use super::*;
    use core::sync::atomic::{AtomicUsize, Ordering};

    static DROP_COUNT: AtomicUsize = AtomicUsize::new(0);

    // The tests below share the single `DROP_COUNT` static: each resets it to 0
    // and then asserts an exact drop count. Under the default multi-threaded
    // test runner they would otherwise interleave and corrupt each other's
    // counts (a real, if test-only, isolation bug). Every `DROP_COUNT`-using
    // test takes this lock first to serialize; it is poison-tolerant so one
    // failing test does not cascade `.unwrap()` panics into the rest.
    static DROP_COUNT_SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
    fn serialize_drop_count() -> std::sync::MutexGuard<'static, ()> {
        DROP_COUNT_SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    struct DropCounter(#[allow(dead_code)] u32);
    impl Drop for DropCounter {
        fn drop(&mut self) {
            DROP_COUNT.fetch_add(1, Ordering::SeqCst);
        }
    }

    // AUDIT: exclusive borrow must be denied while a shared borrow is live and
    // vice-versa (runtime borrow checker), and must be recoverable after the
    // guard drops. Exercises the atomic acquire/release added to downcast_*.
    #[test]
    fn borrow_exclusion_and_recovery() {
        // The runtime borrow guard lives in the *shared* refcount inner, so it
        // is only observable across two clones (a single `RefAny` can't hold two
        // guards at once — the methods take `&mut self`). `b` shares `a`'s inner.
        let mut a = RefAny::new(7i32);
        let mut b = a.clone();

        {
            let r = a.downcast_ref::<i32>().unwrap();
            assert_eq!(*r, 7);
            // shared borrow live -> no mutable borrow via the shared inner
            assert!(b.downcast_mut::<i32>().is_none());
            // another shared borrow is fine
            assert!(b.downcast_ref::<i32>().is_some());
        }

        {
            let mut m = a.downcast_mut::<i32>().unwrap();
            *m = 42;
            // mutable borrow live -> no shared borrow via the shared inner
            assert!(b.downcast_ref::<i32>().is_none());
        }

        assert_eq!(*a.downcast_ref::<i32>().unwrap(), 42);
    }

    // AUDIT: wrong-type downcast must be rejected. Same type -> same id.
    #[test]
    fn type_id_guard() {
        let mut a = RefAny::new(1u64);
        assert!(a.downcast_ref::<i32>().is_none());
        assert!(a.downcast_ref::<u64>().is_some());

        assert_eq!(
            RefAny::get_type_id_static::<u64>(),
            RefAny::get_type_id_static::<u64>()
        );
        assert_ne!(
            RefAny::get_type_id_static::<u64>(),
            RefAny::get_type_id_static::<i64>()
        );
    }

    // AUDIT: replace_contents must run each stored value's destructor exactly
    // once (old value on replace, new value on final drop) and must not leak.
    #[test]
    fn replace_contents_drops_exactly_once() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let mut a = RefAny::new(DropCounter(1));
            let b = RefAny::new(DropCounter(2));
            assert!(a.replace_contents(b));
            // The original `a` value was dropped during replacement.
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
            // `a` now holds the (copied) `b` value; dropped at end of scope.
        }
        // Two DropCounter values were constructed; both must be dropped once.
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    }

    // AUDIT: replace_contents must fail (return false) while a borrow is live.
    #[test]
    fn replace_contents_denied_while_borrowed() {
        let mut a = RefAny::new(1i32);
        // Clone first: `r` will exclusively borrow `a`, so the sibling clone
        // must exist beforehand. Both share the same inner RefCountInner.
        let mut a2 = a.clone();
        let r = a.downcast_ref::<i32>().unwrap();
        // A live shared borrow (num_refs != 0) on the shared inner must block
        // replace_contents via the sibling clone.
        assert!(!a2.replace_contents(RefAny::new(2i32)));
        drop(r);
        assert!(a2.replace_contents(RefAny::new(2i32)));
    }

    // ---- Miri-focused unit tests -------------------------------------------
    // These exercise the pure-Rust memory behavior of each unsafe path so Miri
    // can detect UB (bad provenance, misalignment, use-after-free, leaks,
    // refcount corruption). No FFI, no threads, no OS calls; tiny allocations.

    // MIRI: covers RefAny::new + new_c alloc/copy_nonoverlapping + downcast_ref
    // (&*(ptr as *const U)) + the final Drop path (Box::from_raw + dealloc +
    // custom destructor). A non-Copy heap type checks the destructor runs.
    #[test]
    fn miri_new_downcast_drop_roundtrip() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let mut a = RefAny::new(DropCounter(9));
            // downcast_ref exercises the type-id guard + aligned pointer cast.
            assert!(a.downcast_ref::<DropCounter>().is_some());
            assert!(a.downcast_ref::<u8>().is_none());
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    }

    // MIRI: alignment correctness of new_c's Layout::from_size_align path. An
    // over-aligned payload downcast to a misaligned pointer would be UB.
    #[test]
    fn miri_alignment_preserved() {
        #[repr(align(16))]
        #[derive(Debug)]
        struct Over(u64);
        let mut a = RefAny::new(Over(0xABCD));
        let r = a.downcast_ref::<Over>().unwrap();
        assert_eq!(r.0, 0xABCD);
        assert_eq!((&raw const *r) as usize % 16, 0);
    }

    // MIRI: clone shares one RefCountInner; num_copies increments on clone and
    // decrements on drop (RefCount::clone / RefCount::drop fetch paths). Data
    // must survive while any clone lives and be freed exactly once at the end.
    #[test]
    fn miri_clone_refcount_increment_decrement() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let a = RefAny::new(DropCounter(1));
            assert_eq!(a.get_ref_count(), 1);
            let b = a.clone();
            assert_eq!(a.get_ref_count(), 2);
            assert_eq!(b.get_ref_count(), 2);
            {
                let c = b.clone();
                assert_eq!(c.get_ref_count(), 3);
            }
            // c dropped -> back to 2, nothing freed yet.
            assert_eq!(a.get_ref_count(), 2);
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 0);
        }
        // all clones dropped -> data destructed exactly once.
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    }

    // MIRI: downcast_mut hands out &mut *(ptr as *mut U); mutation must be
    // visible through a shared clone (shared RefCountInner data pointer).
    #[test]
    fn miri_downcast_mut_mutation_visible_across_clones() {
        let mut a = RefAny::new(10u32);
        let mut b = a.clone();
        {
            let mut m = a.downcast_mut::<u32>().unwrap();
            *m += 5;
        }
        assert_eq!(*b.downcast_ref::<u32>().unwrap(), 15);
    }

    // MIRI: the runtime borrow refcount on the shared inner. Exercises
    // increase_ref/decrease_ref/increase_refmut/decrease_refmut and the
    // can_be_shared / can_be_shared_mut predicates directly, plus the
    // checked_sub underflow guard (decrement at zero must saturate, not wrap).
    #[test]
    fn miri_borrow_counter_transitions_and_underflow_guard() {
        let a = RefAny::new(0i32);
        let rc = &a.sharing_info;

        assert!(rc.can_be_shared());
        assert!(rc.can_be_shared_mut());

        rc.increase_ref();
        assert!(rc.can_be_shared()); // shared borrows coexist
        assert!(!rc.can_be_shared_mut()); // but block a mutable borrow
        rc.decrease_ref();
        assert!(rc.can_be_shared_mut());

        rc.increase_refmut();
        assert!(!rc.can_be_shared()); // mutable borrow blocks shared
        assert!(!rc.can_be_shared_mut());
        rc.decrease_refmut();
        assert!(rc.can_be_shared_mut());

        // Underflow guard: extra decrements must saturate at 0, never wrap to
        // usize::MAX (which would permanently break the borrow checker).
        rc.decrease_ref();
        rc.decrease_refmut();
        assert!(rc.can_be_shared());
        assert!(rc.can_be_shared_mut());
    }

    // MIRI: get_type_id_static reads TypeId via from_raw_parts and folds ALL
    // bytes. Same type -> same id (stable within a run); distinct types differ.
    #[test]
    fn miri_type_id_static_stable_and_distinct() {
        assert_eq!(
            RefAny::get_type_id_static::<(u8, u64)>(),
            RefAny::get_type_id_static::<(u8, u64)>()
        );
        assert_ne!(
            RefAny::get_type_id_static::<u32>(),
            RefAny::get_type_id_static::<[u32; 2]>()
        );
    }

    // MIRI: ZST payload uses a null data pointer but must still construct, clone,
    // run its destructor exactly once, and downcast (a ZST reference reads no
    // bytes, so a dangling-but-aligned pointer is a valid reference).
    #[test]
    fn miri_zst_roundtrip_and_destructor() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        struct ZstDrop;
        impl Drop for ZstDrop {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::SeqCst);
            }
        }
        {
            let mut a = RefAny::new(ZstDrop);
            assert_eq!(a.get_data_len(), 0);
            // downcast_ref succeeds for a ZST (dangling ref, no bytes read); the
            // returned guard drops here without running the value's destructor.
            assert!(a.downcast_ref::<ZstDrop>().is_some());
            let _b = a.clone();
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1);
    }

    // MIRI: replace_contents alloc/dealloc/copy path plus the neutralized
    // new_value destructor. Old value destructed once, new value destructed
    // once at final drop, with no leak/double-free of either heap block.
    #[test]
    fn miri_replace_contents_alloc_paths() {
        let _serial = serialize_drop_count();
        DROP_COUNT.store(0, Ordering::SeqCst);
        {
            let mut a = RefAny::new(DropCounter(1));
            assert!(a.replace_contents(RefAny::new(DropCounter(2))));
            assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 1); // old value gone
            assert_eq!(a.downcast_ref::<DropCounter>().unwrap().0, 2u32);
        }
        assert_eq!(DROP_COUNT.load(Ordering::SeqCst), 2);
    }

    // MIRI: replacing across differing sizes/alignments (u8 -> u64) reallocates
    // correctly and keeps the shared pointer aligned for the new type.
    #[test]
    fn miri_replace_contents_changes_layout() {
        let mut a = RefAny::new(7u8);
        assert!(a.replace_contents(RefAny::new(0x1122_3344_5566_7788u64)));
        {
            // downcast_ref takes &mut self, so scope the guard before the next call.
            let r = a.downcast_ref::<u64>().unwrap();
            assert_eq!(*r, 0x1122_3344_5566_7788u64);
            assert_eq!((&raw const *r) as usize % core::mem::align_of::<u64>(), 0);
        }
        // old u8 type must no longer downcast.
        assert!(a.downcast_ref::<u8>().is_none());
    }

    // MIRI: RefCount clone/drop in isolation keeps the inner alive until the
    // last handle drops (Box::into_raw / Box::from_raw balance).
    #[test]
    fn miri_refcount_clone_keeps_inner_alive() {
        let a = RefAny::new(5usize);
        let rc0 = a.sharing_info.clone(); // +1 copy
        let rc1 = rc0.clone(); // +1 copy
        assert_eq!(a.get_ref_count(), 3);
        drop(rc1);
        drop(rc0);
        assert_eq!(a.get_ref_count(), 1);
        // `a` still usable -> inner not freed.
        assert_eq!(*a.clone().downcast_ref::<usize>().unwrap(), 5);
    }
}

#[cfg(test)]
#[allow(
    clippy::items_after_statements,
    clippy::redundant_clone,
    clippy::needless_pass_by_value,
    clippy::needless_range_loop,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::unusual_byte_groupings,
    clippy::many_single_char_names,
    clippy::used_underscore_binding,
    clippy::borrow_as_ptr,
    clippy::cast_ptr_alignment,
    clippy::fn_to_numeric_cast_any,
    trivial_casts,
    unused_qualifications,
    unreachable_pub,
    private_interfaces,
    missing_debug_implementations,
    missing_copy_implementations
)] // pedantic lints are noise in unsafe-exercising test code
mod autotest_generated {
    use alloc::{string::String, vec::Vec};
    use core::{
        ffi::c_void,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    /// Destructor for payloads that need no drop glue (`Copy` types built via
    /// the raw C-ABI `new_c` path).
    extern "C" fn noop_destructor(_: *mut c_void) {}

    /// Store `value` in a `RefAny` and read it back out: the byte-copy into the
    /// heap allocation and the type-checked pointer cast must be lossless.
    fn round_trip<T: 'static + Clone + PartialEq + core::fmt::Debug>(value: T) {
        let mut a = RefAny::new(value.clone());
        let r = a
            .downcast_ref::<T>()
            .expect("downcast to the stored type must succeed");
        assert_eq!(*r, value);
    }

    // ---- RefAny::new_c — raw C-ABI constructor, malformed/boundary inputs ----

    // A NULL pointer with a non-zero length is the classic FFI mistake: copying
    // from it would be UB, so `new_c` must panic instead of reading it.
    #[test]
    #[should_panic(expected = "NULL pointer passed for non-ZST type")]
    fn new_c_null_ptr_with_nonzero_len_panics() {
        drop(RefAny::new_c(
            core::ptr::null(),
            4,
            4,
            RefAny::get_type_id_static::<u32>(),
            AzString::from_const_str("autotest::NullPtr"),
            noop_destructor,
            0,
            0,
        ));
    }

    // A non-power-of-two alignment cannot form a valid `Layout`; it must panic
    // before allocating rather than allocate with a bogus layout (which would
    // make the matching `dealloc` in `drop` UB).
    #[test]
    #[should_panic(expected = "Failed to create layout")]
    fn new_c_non_power_of_two_align_panics() {
        let value: u32 = 7;
        drop(RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            4,
            3, // not a power of two
            RefAny::get_type_id_static::<u32>(),
            AzString::from_const_str("autotest::BadAlign"),
            noop_destructor,
            0,
            0,
        ));
    }

    // `usize::MAX` bytes overflows `isize::MAX` and cannot be a `Layout`: the
    // checked constructor must reject it (no silent overflow into a tiny alloc).
    #[test]
    #[should_panic(expected = "Failed to create layout")]
    fn new_c_huge_len_panics_instead_of_overflowing() {
        let value: u8 = 1;
        drop(RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            usize::MAX,
            1,
            RefAny::get_type_id_static::<u8>(),
            AzString::from_const_str("autotest::HugeLen"),
            noop_destructor,
            0,
            0,
        ));
    }

    // len == 0 is the ZST path: NULL data pointer is legal, `align` is ignored
    // (even a nonsensical 0), nothing is allocated, and a ZST still downcasts
    // (via a dangling-but-aligned reference — there are no bytes to read).
    #[test]
    fn new_c_zero_len_null_ptr_is_a_clean_zst() {
        let mut a = RefAny::new_c(
            core::ptr::null(),
            0,
            0, // invalid alignment, but unused on the ZST path
            RefAny::get_type_id_static::<()>(),
            AzString::from_const_str("autotest::Zst"),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());
        assert!(a.is_type(RefAny::get_type_id_static::<()>()));
        // Type matches and a `&()`/`&mut ()` needs no backing bytes, so the
        // downcast succeeds; each temporary guard releases its borrow slot when it
        // drops at the end of its statement.
        assert!(a.downcast_ref::<()>().is_some());
        assert!(a.downcast_mut::<()>().is_some());
        assert!(a.sharing_info.can_be_shared_mut());
    }

    // Round-trip through the raw C-ABI constructor: what `new_c` encodes,
    // `downcast_ref` must decode bit-for-bit.
    #[test]
    fn new_c_round_trip_matches_rust_constructor() {
        let value: u64 = 0xDEAD_BEEF_CAFE_BABE;
        let mut a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            core::mem::size_of::<u64>(),
            core::mem::align_of::<u64>(),
            RefAny::get_type_id_static::<u64>(),
            AzString::from_const_str("u64"),
            noop_destructor,
            7,
            9,
        );
        assert_eq!(a.get_data_len(), core::mem::size_of::<u64>());
        assert_eq!(a.get_ref_count(), 1);
        assert_eq!(a.get_serialize_fn(), 7);
        assert_eq!(a.get_deserialize_fn(), 9);
        assert!(a.can_serialize());
        assert!(a.can_deserialize());
        assert_eq!(*a.downcast_ref::<u64>().unwrap(), value);
    }

    // The runtime guard is the type ID, nothing else: a matching size, name and
    // destructor must NOT be enough to downcast if the ID differs by one bit.
    #[test]
    fn new_c_wrong_type_id_rejects_downcast() {
        let value: u64 = 0x0102_0304_0506_0708;
        let real_id = RefAny::get_type_id_static::<u64>();
        let mut a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            core::mem::size_of::<u64>(),
            core::mem::align_of::<u64>(),
            real_id ^ 1, // one bit off
            AzString::from_const_str("u64"),
            noop_destructor,
            0,
            0,
        );
        assert!(!a.is_type(real_id));
        assert!(a.downcast_ref::<u64>().is_none());
        assert!(a.downcast_mut::<u64>().is_none());
        // The rejected downcasts must not have left a borrow behind.
        assert!(a.sharing_info.can_be_shared_mut());
    }

    // Over-alignment (align > len) is a valid `Layout`; the payload must land on
    // an address that satisfies the requested alignment.
    #[test]
    fn new_c_over_aligned_small_payload() {
        let value: u8 = 0x5A;
        let mut a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            1,
            16,
            RefAny::get_type_id_static::<u8>(),
            AzString::from_const_str("u8"),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(a.get_data_ptr() as usize % 16, 0);
        assert_eq!(*a.downcast_ref::<u8>().unwrap(), 0x5A);
    }

    // The type name is arbitrary caller-supplied UTF-8 (generated by foreign
    // codegen): empty, unicode, RTL overrides and embedded NULs must survive.
    #[test]
    fn new_c_preserves_unicode_and_empty_type_names() {
        let value: u32 = 0;
        let weird = "app::💥Ünïcødé<T>\u{202E}rtl\u{0}nul";
        let a = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            4,
            4,
            1,
            AzString::from(String::from(weird)),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(a.get_type_name().as_str(), weird);

        let b = RefAny::new_c(
            (&raw const value).cast::<c_void>(),
            4,
            4,
            2,
            AzString::from_const_str(""),
            noop_destructor,
            0,
            0,
        );
        assert_eq!(b.get_type_name().as_str(), "");
    }

    // ---- RefAny::new — post-construction invariants ----

    #[test]
    fn new_invariants_hold() {
        let mut a = RefAny::new(0x1122_3344u32);
        assert_eq!(a.get_data_len(), core::mem::size_of::<u32>());
        assert!(!a.get_data_ptr().is_null());
        assert_eq!(a.get_data_ptr() as usize % core::mem::align_of::<u32>(), 0);
        assert_eq!(a.get_type_id(), RefAny::get_type_id_static::<u32>());
        assert!(a.is_type(RefAny::get_type_id_static::<u32>()));
        assert_eq!(a.get_type_name().as_str(), "u32");
        assert_eq!(a.get_ref_count(), 1);
        assert!(a.has_no_copies());
        assert_eq!(a.get_serialize_fn(), 0);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert_eq!(a.get_update_fn(), 0);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());
        assert!(a.sharing_info.can_be_shared());
        assert!(a.sharing_info.can_be_shared_mut());
        assert_eq!(a.instance_id, 0);
        assert_eq!(*a.downcast_ref::<u32>().unwrap(), 0x1122_3344);
    }

    // A zero-length array of an 8-aligned element is still a ZST: `new` must take
    // the null-pointer path (no zero-size allocation, which would be UB).
    #[test]
    fn new_zero_sized_array_of_aligned_type_is_a_zst() {
        let mut a = RefAny::new([0u64; 0]);
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());
        assert_eq!(
            a.sharing_info
                .debug_get_refcount_copied()
                ._internal_layout_size,
            0
        );
        assert!(a.downcast_ref::<[u64; 0]>().is_some());
        assert_eq!(a.get_ref_count(), 1);
    }

    // Large + heavily over-aligned payload: the alignment recorded at
    // construction must be honoured by the allocation, or every downcast would
    // hand out a misaligned reference.
    #[test]
    fn new_large_over_aligned_payload_round_trips() {
        #[repr(align(64))]
        #[derive(Clone)]
        struct Big([u8; 4096]);

        let mut a = RefAny::new(Big([0xAB; 4096]));
        assert_eq!(a.get_data_len(), 4096);
        assert_eq!(a.get_data_ptr() as usize % 64, 0);
        let r = a.downcast_ref::<Big>().unwrap();
        assert_eq!((&raw const *r) as usize % 64, 0);
        assert!(r.0.iter().all(|&b| b == 0xAB));
    }

    // ---- numeric limits / round-trip ----

    #[test]
    fn integer_limits_round_trip() {
        round_trip(u8::MIN);
        round_trip(u8::MAX);
        round_trip(i8::MIN);
        round_trip(i8::MAX);
        round_trip(u16::MAX);
        round_trip(i16::MIN);
        round_trip(u32::MAX);
        round_trip(i32::MIN);
        round_trip(u64::MAX);
        round_trip(i64::MIN);
        // u128/i128 are 16-aligned on most targets -> exercises the align path
        round_trip(u128::MAX);
        round_trip(i128::MIN);
        round_trip(i128::MAX);
        round_trip(usize::MAX);
        round_trip(isize::MIN);
        round_trip(0usize);
    }

    // Floats are copied as raw bytes, so every bit pattern (NaN payloads, signed
    // zero, infinities) must survive unchanged — no normalization, no rounding.
    #[test]
    fn float_extremes_round_trip_bit_exact() {
        let mut nan = RefAny::new(f64::NAN);
        assert!(nan.downcast_ref::<f64>().unwrap().is_nan());

        // A NaN with a non-canonical payload must come back bit-identical.
        let bits = 0x7FF0_0000_0000_0001u64;
        let mut payload_nan = RefAny::new(f64::from_bits(bits));
        assert_eq!(payload_nan.downcast_ref::<f64>().unwrap().to_bits(), bits);

        let mut neg_zero = RefAny::new(-0.0f64);
        let nz = neg_zero.downcast_ref::<f64>().unwrap();
        assert!(*nz == 0.0 && nz.is_sign_negative());
        drop(nz);

        let mut inf = RefAny::new(f32::NEG_INFINITY);
        assert_eq!(*inf.downcast_ref::<f32>().unwrap(), f32::NEG_INFINITY);
        // f32 and f64 are distinct types even though both are "floats".
        assert!(inf.downcast_ref::<f64>().is_none());

        round_trip(f64::MIN);
        round_trip(f64::MAX);
        round_trip(f64::MIN_POSITIVE);
        round_trip(f32::EPSILON);
        round_trip(f32::MAX);
    }

    // Owned heap payloads: the value is moved in (`mem::forget` on the original)
    // and dropped exactly once at the end — a double-drop here would be a
    // double-free of the String/Vec buffers.
    #[test]
    fn owned_unicode_payloads_round_trip() {
        round_trip(String::new());
        round_trip(String::from("héllo 🌍 \u{202E}rtl\u{0}nul"));
        round_trip('🌍');

        let v: Vec<String> = vec![String::from("a"), String::from("🎉"), String::new()];
        round_trip(v);
    }

    // A struct with interior padding is byte-copied, padding included: the copy
    // must not disturb the initialized fields.
    #[test]
    fn padded_struct_round_trips() {
        #[derive(Clone, PartialEq, Debug)]
        #[repr(C)]
        struct Padded {
            a: u8,
            b: u64,
            c: u8,
        }
        round_trip(Padded {
            a: 0xFF,
            b: u64::MAX,
            c: 0x01,
        });
    }

    // ---- setters: 0 / 1 / usize::MAX (never dereferenced by azul-core) ----

    #[test]
    fn set_serialize_fn_zero_and_extremes() {
        let mut a = RefAny::new(1u32);
        assert_eq!(a.get_serialize_fn(), 0);
        assert!(!a.can_serialize());

        a.set_serialize_fn(usize::MAX);
        assert_eq!(a.get_serialize_fn(), usize::MAX);
        assert!(a.can_serialize());

        a.set_serialize_fn(1);
        assert_eq!(a.get_serialize_fn(), 1);
        assert!(a.can_serialize());

        a.set_serialize_fn(0);
        assert_eq!(a.get_serialize_fn(), 0);
        assert!(!a.can_serialize());

        // The fn pointer lives in the SHARED inner, so a clone's setter is
        // visible through the original.
        let mut b = a.clone();
        b.set_serialize_fn(42);
        assert_eq!(a.get_serialize_fn(), 42);
        assert!(a.can_serialize());
        b.set_serialize_fn(0);
        assert!(!a.can_serialize());
    }

    #[test]
    fn set_deserialize_fn_zero_and_extremes() {
        let mut a = RefAny::new(1u32);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert!(!a.can_deserialize());

        a.set_deserialize_fn(usize::MAX);
        assert_eq!(a.get_deserialize_fn(), usize::MAX);
        assert!(a.can_deserialize());

        a.set_deserialize_fn(1);
        assert_eq!(a.get_deserialize_fn(), 1);

        a.set_deserialize_fn(0);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert!(!a.can_deserialize());

        let mut b = a.clone();
        b.set_deserialize_fn(42);
        assert_eq!(a.get_deserialize_fn(), 42);
        b.set_deserialize_fn(0);
        assert!(!a.can_deserialize());
    }

    // `set_update_fn` only *stores* the address; a bogus value must round-trip
    // and must be resettable to 0. (Deliberately no `downcast_mut` while the
    // observer is bogus — `downcast_mut` transmutes and CALLS it.)
    #[test]
    fn set_update_fn_zero_and_extremes() {
        let mut a = RefAny::new(1u32);
        assert_eq!(a.get_update_fn(), 0);

        a.set_update_fn(usize::MAX);
        assert_eq!(a.get_update_fn(), usize::MAX);

        a.set_update_fn(0);
        assert_eq!(a.get_update_fn(), 0);
        // With the observer unset again, mutable borrows work as normal.
        assert!(a.downcast_mut::<u32>().is_some());
    }

    // The registered observer must fire exactly once per *successful*
    // `downcast_mut`, and must see the PRE-mutation bytes + the payload length.
    static UPDATE_CALLS: AtomicUsize = AtomicUsize::new(0);
    static UPDATE_LEN: AtomicUsize = AtomicUsize::new(0);
    static UPDATE_PRE_VALUE: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn record_update(ptr: *const c_void, len: usize) {
        UPDATE_CALLS.fetch_add(1, Ordering::SeqCst);
        UPDATE_LEN.store(len, Ordering::SeqCst);
        if !ptr.is_null() && len == core::mem::size_of::<u32>() {
            // SAFETY: only installed on a `RefAny` holding a `u32`, and
            // `downcast_mut` fires it with that live payload pointer.
            let pre = unsafe { core::ptr::read_unaligned(ptr.cast::<u32>()) };
            UPDATE_PRE_VALUE.store(pre as usize, Ordering::SeqCst);
        }
    }

    #[test]
    fn update_fn_fires_once_with_pre_mutation_data() {
        UPDATE_CALLS.store(0, Ordering::SeqCst);

        let mut a = RefAny::new(7u32);
        let cb: extern "C" fn(*const c_void, usize) = record_update;
        a.set_update_fn(cb as usize);
        assert_eq!(a.get_update_fn(), cb as usize);

        {
            let mut m = a.downcast_mut::<u32>().unwrap();
            *m = 9;
        }
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);
        assert_eq!(UPDATE_LEN.load(Ordering::SeqCst), 4);
        // The observer saw 7, not 9: it runs BEFORE the borrow is handed out.
        assert_eq!(UPDATE_PRE_VALUE.load(Ordering::SeqCst), 7);

        // A wrong-type downcast must not fire it.
        assert!(a.downcast_mut::<u64>().is_none());
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);

        // A shared borrow is not a mutation -> must not fire it.
        assert_eq!(*a.downcast_ref::<u32>().unwrap(), 9);
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);

        // A *denied* mutable borrow (shared borrow live on a sibling clone)
        // must not fire it either.
        let mut b = a.clone();
        let r = a.downcast_ref::<u32>().unwrap();
        assert!(b.downcast_mut::<u32>().is_none());
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);
        drop(r);

        // Unregistering stops the observer.
        b.set_update_fn(0);
        assert!(b.downcast_mut::<u32>().is_some());
        assert_eq!(UPDATE_CALLS.load(Ordering::SeqCst), 1);
    }

    // ---- predicates ----

    #[test]
    fn is_type_true_false_and_extremes() {
        let a = RefAny::new(0u32);
        let id = a.get_type_id();

        assert!(a.is_type(id));
        assert!(!a.is_type(!id)); // every bit flipped -> always a different id
        assert!(!a.is_type(id.wrapping_add(1)));
        assert!(!a.is_type(RefAny::get_type_id_static::<i32>()));
        if id != 0 {
            assert!(!a.is_type(0));
        }
        if id != u64::MAX {
            assert!(!a.is_type(u64::MAX));
        }
    }

    #[test]
    fn has_no_copies_transitions() {
        let mut a = RefAny::new(1u32);
        assert!(a.has_no_copies());

        {
            let b = a.clone();
            assert!(!a.has_no_copies()); // num_copies == 2
            assert!(!b.has_no_copies());
        }
        assert!(a.has_no_copies()); // clone dropped -> exclusive again

        {
            // A live shared borrow (taken via a sibling clone) also disqualifies.
            let mut c = a.clone();
            let r = c.downcast_ref::<u32>().unwrap();
            assert_eq!(*r, 1);
            assert!(!a.has_no_copies());
        }
        assert!(a.has_no_copies());

        {
            let mut c = a.clone();
            let m = c.downcast_mut::<u32>().unwrap();
            assert_eq!(*m, 1);
            assert!(!a.has_no_copies());
        }
        assert!(a.has_no_copies());
    }

    #[test]
    fn can_serialize_and_can_deserialize_track_the_fn_pointers() {
        let mut a = RefAny::new(1u32);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());

        a.set_serialize_fn(1);
        assert!(a.can_serialize());
        assert!(!a.can_deserialize());

        a.set_deserialize_fn(usize::MAX);
        assert!(a.can_serialize());
        assert!(a.can_deserialize());

        a.set_serialize_fn(0);
        a.set_deserialize_fn(0);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());
    }

    // ---- getters ----

    #[test]
    fn get_ref_count_tracks_clones_and_borrow_guards() {
        let mut a = RefAny::new(5u8);
        assert_eq!(a.get_ref_count(), 1);

        let mut b = a.clone();
        assert_eq!(a.get_ref_count(), 2);
        assert_eq!(b.get_ref_count(), 2);

        {
            // The guard clones the RefCount, so it keeps the data alive.
            let r = b.downcast_ref::<u8>().unwrap();
            assert_eq!(*r, 5);
            assert_eq!(a.get_ref_count(), 3);
        }
        assert_eq!(a.get_ref_count(), 2);

        {
            let m = b.downcast_mut::<u8>().unwrap();
            assert_eq!(*m, 5);
            assert_eq!(a.get_ref_count(), 3);
        }
        assert_eq!(a.get_ref_count(), 2);

        drop(b);
        assert_eq!(a.get_ref_count(), 1);
        assert_eq!(*a.downcast_ref::<u8>().unwrap(), 5);
    }

    #[test]
    fn debug_snapshot_matches_the_live_counters() {
        let a = RefAny::new(0x1122_3344u32);
        let d = a.sharing_info.debug_get_refcount_copied();
        assert_eq!(d.num_copies, 1);
        assert_eq!(d.num_refs, 0);
        assert_eq!(d.num_mutable_refs, 0);
        assert_eq!(d._internal_len, 4);
        assert_eq!(d._internal_layout_size, 4);
        assert_eq!(d._internal_layout_align, core::mem::align_of::<u32>());
        assert_eq!(d.type_id, RefAny::get_type_id_static::<u32>());
        assert_eq!(d.type_name.as_str(), "u32");
        assert_ne!(d.custom_destructor, 0);
        assert_eq!(d.serialize_fn, 0);
        assert_eq!(d.deserialize_fn, 0);

        a.sharing_info.increase_ref();
        a.sharing_info.increase_refmut();
        let d2 = a.sharing_info.debug_get_refcount_copied();
        assert_eq!(d2.num_refs, 1);
        assert_eq!(d2.num_mutable_refs, 1);
        // The first snapshot is a copy, not a view: it must not have changed.
        assert_eq!(d.num_refs, 0);

        a.sharing_info.decrease_ref();
        a.sharing_info.decrease_refmut();
        let d3 = a.sharing_info.debug_get_refcount_copied();
        assert_eq!((d3.num_refs, d3.num_mutable_refs), (0, 0));

        // The Debug impl goes through `downcast()` — it must not panic.
        assert!(!alloc::format!("{:?}", a.sharing_info).is_empty());
    }

    #[test]
    fn get_type_name_reports_the_rust_type() {
        #[derive(Clone)]
        struct AutotestNamed(#[allow(dead_code)] u8);

        let a = RefAny::new(AutotestNamed(1));
        let name = a.get_type_name();
        assert!(
            name.as_str().contains("AutotestNamed"),
            "unexpected type name: {}",
            name.as_str()
        );

        let generic = RefAny::new(Vec::<String>::new());
        assert!(generic.get_type_name().as_str().contains("Vec"));

        assert_eq!(RefAny::new(1u32).get_type_name().as_str(), "u32");
    }

    // ---- RefCount: construction, downcast, clone/drop balance ----

    #[test]
    fn refcount_new_downcast_and_clone_lifecycle() {
        let rc = RefCount::new(RefCountInner {
            _internal_ptr: core::ptr::null(),
            num_copies: AtomicUsize::new(1),
            num_refs: AtomicUsize::new(0),
            num_mutable_refs: AtomicUsize::new(0),
            _internal_len: 0,
            _internal_layout_size: 0,
            _internal_layout_align: 1,
            type_id: 0xDEAD_BEEF,
            type_name: AzString::from_const_str("autotest::Synthetic"),
            custom_destructor: noop_destructor,
            serialize_fn: 0,
            deserialize_fn: 0,
            update_fn: 0,
        });
        assert!(!rc.ptr.is_null());
        assert!(rc.run_destructor);

        let inner = rc.downcast();
        assert_eq!(inner.type_id, 0xDEAD_BEEF);
        assert_eq!(inner.type_name.as_str(), "autotest::Synthetic");
        assert_eq!(inner._internal_len, 0);
        assert!(rc.can_be_shared());
        assert!(rc.can_be_shared_mut());

        // Clones must keep the boxed inner alive; the counters must return to 1
        // so the final drop frees it exactly once.
        let c1 = rc.clone();
        assert_eq!(rc.debug_get_refcount_copied().num_copies, 2);
        let c2 = c1.clone();
        assert_eq!(rc.debug_get_refcount_copied().num_copies, 3);
        drop(c2);
        drop(c1);
        assert_eq!(rc.debug_get_refcount_copied().num_copies, 1);
    }

    // The borrow counters must saturate at 0 instead of wrapping to usize::MAX
    // (an unmatched `FooRef_delete` from C would otherwise permanently wedge the
    // runtime borrow checker), and stay usable afterwards.
    #[test]
    fn borrow_counters_saturate_at_zero_and_stay_usable() {
        let mut a = RefAny::new(3i64);
        {
            let rc = &a.sharing_info;

            // 64 unmatched decrements on both counters.
            for _ in 0..64 {
                rc.decrease_ref();
                rc.decrease_refmut();
            }
            let d = rc.debug_get_refcount_copied();
            assert_eq!(d.num_refs, 0);
            assert_eq!(d.num_mutable_refs, 0);
            assert!(rc.can_be_shared());
            assert!(rc.can_be_shared_mut());

            // Many shared borrows coexist, but block a mutable one.
            for _ in 0..256 {
                rc.increase_ref();
            }
            assert_eq!(rc.debug_get_refcount_copied().num_refs, 256);
            assert!(rc.can_be_shared());
            assert!(!rc.can_be_shared_mut());
            for _ in 0..256 {
                rc.decrease_ref();
            }
            assert_eq!(rc.debug_get_refcount_copied().num_refs, 0);
            assert!(rc.can_be_shared_mut());

            // Same for the mutable counter, plus one extra decrement.
            rc.increase_refmut();
            rc.increase_refmut();
            assert!(!rc.can_be_shared());
            rc.decrease_refmut();
            rc.decrease_refmut();
            rc.decrease_refmut();
            assert_eq!(rc.debug_get_refcount_copied().num_mutable_refs, 0);
        }

        // The borrow checker still works after all those underflow attempts.
        assert_eq!(*a.downcast_ref::<i64>().unwrap(), 3);
        assert!(a.downcast_mut::<i64>().is_some());
    }

    // ---- get_type_id_static ----

    // The u64 type ID is the ONLY runtime guard against a wrong-type downcast,
    // so distinct types must not collide (this is what folding ALL TypeId bytes
    // buys us) and it must be stable within a process run.
    #[test]
    fn type_id_static_is_stable_and_collision_free() {
        let ids = [
            RefAny::get_type_id_static::<u8>(),
            RefAny::get_type_id_static::<u16>(),
            RefAny::get_type_id_static::<u32>(),
            RefAny::get_type_id_static::<u64>(),
            RefAny::get_type_id_static::<u128>(),
            RefAny::get_type_id_static::<usize>(),
            RefAny::get_type_id_static::<i8>(),
            RefAny::get_type_id_static::<i16>(),
            RefAny::get_type_id_static::<i32>(),
            RefAny::get_type_id_static::<i64>(),
            RefAny::get_type_id_static::<i128>(),
            RefAny::get_type_id_static::<isize>(),
            RefAny::get_type_id_static::<f32>(),
            RefAny::get_type_id_static::<f64>(),
            RefAny::get_type_id_static::<bool>(),
            RefAny::get_type_id_static::<char>(),
            RefAny::get_type_id_static::<()>(),
            RefAny::get_type_id_static::<String>(),
            RefAny::get_type_id_static::<Vec<u8>>(),
            RefAny::get_type_id_static::<Vec<u16>>(),
            RefAny::get_type_id_static::<[u8; 1]>(),
            RefAny::get_type_id_static::<[u8; 2]>(),
            RefAny::get_type_id_static::<(u8, u8)>(),
            RefAny::get_type_id_static::<(u8, u16)>(),
            RefAny::get_type_id_static::<Option<u8>>(),
            RefAny::get_type_id_static::<Option<u16>>(),
        ];

        for i in 0..ids.len() {
            for j in (i + 1)..ids.len() {
                assert_ne!(ids[i], ids[j], "type id collision between {i} and {j}");
            }
        }

        // Deterministic within a run.
        assert_eq!(RefAny::get_type_id_static::<Vec<u8>>(), ids[18]);
        assert_eq!(RefAny::get_type_id_static::<u8>(), ids[0]);
    }

    // ---- clone / instance ids ----

    #[test]
    fn root_instance_id_is_zero_and_clones_are_distinct() {
        let a = RefAny::new(0u8);
        assert_eq!(a.instance_id, 0);

        let b = a.clone();
        let c = b.clone();
        assert_ne!(b.instance_id, 0);
        assert_ne!(c.instance_id, 0);
        assert_ne!(b.instance_id, c.instance_id);
        assert_eq!(a.get_ref_count(), 3);
    }

    // ---- replace_contents ----

    #[test]
    fn replace_contents_zst_and_value_transitions() {
        #[derive(Clone)]
        struct Zst;

        let mut a = RefAny::new(Zst);
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());

        // ZST -> sized: a real allocation must appear.
        assert!(a.replace_contents(RefAny::new(0x4142_4344u32)));
        assert_eq!(a.get_data_len(), 4);
        assert!(!a.get_data_ptr().is_null());
        assert!(a.is_type(RefAny::get_type_id_static::<u32>()));
        assert_eq!(*a.downcast_ref::<u32>().unwrap(), 0x4142_4344);

        // sized -> ZST: the pointer goes back to null, but the ZST still downcasts
        // via a dangling reference; each temporary guard releases its borrow slot
        // on drop, so the exclusive slot is free again afterwards.
        assert!(a.replace_contents(RefAny::new(Zst)));
        assert_eq!(a.get_data_len(), 0);
        assert!(a.get_data_ptr().is_null());
        assert!(a.downcast_ref::<Zst>().is_some());
        assert!(a.downcast_mut::<Zst>().is_some());
        assert!(a.sharing_info.can_be_shared_mut());
    }

    #[test]
    fn replace_contents_is_visible_to_all_clones() {
        let mut a = RefAny::new(1u32);
        let mut b = a.clone();

        assert!(a.replace_contents(RefAny::new(2u32)));
        assert_eq!(*b.downcast_ref::<u32>().unwrap(), 2);

        // The type may change too — every clone sees the new type.
        assert!(a.replace_contents(RefAny::new(String::from("swapped"))));
        assert!(b.downcast_ref::<u32>().is_none());
        assert_eq!(b.downcast_ref::<String>().unwrap().as_str(), "swapped");
        assert!(b.get_type_name().as_str().contains("String"));
        assert_eq!(b.get_type_id(), RefAny::get_type_id_static::<String>());
    }

    #[test]
    fn replace_contents_denied_while_mutably_borrowed() {
        let mut a = RefAny::new(1u32);
        let mut b = a.clone();

        let m = a.downcast_mut::<u32>().unwrap();
        // A live mutable borrow on the shared inner must block the replacement
        // (performing it would free memory the `RefMut` still points at).
        assert!(!b.replace_contents(RefAny::new(2u32)));
        drop(m);

        assert!(b.replace_contents(RefAny::new(2u32)));
        assert_eq!(*b.downcast_ref::<u32>().unwrap(), 2);
    }

    // The serialize/deserialize/update hooks are part of the replaced metadata:
    // after a replacement they describe the NEW value, not the old one.
    #[test]
    fn replace_contents_resets_the_fn_pointers_to_the_new_value() {
        let mut a = RefAny::new(1u32);
        a.set_serialize_fn(3);
        a.set_deserialize_fn(4);
        assert!(a.can_serialize());
        assert!(a.can_deserialize());

        assert!(a.replace_contents(RefAny::new(2u32)));
        assert_eq!(a.get_serialize_fn(), 0);
        assert_eq!(a.get_deserialize_fn(), 0);
        assert_eq!(a.get_update_fn(), 0);
        assert!(!a.can_serialize());
        assert!(!a.can_deserialize());
    }

    // Repeated replacement across changing sizes/alignments must neither leak nor
    // corrupt the payload (Miri checks the alloc/dealloc balance here).
    #[test]
    fn repeated_replace_contents_stays_consistent() {
        let mut a = RefAny::new(String::from("start"));
        for i in 0..16u32 {
            assert!(a.replace_contents(RefAny::new(i)));
            assert_eq!(*a.downcast_ref::<u32>().unwrap(), i);
            assert!(a.replace_contents(RefAny::new(u128::from(i) | (1 << 100))));
            assert_eq!(
                *a.downcast_ref::<u128>().unwrap(),
                u128::from(i) | (1 << 100)
            );
            assert!(a.replace_contents(RefAny::new(String::from("s"))));
        }
        assert_eq!(a.downcast_ref::<String>().unwrap().as_str(), "s");
    }

    // ---- destructor robustness / concurrency ----

    // `default_custom_destructor` is `extern "C"`: a panic from the payload's
    // `Drop` must be caught there, not unwound across the FFI boundary (UB).
    #[cfg(feature = "std")]
    #[test]
    fn panicking_payload_drop_is_contained() {
        struct PanicOnDrop(#[allow(dead_code)] u64);
        impl Drop for PanicOnDrop {
            fn drop(&mut self) {
                panic!("autotest: payload Drop panicked (expected, must be contained)");
            }
        }

        let a = RefAny::new(PanicOnDrop(1));
        drop(a); // must not propagate the panic out of the extern "C" destructor
    }

    // RefAny is Send + Sync: concurrent clone/borrow/drop from several threads
    // must leave the reference count exactly where it started.
    #[cfg(feature = "std")]
    #[test]
    fn concurrent_clone_and_borrow_keeps_the_refcount_balanced() {
        use std::{sync::Arc, thread};

        let shared = Arc::new(RefAny::new(11u32));
        let mut handles = Vec::new();

        for _ in 0..4 {
            let s = Arc::clone(&shared);
            handles.push(thread::spawn(move || {
                for _ in 0..16 {
                    let mut local = (*s).clone();
                    // No thread takes a mutable borrow, so a shared borrow can
                    // never be denied.
                    let r = local
                        .downcast_ref::<u32>()
                        .expect("shared borrow must always succeed here");
                    assert_eq!(*r, 11);
                }
            }));
        }
        for h in handles {
            h.join().expect("worker thread panicked");
        }

        assert_eq!(shared.get_ref_count(), 1);
    }

    /// A RELEASED `RefAny` — one whose `RefCount` has been dropped, which nulls
    /// the pointer on purpose so a second C-side delete is a no-op — must be
    /// safe to INTERROGATE from Rust. It used to abort the process instead:
    /// `get_type_id` walked straight into `RefCount::downcast`'s assertion, and
    /// the release build is `panic = "abort"`, so a stale callback data pointer
    /// took the whole app down on an ordinary focus change (device report,
    /// 2026-09-01). The answer is "nothing here", not death.
    #[test]
    fn a_released_refany_answers_instead_of_aborting() {
        let mut released = RefAny::new(7u32);
        // Take the LIVE `RefCount` out and let it drop: that is the real
        // release path -- it frees the `RefCountInner` and runs the payload's
        // destructor -- and what it leaves behind is exactly what a C-side
        // `AzRefCount_delete` leaves, a null ptr with the decrement already
        // done.
        //
        // This used to stomp `ptr` in place instead. `RefAny::drop` is empty
        // and `RefCount::drop` early-returns on a null ptr, so nothing was ever
        // freed: Miri reported both the `RefCountInner` and the `u32` payload
        // as leaked, correctly, and the Heavy Tests job failed on it the first
        // time it got to run.
        drop(core::mem::replace(
            &mut released.sharing_info,
            RefCount {
                ptr: core::ptr::null(),
                run_destructor: false,
            },
        ));

        assert_eq!(released.get_type_id(), 0, "a released RefAny has no type");
        assert_eq!(released.get_type_name().as_str(), "<released>");
        assert!(
            released.downcast_ref::<u32>().is_none(),
            "there is nothing to borrow",
        );
        assert!(
            released.downcast_mut::<u32>().is_none(),
            "there is nothing to borrow mutably",
        );
        // And the type it really held must not be reachable either.
        assert!(released.downcast_ref::<u8>().is_none());
    }
}
