//! The deflate reconstruction both ways, over bytes nobody checked (X-DELTA-AZDRIVE-KEYS-42): the
//! ANALYSIS of a deflate stream a user's file holds (a PDF, a ZIP, a PNG, a gzip - hostile when it
//! came from somebody else) and the REBUILD from a plain text and the corrections a member of the
//! drive wrote. The vendored code's asserts and indexing are what `azlin-preflate/src/lib.rs` leaves
//! to this target; `catch_unwind` around them in azlin-compress is dead under `panic = "abort"`.
//! The first input byte picks the direction: an even one analyses the rest as a deflate stream, an
//! odd one cuts the rest (by the byte's own value) into plain text and corrections and rebuilds.
//! (The unit-test twin: `examples/azlin-preflate/tests/robust.rs`.)
#![no_main]

use azlin_preflate::{
    preflate_whole_deflate_stream, recreate_whole_deflate_stream, PreflateConfig,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((pick, rest)) = data.split_first() else {
        return;
    };
    if pick % 2 == 0 {
        let config = PreflateConfig {
            max_chain_length: 64,
            plain_text_limit: 1 << 22,
            verify_compression: true,
        };
        if let Ok((chunk, plain)) = preflate_whole_deflate_stream(rest, &config) {
            // The oracle: a stream the analysis takes is rebuilt exactly.
            let rebuilt = recreate_whole_deflate_stream(
                plain.text(),
                &chunk.corrections,
                chunk.compressed_size,
            )
            .expect("a stream the analysis took rebuilds");
            assert_eq!(rebuilt[..], rest[..chunk.compressed_size]);
        }
    } else {
        let at = rest.len() * usize::from(*pick) / 256;
        let (plain, corrections) = rest.split_at(at);
        let _ = recreate_whole_deflate_stream(plain, corrections, 1 << 22);
    }
});
