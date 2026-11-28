//! What a read of an AAudio input stream means, and how long the microphone worker is patient: pure, compiled
//! on every host (the NDK calls are in `aaudio.rs`).
//!
//! `AAudioStream_read` answers the number of frames read (0 when nothing arrived within the timeout) or a
//! negative `AAUDIO_ERROR_*`. The worker of the microphone tile treats a read of `0` frames as the END of the
//! stream (the capture vtable's contract: "0 on error / EOF"), and the read was a 100 ms one: an input stream that
//! needs more than 100 ms to deliver its first buffer (it does, right after it starts, and after the system moved
//! the microphone to another app and back) stopped the microphone for good. An empty read now means "wait for the
//! microphone", for a bounded time, and a stream the system disconnected (`AAUDIO_ERROR_DISCONNECTED`: a headset
//! plugged in or out, another app took the microphone) is opened again instead of being given up.

/// `AAUDIO_ERROR_DISCONNECTED`: the device behind the stream went away.
pub const ERROR_DISCONNECTED: i32 = -899;

/// Reads of 100 ms with nothing in them before the stream is given up: 5 s. The first buffer of a started input
/// stream takes tens of milliseconds, a busy phone some hundred.
pub const MAX_EMPTY_READS: u32 = 50;

/// Times a disconnected stream is opened again before the microphone is given up.
pub const MAX_REOPENS: u32 = 3;

/// What the reader of the stream does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// `frames` frames arrived: deliver them.
    Deliver(u32),
    /// Nothing yet: read again.
    Retry,
    /// The system disconnected the stream: open a new one, then read again.
    Reopen,
    /// Nothing will come: the stream is dead (an error, or too long silent), end the microphone.
    Stop,
}

/// How much patience is left, across the reads of one `mic_read`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Patience {
    empty_reads: u32,
    reopens: u32,
}

impl Patience {
    /// A reader with all its patience.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            empty_reads: 0,
            reopens: 0,
        }
    }

    /// What to do after a read that answered `rc` (frames read, or a negative error).
    #[must_use]
    pub fn next(&mut self, rc: i32) -> Action {
        if rc > 0 {
            // Data: the silence so far is over.
            self.empty_reads = 0;
            return Action::Deliver(rc.unsigned_abs());
        }
        if rc == 0 {
            self.empty_reads += 1;
            return if self.empty_reads >= MAX_EMPTY_READS {
                Action::Stop
            } else {
                Action::Retry
            };
        }
        if rc == ERROR_DISCONNECTED && self.reopens < MAX_REOPENS {
            self.reopens += 1;
            self.empty_reads = 0;
            return Action::Reopen;
        }
        Action::Stop
    }
}

#[cfg(test)]
#[path = "aaudio_read_test.rs"]
mod tests;
