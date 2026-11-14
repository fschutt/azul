//! C2: secret hygiene. `Secret<N>` never prints a byte, has no `Clone`, `Copy`, `Default`,
//! `PartialEq`, `Display` or serialization (the `compile_fail` doctests in `src/secret.rs` and the
//! negative-trait checks below), wipes on `zeroize()`, and compares in constant time through
//! `subtle`.
//!
//! Implements planning/architecture.md section 5.5 and QUALITY rule 8 (secrets only in witness
//! types: no `Clone`, redacted `Debug`, constant-time equality).

#![cfg_attr(not(feature = "test-support"), allow(unused_imports, dead_code))]

use std::fs;
use std::path::PathBuf;

use azlin_keys::Secret;
#[cfg(feature = "test-support")]
use azlin_keys::{EntropyError, FailingEntropy, SeededEntropy};
use zeroize::Zeroize;

/// Negative-trait checks: `<Secret<N> as AmbiguousIfX<_>>::check()` compiles only when the type
/// does NOT implement the trait (the ambiguity of two blanket impls). If someone adds the impl,
/// this test target stops compiling, which is the failure.
trait AmbiguousIfClone<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfClone<()> for T {}
impl<T: Clone> AmbiguousIfClone<u8> for T {}

trait AmbiguousIfCopy<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfCopy<()> for T {}
impl<T: Copy> AmbiguousIfCopy<u8> for T {}

trait AmbiguousIfPartialEq<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfPartialEq<()> for T {}
impl<T: ?Sized + PartialEq> AmbiguousIfPartialEq<u8> for T {}

trait AmbiguousIfDefault<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfDefault<()> for T {}
impl<T: Default> AmbiguousIfDefault<u8> for T {}

trait AmbiguousIfDisplay<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfDisplay<()> for T {}
impl<T: ?Sized + std::fmt::Display> AmbiguousIfDisplay<u8> for T {}

trait AmbiguousIfHash<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfHash<()> for T {}
impl<T: ?Sized + std::hash::Hash> AmbiguousIfHash<u8> for T {}

fn secret_of<const N: usize>(fill: u8) -> Secret<N> {
    Secret::from_bytes([fill; N])
}

/// A struct that derives `Debug` over a secret, as a config or a key bundle does.
#[derive(Debug)]
#[allow(dead_code)] // its fields are read through the derived Debug, which is the point
struct Holder {
    label: &'static str,
    key: Secret<32>,
}

fn debug_and_ct_eq_for<const N: usize>() {
    // The Debug text is the same for every content and names only the size.
    let a = secret_of::<N>(0xAB);
    let b = secret_of::<N>(0x01);
    let expected = format!("Secret<{N}>(***)");
    assert_eq!(format!("{a:?}"), expected);
    assert_eq!(format!("{b:?}"), expected);
    assert_eq!(
        format!("{a:#?}"),
        expected,
        "pretty Debug prints no byte either"
    );

    // Equality: same bytes equal, any single differing byte (or bit) unequal.
    assert!(a.ct_eq(&a));
    assert!(a.ct_eq(&a.duplicate()));
    assert!(!a.ct_eq(&b));
    for flip in 0..N * 8 {
        let (position, bit) = (flip / 8, flip % 8);
        let mut bytes = [0xABu8; N];
        bytes[position] ^= 1 << bit;
        let flipped = Secret::<N>::from_bytes(bytes);
        assert!(
            !a.ct_eq(&flipped),
            "byte {position} bit {bit} must make a difference"
        );
    }
}

#[test]
fn c2_secret_hygiene() {
    // The sizes the engine uses: 16 (recovery code), 32 (keys), 64 (key pairs and seeds).
    debug_and_ct_eq_for::<16>();
    debug_and_ct_eq_for::<32>();
    debug_and_ct_eq_for::<64>();
    debug_and_ct_eq_for::<68>(); // a space key's plaintext (S15c1)

    // A derived Debug of a holder shows the redacted secret and nothing of its bytes.
    let holder = Holder {
        label: "device key",
        key: secret_of::<32>(0xCD),
    };
    let text = format!("{holder:?}");
    assert_eq!(
        text,
        "Holder { label: \"device key\", key: Secret<32>(***) }"
    );
    assert!(!text.to_lowercase().contains("cd"));
    assert!(
        !text.contains("205"),
        "the decimal of 0xCD is not printed either"
    );
    let pretty = format!("{holder:#?}");
    assert!(pretty.contains("Secret<32>(***)"));
    assert!(!pretty.to_lowercase().contains("cd"));

    // No Clone, Copy, PartialEq, Default, Display, Hash: each line compiles only while the trait
    // is absent.
    <Secret<32> as AmbiguousIfClone<_>>::check();
    <Secret<32> as AmbiguousIfCopy<_>>::check();
    <Secret<32> as AmbiguousIfPartialEq<_>>::check();
    <Secret<32> as AmbiguousIfDefault<_>>::check();
    <Secret<32> as AmbiguousIfDisplay<_>>::check();
    <Secret<32> as AmbiguousIfHash<_>>::check();
    <Secret<16> as AmbiguousIfClone<_>>::check();
    <Secret<64> as AmbiguousIfClone<_>>::check();
}

