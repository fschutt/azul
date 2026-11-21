//! A container (AZC1) as a pack member or a stored object holds it: the header, the segments
//! (zstd, brotli, the shipped dictionaries), the transform behind them and the end record
//! (`azlin_compress::decode`).
#![no_main]

use azlin_compress::Options;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let mut options = Options::default();
    options.limits.max_decoded_len = 1 << 22;
    let _ = azlin_compress::decode(data, &options);
});
