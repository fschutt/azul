//! An AZL1 object read with a file key and an object id the input names:
//! `file key (32) || object id (16) || object`. The header and the commitment are read as
//! untrusted bytes; the sealed info and the segments are authenticated, so bytes that were not
//! sealed with the key stop there (X-DELTA-AZDRIVE-KEYS-42). What the key reaches is the second
//! half: the input is also written as a FILE (`azl1::encrypt`: the kind detection, the whole-file
//! transforms - Lepton's encoder and the preflate analysis over bytes a user or a share handed
//! over -, the codecs) and read back whole.
#![no_main]

use azul_storage::crypto::{
    azl1::{self, OpenObject, WriteOptions},
    DriveKey, FileKey, ObjectId,
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
    let drive_key = DriveKey::from_bytes([7; 32]);
    if let Ok((sealed, summary)) = azl1::encrypt(object, id, &drive_key, &WriteOptions::default()) {
        if let Ok(own_key) = drive_key.unwrap_file_key(&summary.wrapped_key, &id) {
            // The oracle: what is written is what is read, whatever the transforms did to it.
            let back = azl1::decrypt(&sealed, &id, &own_key)
                .expect("an object this program just wrote reads back");
            assert_eq!(back, object, "the file read back is not the file written");
        }
    }
    let _ = azl1::decrypt(object, &id, &file_key);
    if let Ok(open) = OpenObject::open_expecting(object, &id, &file_key, object.len() as u64) {
        let _ = open.read_all(object);
        let _ = open.read_range(object, 0, 100);
        let _ = open.read_range(object, 1 << 20, (1 << 20) + 100);
    }
});
