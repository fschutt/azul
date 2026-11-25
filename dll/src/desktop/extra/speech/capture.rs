//! The microphone the built-in recognizer listens to, per platform: the Mac's through an
//! `AVAudioEngine` tap (the same stack the system's recognizer uses, so the same permission
//! rules), Linux's through ALSA at 16 kHz mono. Where there is none, the built-in recognizer
//! says so instead of listening to nothing.

use super::{
    builtin::{Microphone, MicrophoneSource},
    recognizer::Outcome,
};

/// The microphone of this platform.
#[must_use]
pub(super) fn platform_source() -> Box<dyn MicrophoneSource> {
    #[cfg(target_os = "macos")]
    {
        Box::new(super::apple::AppleMicrophoneSource)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(LinuxSource)
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        Box::new(NoSource)
    }
}

/// A platform whose microphone the built-in recognizer cannot use yet.
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
struct NoSource;

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
impl MicrophoneSource for NoSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        Err((
            Outcome::Unavailable,
            "the built-in recognizer has no microphone on this platform yet".to_string(),
        ))
    }
}

/// ALSA's default capture device, asked for 16 kHz mono (ALSA converts).
#[cfg(target_os = "linux")]
struct LinuxSource;

#[cfg(target_os = "linux")]
impl MicrophoneSource for LinuxSource {
    fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
        let Some(raw) = crate::desktop::extra::audio::raw_microphone() else {
            return Err((Outcome::Unavailable, "this build has no microphone backend".to_string()));
        };
        let handle = (raw.open)(super::builtin::MODEL_RATE, 1);
        if handle == 0 {
            return Err((
                Outcome::Unavailable,
                "no microphone could be opened (is a capture device there, and ALSA installed?)".to_string(),
            ));
        }
        Ok(Box::new(LinuxMicrophone { raw, handle }))
    }
}

#[cfg(target_os = "linux")]
struct LinuxMicrophone {
    raw: crate::desktop::extra::audio::RawMicrophone,
    handle: u64,
}

#[cfg(target_os = "linux")]
impl Microphone for LinuxMicrophone {
    fn rate(&self) -> u32 {
        super::builtin::MODEL_RATE
    }

    fn read(&mut self, out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
        // The device blocks for a chunk of about 64 ms; no frames is a stretch without sound.
        if (self.raw.read)(self.handle, out) == 0 {
            out.clear();
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
impl Drop for LinuxMicrophone {
    fn drop(&mut self) {
        (self.raw.close)(self.handle);
    }
}
