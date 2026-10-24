//! Lepton (lepton_jpeg 0.5.8): one JPEG image to its Lepton data and back, with the image's
//! own size as Lepton's hard limits, no global thread pool, and a panic inside Lepton turned
//! into an error.

use std::{
    io::Cursor,
    panic::{catch_unwind, AssertUnwindSafe},
};

use lepton_jpeg::{EnabledFeatures, LeptonThreadPool};
use zeroize::Zeroizing;

use super::scan::JpegSpan;
use crate::error::{damaged, Error};

/// A thread pool that starts one short-lived thread per task: no global state, nothing left
/// running once the image is done.
struct SpawnPool {
    threads: usize,
}

impl LeptonThreadPool for SpawnPool {
    fn max_parallelism(&self) -> usize {
        self.threads.max(1)
    }

    fn run(&self, f: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(f);
    }
}

fn file_size(len: usize) -> Result<u32, Error> {
    u32::try_from(len).map_err(|_| Error::Limit(String::from("a JPEG over 4 GiB")))
}

/// The Lepton data of `jpeg` (the bytes of `span`).
pub(crate) fn encode(jpeg: &[u8], span: &JpegSpan, threads: usize) -> Result<Vec<u8>, Error> {
    let mut features = EnabledFeatures::compat_lepton_vector_write();
    features.max_jpeg_width = u32::from(span.width);
    features.max_jpeg_height = u32::from(span.height);
    features.max_jpeg_file_size = file_size(jpeg.len())?;
    features.max_processor_threads = u32::try_from(threads.max(1)).unwrap_or(1);
    let pool = SpawnPool { threads };
    let mut out = Vec::with_capacity(jpeg.len());
    // Lepton returns errors for bad input; a panic would be its bug, and still only means
    // that this image stays as it is.
    let result = catch_unwind(AssertUnwindSafe(|| {
        lepton_jpeg::encode_lepton(
            &mut Cursor::new(jpeg),
            &mut Cursor::new(&mut out),
            &features,
            &pool,
        )
    }));
    match result {
        Ok(Ok(_)) => Ok(out),
        Ok(Err(e)) => Err(Error::Codec(format!("lepton encode: {e}"))),
        Err(_) => Err(Error::Codec(String::from("lepton encode panicked"))),
    }
}

/// The JPEG of `lepton`, which must be exactly `jpeg_len` bytes of a `width` x `height` image.
pub(crate) fn decode(
    lepton: &[u8],
    jpeg_len: usize,
    width: u16,
    height: u16,
    threads: usize,
) -> Result<Zeroizing<Vec<u8>>, Error> {
    let mut features = EnabledFeatures::compat_lepton_vector_read();
    features.max_jpeg_width = u32::from(width);
    features.max_jpeg_height = u32::from(height);
    features.max_jpeg_file_size = file_size(jpeg_len)?;
    features.max_processor_threads = u32::try_from(threads.max(1)).unwrap_or(1);
    let pool = SpawnPool { threads };
    let mut out = Zeroizing::new(Vec::with_capacity(jpeg_len));
    let result = catch_unwind(AssertUnwindSafe(|| {
        lepton_jpeg::decode_lepton(&mut Cursor::new(lepton), &mut *out, &features, &pool)
    }));
    match result {
        Ok(Ok(_)) => {}
        Ok(Err(e)) => return Err(damaged(format!("a Lepton record does not decode: {e}"))),
        Err(_) => return Err(damaged("a Lepton record does not decode (Lepton panicked)")),
    }
    if out.len() != jpeg_len {
        return Err(damaged(
            "a Lepton record decodes to another length than it says",
        ));
    }
    Ok(out)
}
