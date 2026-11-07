//! The READ path of a stored transform: the record stream Lepton's decoder and the preflate
//! rebuild turn back into a file (`azlin_compress::transform::inverse`). The bytes are those of a
//! member of the drive - who holds the file key, so authentication proves nothing of what is
//! inside - and `catch_unwind` around the decoders is dead under `panic = "abort"`: any panic these
//! find is an abort of every device that reads the object (X-DELTA-AZDRIVE-KEYS-15). The first
//! input byte names the transform (0 none, 1 JPEG, 2 containers).
#![no_main]

use azlin_compress::{transform, Limits, TransformId};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Some((id, stream)) = data.split_first() else {
        return;
    };
    let id = match id % 3 {
        0 => TransformId::None,
        1 => TransformId::Jpeg,
        _ => TransformId::Containers,
    };
    let limits = Limits {
        max_decoded_len: 1 << 22,
        ..Limits::default()
    };
    let mut out = Vec::new();
    let _ = transform::inverse(id, &mut &*stream, &mut out, &limits, 1);
});
