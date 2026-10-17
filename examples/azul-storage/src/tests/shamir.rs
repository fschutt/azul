//! Shamir's secret sharing over GF(256): the field's known answers (FIPS-197), known splits,
//! and the properties - any threshold of shares gives the secret back, a single share of a
//! 2-of-n split is uniform whatever the secret.

use crate::crypto::{
    shamir::{combine, gf_inv, gf_mul, split, split_with, Share, MAX_SHARES},
    CryptoError, Zeroizing,
};

/// The coefficients the known-answer splits use: bytes from `start`, each the last times 5
/// plus 1 (mod 256).
fn counter(start: u8) -> impl FnMut(&mut [u8]) -> Result<(), CryptoError> {
    let mut state = start;
    move |buf: &mut [u8]| {
        for byte in buf.iter_mut() {
            *byte = state;
            state = state.wrapping_mul(5).wrapping_add(1);
        }
        Ok(())
    }
}

/// A deterministic source for the property tests (SplitMix64).
struct Source(u64);

impl Source {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }

    fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn share(x: u8, y: &[u8]) -> Share {
    Share {
        x,
        y: Zeroizing::new(y.to_vec()),
    }
}

#[test]
fn the_field_is_the_aes_field_of_fips_197() {
    // FIPS-197 section 4.2: {57} x {83} = {c1}; section 4.2.1: {57} x {13} = {fe} and the
    // xtime steps {57} x {02}, {04}, {08}, {10}.
    assert_eq!(gf_mul(0x57, 0x83), 0xC1);
    assert_eq!(gf_mul(0x57, 0x13), 0xFE);
    assert_eq!(
        [0x02, 0x04, 0x08, 0x10].map(|b| gf_mul(0x57, b)),
        [0xAE, 0x47, 0x8E, 0x07]
    );
    // The inverse the S-box starts from: {53}^-1 = {ca}.
    assert_eq!(gf_inv(0x53), 0xCA);
    for a in 1..=255u8 {
        assert_eq!(gf_mul(a, gf_inv(a)), 1, "a times its inverse, a = {a}");
        assert_eq!(gf_mul(a, 1), a);
        assert_eq!(gf_mul(a, 0), 0);
    }
    assert_eq!(gf_inv(0), 0);
}

#[test]
fn a_one_byte_split_is_the_line_through_the_secret() {
    // f(x) = 01 + 57 x: f(1) = 56, f(2) = 01 + ae = af, f(3) = 01 + (ae + 57) = f8.
    let shares = split_with(&[0x01], 2, 3, &mut |buf: &mut [u8]| {
        buf.fill(0x57);
        Ok(())
    })
    .unwrap();
    let points: Vec<(u8, String)> = shares.iter().map(|s| (s.x, hex(&s.y))).collect();
    assert_eq!(
        points,
        [
            (1, String::from("56")),
            (2, String::from("af")),
            (3, String::from("f8"))
        ]
    );
}

#[test]
fn known_splits_come_out_byte_for_byte_and_back() {
    let shares = split_with(b"Azlin", 2, 3, &mut counter(7)).unwrap();
    let ys: Vec<String> = shares.iter().map(|s| hex(&s.y)).collect();
    assert_eq!(ys, ["465ed9e3dd", "4f321d6613", "4816a8eca0"]);
    for pair in [[0, 1], [0, 2], [1, 2]] {
        let picked: Vec<Share> = pair.iter().map(|&i| shares[i].clone()).collect();
        assert_eq!(&combine(&picked).unwrap()[..], b"Azlin", "shares {pair:?}");
    }

    let secret: Vec<u8> = (0..16).collect();
    let shares = split_with(&secret, 3, 5, &mut counter(200)).unwrap();
    assert_eq!(hex(&shares[0].y), "2148933a051c970e2900fb12cd343f46");
    assert_eq!(hex(&shares[4].y), "3e823f7862987658ac3ce01284f26be6");
    let picked = [shares[0].clone(), shares[2].clone(), shares[4].clone()];
    assert_eq!(&combine(&picked).unwrap()[..], &secret[..]);
    assert_eq!(&combine(&shares[1..4]).unwrap()[..], &secret[..]);
}

