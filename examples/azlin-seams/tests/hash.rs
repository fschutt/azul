//! `Hash32` and `hash32`: domain separation, the length frame, strict lowercase hex.
//!
//! Implements planning/architecture.md section 5.9 (hash chains: `prev` is a hash) and section 2.5
//! (a domain string per structure).

use std::collections::BTreeSet;

use azlin_seams::sim::Rng;
use azlin_seams::{hash32, Hash32, IdError};

mod common;
use common::to_hex;

/// The bytes `hash32` hashes: each part behind its length (`u64`, little-endian).
fn framed(parts: &[&[u8]]) -> Vec<u8> {
    let mut out = Vec::new();
    for part in parts {
        out.extend_from_slice(&(part.len() as u64).to_le_bytes());
        out.extend_from_slice(part);
    }
    out
}

#[test]
fn hash32_is_domain_separated_and_displays_lowercase_hex() {
    let parts: [&[u8]; 2] = [b"hello", b"world"];

    // The layout, re-derived with the one-shot `blake3::derive_key`: the domain is the context, the
    // input is the length-framed parts.
    let expected = blake3::derive_key("azlin-test-v1", &framed(&parts));
    assert_eq!(
        hash32("azlin-test-v1", &parts),
        Hash32::from_bytes(expected)
    );
    assert_eq!(
        hash32("azlin-test-v1", &[]),
        Hash32::from_bytes(blake3::derive_key("azlin-test-v1", b""))
    );

    // Deterministic, and a domain is a different function.
    assert_eq!(
        hash32("azlin-test-v1", &parts),
        hash32("azlin-test-v1", &parts)
    );
    assert_ne!(
        hash32("azlin-test-v1", &parts),
        hash32("azlin-test-v2", &parts)
    );
    assert_ne!(
        hash32("azlin-test-v1", &parts),
        hash32("azlin-other-v1", &parts)
    );
    assert_ne!(
        hash32("a", &parts),
        hash32("", &parts),
        "the empty domain is a domain too"
    );
    // The domain is not just another part of the input.
    assert_ne!(
        hash32("azlin-test-v1", &[b"x"]),
        hash32("", &[b"azlin-test-v1", b"x"])
    );
    // And it is not the plain BLAKE3 of the framed parts.
    assert_ne!(
        hash32("azlin-test-v1", &parts).as_bytes(),
        blake3::hash(&framed(&parts)).as_bytes()
    );

    // The length frame: the same bytes cut into other parts are another input.
    let split_a = hash32("d", &[b"ab", b"c"]);
    let split_b = hash32("d", &[b"a", b"bc"]);
    let whole = hash32("d", &[b"abc"]);
    assert_ne!(split_a, split_b);
    assert_ne!(split_a, whole);
    assert_ne!(split_b, whole);
    assert_ne!(
        hash32("d", &[]),
        hash32("d", &[b""]),
        "no parts is not one empty part"
    );
    assert_ne!(hash32("d", &[b""]), hash32("d", &[b"", b""]));
    assert_ne!(
        hash32("d", &[b"a", b"b"]),
        hash32("d", &[b"b", b"a"]),
        "the order matters"
    );
    // A part that looks like a frame does not forge one.
    let mut forged = 1u64.to_le_bytes().to_vec();
    forged.push(b'a');
    assert_ne!(
        hash32("d", &[&forged]),
        hash32("d", &[b"a"]),
        "a part holding a frame is still one part"
    );

    // Many random part lists: equal lists hash equally, different lists differently.
    let mut rng = Rng::new(0x5EA15);
    let mut seen: BTreeSet<[u8; 32]> = BTreeSet::new();
    let mut lists: Vec<Vec<Vec<u8>>> = Vec::new();
    for _ in 0..300 {
        let count = rng.below(4).unwrap_or(0) as usize;
        let list: Vec<Vec<u8>> = (0..count)
            .map(|_| {
                let len = rng.below(6).unwrap_or(0) as usize;
                rng.bytes(len)
            })
            .collect();
        lists.push(list);
    }
    for list in &lists {
        let refs: Vec<&[u8]> = list.iter().map(Vec::as_slice).collect();
        let hash = hash32("azlin-prop-v1", &refs);
        assert_eq!(hash, hash32("azlin-prop-v1", &refs));
        seen.insert(*hash.as_bytes());
    }
    let distinct: BTreeSet<&Vec<Vec<u8>>> = lists.iter().collect();
    assert_eq!(
        seen.len(),
        distinct.len(),
        "distinct inputs, distinct hashes"
    );

    // Display: 64 lowercase hex digits, the bytes in order; Debug names the type.
    let hash = hash32("azlin-test-v1", &parts);
    let text = hash.to_string();
    assert_eq!(text.len(), 64);
    assert_eq!(text, to_hex(hash.as_bytes()));
    assert!(text
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(format!("{hash:?}"), format!("Hash32({text})"));
    let mut bytes = [0u8; 32];
    bytes[0] = 0xAB;
    bytes[31] = 0x0F;
    assert_eq!(
        Hash32::from_bytes(bytes).to_string(),
        "ab0000000000000000000000000000000000000000000000000000000000000f"
    );
}

#[test]
fn hash32_zero_ordering_and_bytes() {
    assert!(Hash32::ZERO.is_zero());
    assert_eq!(Hash32::ZERO.as_bytes(), &[0u8; 32]);
    assert_eq!(Hash32::LEN, 32);
    assert_eq!(
        Hash32::ZERO.to_string(),
        "0000000000000000000000000000000000000000000000000000000000000000"
    );
    let mut one = [0u8; 32];
    one[31] = 1;
    assert!(!Hash32::from_bytes(one).is_zero());
    let mut high = [0u8; 32];
    high[0] = 1;
    // Ordered as bytes, first byte most significant: usable as a map key.
    assert!(Hash32::from_bytes(one) < Hash32::from_bytes(high));
    assert!(Hash32::ZERO < Hash32::from_bytes(one));
    let mut set = BTreeSet::new();
    set.insert(Hash32::from_bytes(high));
    set.insert(Hash32::from_bytes(one));
    set.insert(Hash32::ZERO);
    set.insert(Hash32::ZERO);
    assert_eq!(set.len(), 3);
    // A hash of anything is not ZERO.
    assert!(!hash32("d", &[]).is_zero());
}

#[test]
fn hash32_from_str_is_strict() {
    let hash = hash32("azlin-test-v1", &[b"x"]);
    let text = hash.to_string();
    assert_eq!(text.parse::<Hash32>(), Ok(hash));
    assert_eq!(
        "0000000000000000000000000000000000000000000000000000000000000000".parse::<Hash32>(),
        Ok(Hash32::ZERO)
    );
    assert_eq!(
        "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".parse::<Hash32>(),
        Ok(Hash32::from_bytes([0xFF; 32]))
    );

    // Length: anything but 64 digits.
    for len in [0usize, 1, 2, 62, 63, 65, 66, 128] {
        let text: String = "a".repeat(len);
        assert_eq!(text.parse::<Hash32>(), Err(IdError::Length), "{len} digits");
    }
    // A character that is not a lowercase hex digit, at every position of a 64-byte text.
    let valid = "0123456789abcdef".repeat(4);
    assert_eq!(
        valid.parse::<Hash32>().map(|h| h.to_string()),
        Ok(valid.clone())
    );
    for bad in [
        'A', 'F', 'g', 'G', 'x', ' ', '\n', '\0', '-', '+', '/', ':', '`', '@',
    ] {
        for position in [0usize, 1, 31, 62, 63] {
            let mut bytes = valid.clone().into_bytes();
            bytes[position] = bad as u8;
            let text = String::from_utf8(bytes).expect("ascii");
            assert_eq!(
                text.parse::<Hash32>(),
                Err(IdError::BadDigit),
                "{bad:?} at {position}"
            );
        }
    }
    // Uppercase is another spelling of the same hash: refused, there is one spelling.
    assert_eq!(
        text.to_uppercase().parse::<Hash32>(),
        Err(IdError::BadDigit)
    );
    // A prefix, padding, or a multi-byte character that makes 64 bytes.
    assert_eq!(
        format!("0x{}", &text[..62]).parse::<Hash32>(),
        Err(IdError::BadDigit)
    );
    assert_eq!(
        format!(" {}", &text[..63]).parse::<Hash32>(),
        Err(IdError::BadDigit)
    );
    assert_eq!(
        format!("{} ", &text[..63]).parse::<Hash32>(),
        Err(IdError::BadDigit)
    );
    assert_eq!(
        format!("{}\n", &text[..63]).parse::<Hash32>(),
        Err(IdError::BadDigit)
    );
    let wide = format!("{}\u{e9}", "a".repeat(62)); // 62 + 2 bytes
    assert_eq!(wide.len(), 64);
    assert_eq!(wide.parse::<Hash32>(), Err(IdError::BadDigit));
    // The text of a signature or a 16-byte id is no hash.
    assert_eq!(text[..32].parse::<Hash32>(), Err(IdError::Length));
    assert_eq!(
        format!("{text}{text}").parse::<Hash32>(),
        Err(IdError::Length)
    );

    // Round trip over random bytes.
    let mut rng = Rng::new(0x4A5C);
    for _ in 0..200 {
        let mut bytes = [0u8; 32];
        rng.fill_bytes(&mut bytes);
        let hash = Hash32::from_bytes(bytes);
        assert_eq!(hash.to_string().parse::<Hash32>(), Ok(hash));
    }
}
