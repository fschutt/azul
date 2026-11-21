//! A node's answer as HTTP/1.1 bytes (S3 over iroh): the head, the headers, a content length, a
//! chunked body with its size lines.
#![no_main]

use azul_storage::http1;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    for head_request in [false, true] {
        let _ = http1::decode_reply(data, head_request);
    }
});