#[test]
fn any_threshold_of_shares_gives_the_secret_back_in_any_order() {
    let mut source = Source(2026_10_10);
    for _ in 0..200 {
        let threshold = 2 + source.below(4) as u8;
        let count = threshold + source.below(4) as u8;
        let len = source.below(40) as usize;
        let secret = source.bytes(len);
        let shares = split(&secret, threshold, count).unwrap();
        assert_eq!(shares.len(), usize::from(count));
        assert!(shares.iter().all(|s| s.y.len() == secret.len()));
        // A random subset of `threshold` or more, in a random order.
        let mut order: Vec<usize> = (0..shares.len()).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, source.below(i as u64 + 1) as usize);
        }
        let take = usize::from(threshold) + source.below(u64::from(count - threshold) + 1) as usize;
        let picked: Vec<Share> = order[..take].iter().map(|&i| shares[i].clone()).collect();
        assert_eq!(
            &combine(&picked).unwrap()[..],
            &secret[..],
            "{threshold} of {count}"
        );
    }
}

#[test]
fn fewer_shares_than_the_threshold_do_not_give_the_secret() {
    let mut source = Source(7);
    for _ in 0..50 {
        let secret = source.bytes(32);
        let shares = split(&secret, 3, 5).unwrap();
        assert_ne!(&combine(&shares[..2]).unwrap()[..], &secret[..]);
    }
}

#[test]
fn one_share_of_a_two_of_n_split_is_uniform_whatever_the_secret() {
    // For each secret byte and each x, the share's byte runs through all 256 values as the
    // coefficient does: one share alone fits every secret equally.
    for secret in [0x00u8, 0x01, 0x5A, 0xFF] {
        for x in [1u8, 2, 3, 200] {
            let mut seen = [false; 256];
            for coefficient in 0..=255u8 {
                let shares = split_with(&[secret], 2, x.max(3), &mut |buf: &mut [u8]| {
                    buf.fill(coefficient);
                    Ok(())
                })
                .unwrap();
                let at_x = shares.iter().find(|s| s.x == x).unwrap();
                seen[usize::from(at_x.y[0])] = true;
            }
            assert!(seen.iter().all(|&s| s), "secret {secret:#04x}, x = {x}");
        }
    }
}

#[test]
fn splits_and_combinations_that_cannot_work_are_refused() {
    assert!(matches!(
        split(b"k", 1, 3),
        Err(CryptoError::Unsupported(_))
    ));
    assert!(matches!(
        split(b"k", 4, 3),
        Err(CryptoError::Unsupported(_))
    ));
    assert_eq!(split(b"k", 2, MAX_SHARES).unwrap().len(), 255);
    assert!(matches!(combine(&[]), Err(CryptoError::Damaged(_))));
    assert!(matches!(
        combine(&[share(0, b"a"), share(1, b"b")]),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        combine(&[share(1, b"a"), share(1, b"a")]),
        Err(CryptoError::Damaged(_))
    ));
    assert!(matches!(
        combine(&[share(1, b"a"), share(2, b"bb")]),
        Err(CryptoError::Damaged(_))
    ));
    assert_eq!(
        format!("{:?}", share(2, b"secret")),
        "Share(x = 2, 6 bytes)",
        "a share never prints its bytes"
    );
    // Two splits of one secret differ (fresh coefficients), and each gives it back.
    let a = split(b"the same secret", 2, 3).unwrap();
    let b = split(b"the same secret", 2, 3).unwrap();
    assert_ne!(a[0].y, b[0].y);
    assert_eq!(&combine(&b[1..]).unwrap()[..], b"the same secret");
}
