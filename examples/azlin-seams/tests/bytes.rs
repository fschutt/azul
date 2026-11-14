//! `ByteCount`: checked arithmetic that never wraps, and the KiB/MiB/GiB constructors.
//!
//! Implements planning/architecture.md section 1.4 rule 4 (limits are stated up front, in a type
//! that cannot wrap).

use azlin_seams::sim::Rng;
use azlin_seams::ByteCount;

/// The sizes a test chooses from: the edges of `u64` and a few round numbers.
fn edge_values() -> Vec<u64> {
    vec![
        0,
        1,
        2,
        1023,
        1024,
        1025,
        u64::from(u32::MAX),
        u64::from(u32::MAX) + 1,
        u64::MAX / 2,
        u64::MAX / 2 + 1,
        u64::MAX - 2,
        u64::MAX - 1,
        u64::MAX,
    ]
}

#[test]
fn byte_count_checked_arithmetic_never_wraps() {
    let max = ByteCount::MAX;
    assert_eq!(max.get(), u64::MAX);
    assert_eq!(ByteCount::ZERO.get(), 0);
    assert!(ByteCount::ZERO.is_zero());
    assert!(!ByteCount::new(1).is_zero());

    // Overflow is `None`, never a small number.
    assert_eq!(max.checked_add(ByteCount::new(1)), None);
    assert_eq!(ByteCount::new(1).checked_add(max), None);
    assert_eq!(max.checked_add(max), None);
    assert_eq!(max.checked_add(ByteCount::ZERO), Some(max));
    assert_eq!(
        ByteCount::new(u64::MAX - 1).checked_add(ByteCount::new(1)),
        Some(max)
    );
    assert_eq!(
        ByteCount::new(40).checked_add(ByteCount::new(2)),
        Some(ByteCount::new(42))
    );
    // Underflow is `None`.
    assert_eq!(ByteCount::ZERO.checked_sub(ByteCount::new(1)), None);
    assert_eq!(ByteCount::new(5).checked_sub(ByteCount::new(6)), None);
    assert_eq!(
        ByteCount::new(5).checked_sub(ByteCount::new(5)),
        Some(ByteCount::ZERO)
    );
    assert_eq!(max.checked_sub(max), Some(ByteCount::ZERO));
    assert_eq!(
        max.checked_sub(ByteCount::new(1)),
        Some(ByteCount::new(u64::MAX - 1))
    );
    // Multiplication.
    assert_eq!(max.checked_mul(2), None);
    assert_eq!(max.checked_mul(1), Some(max));
    assert_eq!(max.checked_mul(0), Some(ByteCount::ZERO));
    assert_eq!(ByteCount::new(1 << 63).checked_mul(2), None);
    assert_eq!(
        ByteCount::new(1 << 62).checked_mul(2),
        Some(ByteCount::new(1 << 63))
    );
    assert_eq!(
        ByteCount::kib(4).checked_mul(3),
        Some(ByteCount::new(12_288))
    );
    // Saturating forms stop at the ends.
    assert_eq!(max.saturating_add(ByteCount::new(1)), max);
    assert_eq!(max.saturating_add(max), max);
    assert_eq!(
        ByteCount::new(1).saturating_add(ByteCount::new(2)),
        ByteCount::new(3)
    );
    assert_eq!(
        ByteCount::new(5).saturating_sub(ByteCount::new(6)),
        ByteCount::ZERO
    );
    assert_eq!(
        ByteCount::new(6).saturating_sub(ByteCount::new(5)),
        ByteCount::new(1)
    );
    assert_eq!(ByteCount::ZERO.saturating_sub(max), ByteCount::ZERO);

    // Every pair of edge values against a model in 128-bit arithmetic: no wrapping anywhere.
    let edges = edge_values();
    let mut checked = 0usize;
    for &a in &edges {
        for &b in &edges {
            check_pair(a, b);
            checked += 1;
        }
    }
    let mut rng = Rng::new(0xB17E5);
    for _ in 0..5_000 {
        let a = match rng.below(3) {
            Some(0) => rng.next_u64(),
            Some(1) => u64::MAX - rng.below(1 << 20).unwrap_or(0),
            _ => rng.below(1 << 20).unwrap_or(0),
        };
        let b = match rng.below(3) {
            Some(0) => rng.next_u64(),
            Some(1) => u64::MAX - rng.below(1 << 20).unwrap_or(0),
            _ => rng.below(1 << 20).unwrap_or(0),
        };
        check_pair(a, b);
        checked += 1;
    }
    assert_eq!(checked, edges.len() * edges.len() + 5_000);
}

