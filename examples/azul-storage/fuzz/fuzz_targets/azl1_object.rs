//! An AZL1 object read with a file key and an object id the input names:
//! `file key (32) || object id (16) || object`. The header, the sealed info, the trailer's table
//! and every segment are read as untrusted bytes (`azl1::decrypt`, `OpenObject::read_all` and
//! `read_range`).
#![no_main]

use azul_storage::crypto::{
    azl1::{self, OpenObject},
    FileKey, ObjectId,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((key, rest)) = data.split_at_checked(32) else {
        return;
    };
    let Some((id, object)) = rest.split_at_checked(16) else {
        return;
    };
    let Some(file_key) = FileKey::from_slice(key) else {
        return;
    };
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(id);
    let id = ObjectId(bytes);
    let _ = azl1::decrypt(object, &id, &file_key);
    if let Ok(open) = OpenObject::open_expecting(object, &id, &file_key, object.len() as u64) {
        let _ = open.read_all(object);
        let _ = open.read_range(object, 0, 100);
        let _ = open.read_range(object, 1 << 20, (1 << 20) + 100);
    }
});
