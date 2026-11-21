//! An S3 listing (`ListObjectsV2`) as a bucket answers it: XML from the storage, read as text.
#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(xml) = std::str::from_utf8(data) {
        let _ = azul_storage::s3::parse_listing(xml);
    }
});