/// One pair against the 128-bit model.
fn check_pair(a: u64, b: u64) {
    let (x, y) = (ByteCount::new(a), ByteCount::new(b));
    let sum = u128::from(a) + u128::from(b);
    let want_add = u64::try_from(sum).ok().map(ByteCount::new);
    assert_eq!(x.checked_add(y), want_add, "{a} + {b}");
    assert_eq!(
        x.saturating_add(y),
        ByteCount::new(u64::try_from(sum).unwrap_or(u64::MAX)),
        "{a} +sat {b}"
    );
    let want_sub = a.checked_sub(b).map(ByteCount::new);
    assert_eq!(x.checked_sub(y), want_sub, "{a} - {b}");
    assert_eq!(
        x.saturating_sub(y),
        ByteCount::new(a.saturating_sub(b)),
        "{a} -sat {b}"
    );
    let product = u128::from(a) * u128::from(b);
    assert_eq!(
        x.checked_mul(b),
        u64::try_from(product).ok().map(ByteCount::new),
        "{a} * {b}"
    );
    // Ordering is the ordering of the numbers.
    assert_eq!(x.cmp(&y), a.cmp(&b));
}

#[test]
fn byte_count_constructors_kib_mib_gib() {
    assert_eq!(ByteCount::kib(0), ByteCount::ZERO);
    assert_eq!(ByteCount::kib(1).get(), 1_024);
    assert_eq!(ByteCount::mib(1).get(), 1_048_576);
    assert_eq!(ByteCount::gib(1).get(), 1_073_741_824);
    assert_eq!(ByteCount::kib(4).get(), 4_096);
    assert_eq!(ByteCount::mib(4).get(), 4 * 1_048_576);
    assert_eq!(ByteCount::gib(5).get(), 5 * 1_073_741_824);
    // The units are powers of 1024 of one another.
    assert_eq!(ByteCount::kib(1024), ByteCount::mib(1));
    assert_eq!(ByteCount::mib(1024), ByteCount::gib(1));
    assert_eq!(ByteCount::kib(1024 * 1024), ByteCount::gib(1));
    // The largest argument cannot overflow: gib(u32::MAX) is below 2^62.
    assert_eq!(ByteCount::kib(u32::MAX).get(), u64::from(u32::MAX) * 1024);
    assert_eq!(
        ByteCount::mib(u32::MAX).get(),
        u64::from(u32::MAX) * 1_048_576
    );
    assert_eq!(
        ByteCount::gib(u32::MAX).get(),
        u64::from(u32::MAX) * 1_073_741_824
    );
    assert!(ByteCount::gib(u32::MAX) < ByteCount::new(1 << 62));
    assert!(ByteCount::gib(u32::MAX) < ByteCount::MAX);
    // They are `const fn`: a limit is a constant of its crate.
    const HEAD_MAX: ByteCount = ByteCount::kib(4);
    const CAP: ByteCount = ByteCount::gib(1);
    const SMALL: ByteCount = ByteCount::mib(64);
    assert_eq!(HEAD_MAX.get(), 4096);
    assert_eq!(CAP.get(), 1 << 30);
    assert_eq!(SMALL.get(), 1 << 26);
    assert!(HEAD_MAX < SMALL && SMALL < CAP);

    // `new`, `of_len` and the text form.
    assert_eq!(ByteCount::new(77).get(), 77);
    assert_eq!(ByteCount::of_len(0), ByteCount::ZERO);
    assert_eq!(ByteCount::of_len(4096), ByteCount::kib(4));
    assert_eq!(ByteCount::of_len(vec![0u8; 10].len()), ByteCount::new(10));
    assert_eq!(ByteCount::kib(1).to_string(), "1024 B");
    assert_eq!(ByteCount::ZERO.to_string(), "0 B");
    assert_eq!(ByteCount::MAX.to_string(), "18446744073709551615 B");
    assert_eq!(format!("{:?}", ByteCount::new(3)), "ByteCount(3)");
    assert_eq!(ByteCount::default(), ByteCount::ZERO);
}
