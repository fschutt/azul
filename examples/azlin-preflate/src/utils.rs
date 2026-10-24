/// A raw deflate stream of `data` at `level`, made with flate2 (the unit tests' streams).
#[cfg(test)]
pub fn deflate_for_tests(data: &[u8], level: u32) -> Vec<u8> {
    use std::io::Write;
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// `len` bytes of prose-like text from a seeded generator, with repeats near and far.
#[cfg(test)]
pub fn text_for_tests(len: usize) -> Vec<u8> {
    const WORDS: &[&str] = &[
        "the", "stream", "is", "rebuilt", "from", "its", "plain", "text", "and", "a", "few",
        "corrections", "bit", "for", "bit", "every", "time",
    ];
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        out.extend_from_slice(WORDS[(seed % WORDS.len() as u64) as usize].as_bytes());
        out.push(if seed % 11 == 0 { b'\n' } else { b' ' });
    }
    out.truncate(len);
    out
}

#[allow(dead_code)]
/// handy function to compare two arrays, and print the first mismatch. Useful for debugging.
#[cfg(test)]
#[track_caller]
pub fn assert_eq_array<T: PartialEq + std::fmt::Debug>(a: &[T], b: &[T]) {
    use core::panic;

    if a.len() != b.len() {
        for i in 0..std::cmp::min(a.len(), b.len()) {
            assert_eq!(
                a[i],
                b[i],
                "length mismatch {},{} and first mismatch at offset {}",
                a.len(),
                b.len(),
                i
            );
        }
        panic!(
            "length mismatch {} and {}, but common prefix identical",
            a.len(),
            b.len()
        );
    } else {
        for i in 0..a.len() {
            assert_eq!(
                a[i],
                b[i],
                "length identical {}, but first mismatch at offset {}",
                a.len(),
                i
            );
        }
    }
}

#[cfg(test)]
#[track_caller]
pub fn assert_block_eq(
    a: &crate::deflate::deflate_token::DeflateTokenBlock,
    b: &crate::deflate::deflate_token::DeflateTokenBlock,
) {
    use crate::deflate::deflate_token::DeflateTokenBlockType;

    if a == b {
        return;
    }

    if std::mem::discriminant(&a.block_type) != std::mem::discriminant(&b.block_type) {
        panic!("block type mismatch {:?} {:?}", a.block_type, b.block_type);
    }

    match (&a.block_type, &b.block_type) {
        (
            DeflateTokenBlockType::Huffman {
                tokens: ta,
                huffman_type: ha,
            },
            DeflateTokenBlockType::Huffman {
                tokens: tb,
                huffman_type: hb,
            },
        ) => {
            assert_eq_array(&ta, &tb);
            assert_eq!(ha, hb);
        }
        (
            DeflateTokenBlockType::Stored { uncompressed: ua },
            DeflateTokenBlockType::Stored { uncompressed: ub },
        ) => {
            assert_eq_array(&ua, &ub);
        }
        _ => {
            panic!("unexpected block type");
        }
    }
    assert_eq!(a.last, b.last, "last flag mismatch");
}