#[test]
fn c2_zeroize_clears_the_bytes() {
    let mut s = secret_of::<32>(0xAB);
    assert_eq!(s.expose(), &[0xABu8; 32]);
    s.zeroize();
    assert_eq!(s.expose(), &[0u8; 32], "zeroize() clears every byte");

    let mut big = secret_of::<68>(0xFF);
    big.zeroize();
    assert!(big.expose().iter().all(|b| *b == 0));

    // The duplicate is independent: wiping one does not wipe the other.
    let original = secret_of::<16>(0x5A);
    let mut copy = original.duplicate();
    assert!(original.ct_eq(&copy));
    copy.zeroize();
    assert_eq!(copy.expose(), &[0u8; 16]);
    assert_eq!(original.expose(), &[0x5Au8; 16]);
    assert!(!original.ct_eq(&copy));
}

#[test]
fn c2_expose_mut_and_from_slice() {
    let mut s = secret_of::<32>(0);
    s.expose_mut().copy_from_slice(&[7u8; 32]);
    assert_eq!(s.expose(), &[7u8; 32]);

    assert!(Secret::<32>::from_slice(&[1u8; 32]).is_some());
    assert!(Secret::<32>::from_slice(&[1u8; 31]).is_none());
    assert!(Secret::<32>::from_slice(&[1u8; 33]).is_none());
    assert!(Secret::<32>::from_slice(&[]).is_none());
    assert!(Secret::<0>::from_slice(&[]).is_some());
    let from_slice = Secret::<16>::from_slice(&[9u8; 16]).expect("16 bytes");
    assert!(from_slice.ct_eq(&secret_of::<16>(9)));
}

/// `ct_eq` is `subtle`'s: the source compares through `ConstantTimeEq` and never with `==` or
/// `memcmp`-like helpers, and the crate depends on `subtle` (a source check: a timing test on a
/// shared machine proves nothing).
#[test]
fn c2_ct_eq_uses_subtle_and_not_equality_operators() {
    let source =
        fs::read_to_string(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/secret.rs"))
            .expect("src/secret.rs");
    let code = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("use subtle::ConstantTimeEq;"),
        "ct_eq is subtle's"
    );
    assert!(code.contains(".ct_eq("), "ct_eq calls subtle's ct_eq");
    // The body of `fn ct_eq` holds no `==`, `!=`, `.eq(` or `starts_with`.
    let body_start = code.find("pub fn ct_eq").expect("fn ct_eq");
    let body_end = body_start + code[body_start..].find("\n    }\n").expect("end of ct_eq");
    let body = &code[body_start..body_end];
    for forbidden in ["==", "!=", ".eq(", "starts_with", "memcmp", "iter().zip"] {
        assert!(
            !body.contains(forbidden),
            "ct_eq must not use `{forbidden}`: {body}"
        );
    }
    // No serialization or comparison trait is implemented anywhere in the module.
    for forbidden in [
        "impl<const N: usize> Clone",
        "impl<const N: usize> PartialEq",
        "impl<const N: usize> Default",
        "impl<const N: usize> fmt::Display",
        "Serialize",
        "derive(",
    ] {
        assert!(
            !code.contains(forbidden),
            "src/secret.rs must not contain `{forbidden}`"
        );
    }
    // The destructor wipes.
    assert!(code.contains("impl<const N: usize> Drop for Secret<N>"));
    assert!(code.contains("self.0.zeroize()"));
}

#[cfg(feature = "test-support")]
#[test]
fn c2_generate_fills_in_place_and_fails_without_a_secret() {
    let mut ent = SeededEntropy::from_u64(11);
    let a = Secret::<32>::generate(&mut ent).expect("32 bytes");
    assert_eq!(ent.drawn(), 32, "exactly N bytes are drawn");
    let b = Secret::<32>::generate(&mut ent).expect("32 bytes");
    assert!(!a.ct_eq(&b), "two draws differ");
    assert_ne!(a.expose(), &[0u8; 32]);

    let mut replay = SeededEntropy::from_u64(11);
    let again = Secret::<32>::generate(&mut replay).expect("32 bytes");
    assert!(a.ct_eq(&again), "the same seed gives the same secret");

    let mut failing = FailingEntropy::after(31);
    assert_eq!(
        Secret::<32>::generate(&mut failing).err(),
        Some(EntropyError::Exhausted)
    );
    let mut failing = FailingEntropy::after(0);
    assert_eq!(
        Secret::<16>::generate(&mut failing).err(),
        Some(EntropyError::Exhausted)
    );
}
