//! The microphone the built-in recognizer and the system's recognizer listen to, per platform: the Mac's and the iPhone's through an
//! `AVAudioEngine` tap (the same stack the system's recognizer uses, so the same permission
//! rules), Linux's through ALSA at 16 kHz mono. Where there is none, the built-in recognizer
//! says so instead of listening to nothing.

use super::builtin::MicrophoneSource;

/// The microphone of this platform.
#[must_use]
pub(super) fn platform_source() -> Box<dyn MicrophoneSource> {
    #[cfg(any(target_os = "macos", target_os = "ios"))]
    {
        Box::new(super::apple::AppleMicrophoneSource)
    }
    #[cfg(target_os = "linux")]
    {
        Box::new(linux::Source)
    }
    #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
    {
        Box::new(none::Source)
    }
}

/// A platform whose microphone the built-in recognizer cannot use yet.
#[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "linux")))]
mod none {
    use super::{super::builtin::Microphone, super::recognizer::Outcome, MicrophoneSource};

    pub(super) struct Source;

    impl MicrophoneSource for Source {
        fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
            Err((
                Outcome::Unavailable,
                "the built-in recognizer has no microphone on this platform yet".to_string(),
            ))
        }
    }
}

/// ALSA's default capture device, asked for 16 kHz mono (ALSA converts).
#[cfg(target_os = "linux")]
mod linux {
    use super::{
        super::{
            builtin::{Microphone, MODEL_RATE},
            recognizer::Outcome,
        },
        MicrophoneSource,
    };

    pub(super) struct Source;

    impl MicrophoneSource for Source {
        fn open(&mut self) -> Result<Box<dyn Microphone>, (Outcome, String)> {
            let Some(raw) = crate::desktop::extra::audio::raw_microphone() else {
                return Err((Outcome::Unavailable, "this build has no microphone backend".to_string()));
            };
            let handle = (raw.open)(MODEL_RATE, 1);
            if handle == 0 {
                return Err((
                    Outcome::Unavailable,
                    "no microphone could be opened (is a capture device there, and ALSA installed?)".to_string(),
                ));
            }
            Ok(Box::new(Capture { raw, handle }))
        }
    }

    struct Capture {
        raw: crate::desktop::extra::audio::RawMicrophone,
        handle: u64,
    }

    impl Microphone for Capture {
        fn rate(&self) -> u32 {
            MODEL_RATE
        }

        fn read(&mut self, out: &mut Vec<f32>, _wait_ms: u32) -> Result<(), String> {
            // The device blocks for a chunk of about 64 ms; no frames is a stretch without sound.
            if (self.raw.read)(self.handle, out) == 0 {
                out.clear();
            }
            Ok(())
        }
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            (self.raw.close)(self.handle);
        }
    }
}
