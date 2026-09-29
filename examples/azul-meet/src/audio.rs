//! The pure audio logic of AzMeet, free of azul types so `cargo test -p AzMeet` checks it without
//! a window or a sound card: the wire format of the audio track, the packetizer that cuts
//! captured chunks into 20 ms packets, the jitter buffer that turns received packets back into a
//! steady stream, the playout clock, the control message that carries mute and deafen, the test
//! tone, and the lines the window shows.
//!
//! # Wire format
//!
//! Audio rides the iroh frame path on its own track. One frame, numbers little endian:
//!
//! ```text
//! [version u8 = 1][codec u8 = 1: PCM s16][count u8][reserved u8 = 0][sample_rate u32]
//! count times: [sequence u32][length u16][length samples, i16 each]      oldest packet first
//! ```
//!
//! A packet is 20 ms of mono audio at the sender's microphone rate (960 samples at 48 kHz).
//! Frames on an iroh track are latest-wins: a newer frame replaces one that has not left yet, and
//! the receiver keeps only the newest frame of a track until the app reads it. So every frame
//! carries the newest packet and the [`REDUNDANCY`] - 1 packets before it: a skipped frame loses
//! nothing while the next one arrives, and the jitter buffer drops the copies.
//!
//! PCM s16 is codec 1. The codec byte leaves room for Opus, the next step (about 32 kbit/s
//! instead of 768, with its own loss concealment and forward error correction).
//!
//! # Control messages
//!
//! Reliable iroh messages, one kind byte first. `[1][flags u8]` is the sender's state: bit 0
//! muted, bit 1 deafened. Unknown kinds are ignored, so later versions can add kinds; bytes after
//! a known message are ignored, so later versions can append fields.

use std::collections::{BTreeMap, VecDeque};

/// Milliseconds of audio in one packet.
pub const PACKET_MS: u32 = 20;
/// Version byte of an audio frame.
pub const WIRE_VERSION: u8 = 1;
/// Codec byte: 16-bit signed PCM.
pub const CODEC_PCM16: u8 = 1;
/// Packets per frame: the newest and the ones before it.
pub const REDUNDANCY: usize = 3;
/// Packets the jitter buffer holds before it starts to play (60 ms).
pub const TARGET_PACKETS: usize = 3;
/// Packets the jitter buffer holds at most; past that the oldest are dropped (200 ms).
pub const MAX_PACKETS: usize = 10;
/// Turns with an empty buffer after which the jitter buffer waits to fill up again (200 ms): the
/// sender has gone quiet (muted, or gone), so its next packet starts a new stretch of speech.
pub const REFILL_AFTER_MISSES: u32 = 10;
/// The most the test tone and the playout clock catch up after a stall; older turns are skipped.
pub const MAX_CATCH_UP_MS: u64 = 100;
/// Peak level of the test tone.
pub const TONE_AMPLITUDE: f32 = 0.2;

/// Samples (per channel) in one packet at `sample_rate`; at least one.
pub fn samples_per_packet(_sample_rate: u32) -> usize {
    0
}

/// One sample as 16-bit PCM: clamped to -1.0..=1.0, NaN as silence.
pub fn to_pcm16(_sample: f32) -> i16 {
    0
}

/// One 16-bit PCM sample as `f32` in -1.0..=1.0.
pub fn from_pcm16(_sample: i16) -> f32 {
    0.0
}

/// A packet as `f32` samples, for `AudioSink::play`.
pub fn pcm_to_f32(_samples: &[i16]) -> Vec<f32> {
    Vec::new()
}

/// 20 ms of mono 16-bit PCM and its place in the sender's stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub sequence: u32,
    pub samples: Vec<i16>,
}

/// One frame of the audio track, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireFrame {
    pub sample_rate: u32,
    pub packets: Vec<Packet>,
}

/// The frame carrying `packets` (oldest first) at `sample_rate`.
pub fn encode_frame(_sample_rate: u32, _packets: &[Packet]) -> Vec<u8> {
    Vec::new()
}

/// Reads a frame; `None` for another version or codec, no packets, a zero rate, or bytes that do
/// not add up.
pub fn decode_frame(_bytes: &[u8]) -> Option<WireFrame> {
    None
}

/// Cuts captured audio (chunks of any length, any channel count) into 20 ms mono packets and
/// wraps each new packet, with the ones before it, into a frame.
#[derive(Debug, Default)]
pub struct Packetizer {
    sample_rate: u32,
    pending: Vec<i16>,
    next_sequence: u32,
    recent: VecDeque<Packet>,
}

impl Packetizer {
    pub fn new() -> Self {
        Packetizer::default()
    }

    /// Feeds one captured chunk (interleaved `f32`, mixed down to mono) and returns the frames to
    /// send: one for every packet it completes.
    pub fn push(&mut self, _sample_rate: u32, _channels: u16, _samples: &[f32]) -> Vec<Vec<u8>> {
        Vec::new()
    }

    /// Forgets the unfinished packet and the packets a frame repeats, as on mute: the next frame
    /// carries only new audio. Sequence numbers continue.
    pub fn reset(&mut self) {}

    /// The sequence number the next packet gets.
    pub fn next_sequence(&self) -> u32 {
        self.next_sequence
    }
}

/// What a jitter buffer has seen so far.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JitterStats {
    /// Packets taken in (each sequence number once).
    pub received: u64,
    /// Copies of packets already taken in: the redundancy of the wire format.
    pub duplicates: u64,
    /// Packets that arrived after their turn to play had passed.
    pub late: u64,
    /// Packets dropped because the buffer was full.
    pub overflow: u64,
    /// Packets played.
    pub played: u64,
    /// Turns played as silence because their packet was missing.
    pub silent: u64,
}

/// Holds one peer's packets until their turn: in sequence order whatever order they arrive in,
/// without copies, without packets whose turn has passed, and with silence for missing ones.
///
/// Sequence numbers are compared as plain `u32`s: at 50 packets a second they wrap after two
/// years and a half of one call.
#[derive(Debug)]
pub struct JitterBuffer {
    target: usize,
    max: usize,
    sample_rate: u32,
    packets: BTreeMap<u32, Vec<i16>>,
    /// The sequence number whose turn is next; `None` while the buffer fills up.
    next: Option<u32>,
    /// Packets below this sequence number are late.
    floor: Option<u32>,
    /// The highest sequence number seen, and which of the 128 below it were seen (bit n = newest - n).
    newest: Option<u32>,
    seen: u128,
    /// Turns in a row with an empty buffer.
    misses: u32,
    stats: JitterStats,
}

impl JitterBuffer {
    /// A buffer that starts to play once it holds `target` packets and holds at most `max`.
    pub fn new(target: usize, max: usize) -> Self {
        let target = target.max(1);
        JitterBuffer {
            target,
            max: max.max(target),
            sample_rate: 0,
            packets: BTreeMap::new(),
            next: None,
            floor: None,
            newest: None,
            seen: 0,
            misses: 0,
            stats: JitterStats::default(),
        }
    }

    /// Takes in a packet sent at `sample_rate`.
    pub fn push(&mut self, _sample_rate: u32, _packet: Packet) {}

    /// The next 20 ms to play: the packet whose turn it is, silence when it is missing, or `None`
    /// while the buffer fills up (before the first packet, and after the sender went quiet).
    pub fn pop(&mut self) -> Option<Vec<i16>> {
        None
    }

    /// Packets waiting for their turn.
    pub fn buffered(&self) -> usize {
        self.packets.len()
    }

    /// Whether the buffer has started to play.
    pub fn is_playing(&self) -> bool {
        self.next.is_some()
    }

    /// The rate of the packets it holds.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn stats(&self) -> JitterStats {
        self.stats
    }

    /// Drops the waiting packets and fills up again before playing (deafen); the counts stay.
    pub fn clear(&mut self) {}
}

/// Paces playout at one packet per 20 ms of wall time.
#[derive(Debug, Default)]
pub struct PlayoutClock {
    /// Turns handed out so far; turn k is due `k * 20` ms after the start.
    done: u64,
}

impl PlayoutClock {
    pub fn new() -> Self {
        PlayoutClock::default()
    }

    /// How many packets to play now, `elapsed_ms` after the clock started. After a stall longer
    /// than [`MAX_CATCH_UP_MS`] the older turns are skipped rather than played in a burst.
    pub fn due(&mut self, _elapsed_ms: u64) -> u64 {
        0
    }

    /// Milliseconds from `elapsed_ms` until the next turn is due.
    pub fn wait_ms(&self, _elapsed_ms: u64) -> u64 {
        0
    }
}

/// A sine for tests (`AZMEET_TEST_TONE=1`), handed out by wall time like a microphone would.
#[derive(Debug)]
pub struct ToneSource {
    frequency: f64,
    sample_rate: u32,
    /// Samples handed out so far.
    produced: u64,
}

impl ToneSource {
    pub fn new(frequency: f32, sample_rate: u32) -> Self {
        ToneSource {
            frequency: f64::from(frequency),
            sample_rate,
            produced: 0,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The mono samples due `elapsed_ms` after the tone started that were not handed out yet (at
    /// most [`MAX_CATCH_UP_MS`] of them after a stall).
    pub fn take(&mut self, _elapsed_ms: u64) -> Vec<f32> {
        Vec::new()
    }
}

/// A participant's audio state, as its control message says.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PeerState {
    /// The microphone is off.
    pub muted: bool,
    /// The participant hears nobody.
    pub deafened: bool,
}

/// A control message between two participants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    State(PeerState),
}

/// The control message announcing `state`.
pub fn encode_state(_state: PeerState) -> Vec<u8> {
    Vec::new()
}

/// Reads a control message; `None` for an unknown kind or a short message.
pub fn decode_control(_bytes: &[u8]) -> Option<Control> {
    None
}

/// A row of the people list: `label` ("Ben · connected", "Ada (you)") and, when known, the
/// person's audio state: "Ben · connected · muted".
pub fn person_line(label: &str, _state: Option<PeerState>) -> String {
    label.to_string()
}

/// The window's line for one peer's incoming audio:
/// "Audio from Ben: 250 packets, 247 played, 3 silent, 0 late, 3 buffered".
pub fn audio_line(_name: &str, _stats: &JitterStats, _buffered: usize) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    /// Samples in a packet at 48 kHz.
    const N: usize = 960;
    /// What `played` reports for a turn of silence.
    const SILENCE: i32 = -1;

    /// A packet whose samples all say which one it is.
    fn packet(sequence: u32) -> Packet {
        Packet {
            sequence,
            samples: vec![sequence as i16 + 100; N],
        }
    }

    fn filled(target: usize, max: usize, sequences: &[u32]) -> JitterBuffer {
        let mut jitter = JitterBuffer::new(target, max);
        for s in sequences {
            jitter.push(RATE, packet(*s));
        }
        jitter
    }

    /// What `pops` turns played: the packet's sequence number, `SILENCE`, or `None` while filling.
    fn played(jitter: &mut JitterBuffer, pops: usize) -> Vec<Option<i32>> {
        (0..pops)
            .map(|_| {
                jitter.pop().map(|samples| {
                    assert_eq!(samples.len(), N, "a turn is one packet long");
                    if samples.iter().all(|s| *s == 0) {
                        SILENCE
                    } else {
                        i32::from(samples[0]) - 100
                    }
                })
            })
            .collect()
    }

    fn sequences(frame: &[u8]) -> Vec<u32> {
        decode_frame(frame)
            .expect("the packetizer's frames decode")
            .packets
            .iter()
            .map(|p| p.sequence)
            .collect()
    }

    #[test]
    fn a_packet_is_20_ms_of_mono_audio() {
        assert_eq!(samples_per_packet(48_000), 960);
        assert_eq!(samples_per_packet(44_100), 882);
        assert_eq!(samples_per_packet(16_000), 320);
        assert_eq!(samples_per_packet(0), 1);
    }

    #[test]
    fn samples_become_16_bit_pcm_clamped_and_come_back() {
        assert_eq!(to_pcm16(0.0), 0);
        assert_eq!(to_pcm16(1.0), 32767);
        assert_eq!(to_pcm16(-1.0), -32767);
        assert_eq!(to_pcm16(0.5), 16384);
        assert_eq!(to_pcm16(4.0), 32767);
        assert_eq!(to_pcm16(-4.0), -32767);
        assert_eq!(to_pcm16(f32::NAN), 0);
        assert_eq!(from_pcm16(32767), 1.0);
        assert_eq!(from_pcm16(i16::MIN), -1.0);
        for s in [-32767_i16, -12345, -1, 0, 1, 777, 32767] {
            assert_eq!(to_pcm16(from_pcm16(s)), s);
        }
        assert_eq!(pcm_to_f32(&[0, 32767]), vec![0.0, 1.0]);
    }

    #[test]
    fn a_frame_round_trips_through_the_wire_format() {
        let packets = vec![
            Packet {
                sequence: 7,
                samples: vec![1, -2, i16::MAX, i16::MIN],
            },
            Packet {
                sequence: 8,
                samples: vec![],
            },
        ];
        let bytes = encode_frame(44_100, &packets);
        assert_eq!(
            &bytes[..8],
            &[WIRE_VERSION, CODEC_PCM16, 2, 0, 0x44, 0xac, 0, 0]
        );
        assert_eq!(&bytes[8..14], &[7, 0, 0, 0, 4, 0]);
        assert_eq!(bytes.len(), 8 + (6 + 4 * 2) + 6);
        assert_eq!(
            decode_frame(&bytes),
            Some(WireFrame {
                sample_rate: 44_100,
                packets
            })
        );
    }

    #[test]
    fn a_malformed_frame_is_refused() {
        let good = encode_frame(RATE, &[packet(1)]);
        assert!(decode_frame(&good).is_some());
        let mut bad: Vec<Vec<u8>> = vec![
            Vec::new(),
            good[..7].to_vec(),
            good[..good.len() - 1].to_vec(),
        ];
        let mut trailing = good.clone();
        trailing.push(0);
        bad.push(trailing);
        // Another version, another codec, no packets, more packets than there are.
        for (index, value) in [(0, 2), (1, 9), (2, 0), (2, 2)] {
            let mut changed = good.clone();
            changed[index] = value;
            bad.push(changed);
        }
        let mut no_rate = good.clone();
        no_rate[4..8].copy_from_slice(&[0; 4]);
        bad.push(no_rate);
        for bytes in bad {
            assert_eq!(
                decode_frame(&bytes),
                None,
                "{:?}",
                &bytes[..bytes.len().min(12)]
            );
        }
    }

    #[test]
    fn the_packetizer_cuts_any_chunking_into_20_ms_packets() {
        let mut packetizer = Packetizer::new();
        let mut frames = Vec::new();
        for _ in 0..4 {
            frames.extend(packetizer.push(RATE, 1, &[0.25; 500]));
        }
        // 2000 samples: two packets of 960, and 80 wait for the next chunk.
        assert_eq!(frames.len(), 2);
        let first = decode_frame(&frames[0]).expect("a frame");
        assert_eq!(first.sample_rate, RATE);
        assert_eq!(first.packets.len(), 1);
        assert_eq!(first.packets[0].sequence, 0);
        assert_eq!(first.packets[0].samples, vec![to_pcm16(0.25); N]);
        assert_eq!(packetizer.next_sequence(), 2);
        assert_eq!(packetizer.push(RATE, 1, &[0.25; 880]).len(), 1);
    }

    #[test]
    fn each_frame_repeats_the_packets_before_it() {
        let mut packetizer = Packetizer::new();
        let frames = packetizer.push(RATE, 1, &vec![0.0; N * 5]);
        let carried: Vec<Vec<u32>> = frames.iter().map(|f| sequences(f)).collect();
        assert_eq!(
            carried,
            vec![
                vec![0],
                vec![0, 1],
                vec![0, 1, 2],
                vec![1, 2, 3],
                vec![2, 3, 4]
            ]
        );
    }

    #[test]
    fn stereo_is_mixed_down_to_mono() {
        let mut packetizer = Packetizer::new();
        let stereo: Vec<f32> = (0..N).flat_map(|_| [0.5, -0.1]).collect();
        let frames = packetizer.push(RATE, 2, &stereo);
        assert_eq!(frames.len(), 1);
        let wire = decode_frame(&frames[0]).expect("a frame");
        assert_eq!(
            wire.packets[0].samples,
            vec![to_pcm16((0.5 - 0.1) / 2.0); N]
        );
    }

    #[test]
    fn muting_forgets_the_unfinished_packet_but_sequence_numbers_continue() {
        let mut packetizer = Packetizer::new();
        assert_eq!(packetizer.push(RATE, 1, &vec![0.5; N + N / 2]).len(), 1);
        packetizer.reset();
        let frames = packetizer.push(RATE, 1, &vec![-0.5; N]);
        assert_eq!(frames.len(), 1);
        let wire = decode_frame(&frames[0]).expect("a frame");
        assert_eq!(
            wire.packets.len(),
            1,
            "the frame repeats no audio from before the mute"
        );
        assert_eq!(wire.packets[0].sequence, 1);
        assert_eq!(wire.packets[0].samples, vec![to_pcm16(-0.5); N]);
    }

    #[test]
    fn a_new_sample_rate_starts_a_new_packet_and_nothing_is_cut_from_no_audio() {
        let mut packetizer = Packetizer::new();
        assert!(packetizer.push(RATE, 1, &[0.1; 500]).is_empty());
        let frames = packetizer.push(16_000, 1, &[0.1; 320]);
        assert_eq!(frames.len(), 1);
        let wire = decode_frame(&frames[0]).expect("a frame");
        assert_eq!(wire.sample_rate, 16_000);
        assert_eq!(wire.packets.len(), 1);
        assert_eq!(wire.packets[0].samples.len(), 320);
        assert!(packetizer.push(0, 1, &[0.1; 2000]).is_empty());
        assert!(packetizer.push(RATE, 0, &[0.1; 2000]).is_empty());
    }

    #[test]
    fn the_jitter_buffer_waits_for_three_packets_before_it_plays() {
        let mut jitter = JitterBuffer::new(3, 10);
        assert_eq!(jitter.pop(), None);
        jitter.push(RATE, packet(0));
        jitter.push(RATE, packet(1));
        assert_eq!(jitter.pop(), None);
        assert!(!jitter.is_playing());
        jitter.push(RATE, packet(2));
        assert_eq!(played(&mut jitter, 3), vec![Some(0), Some(1), Some(2)]);
        assert!(jitter.is_playing());
        assert_eq!(jitter.stats().played, 3);
        assert_eq!(jitter.sample_rate(), RATE);
    }

    #[test]
    fn packets_play_in_sequence_order_whatever_order_they_arrive_in() {
        let mut jitter = filled(3, 10, &[12, 10, 11, 14, 13]);
        assert_eq!(
            played(&mut jitter, 5),
            vec![Some(10), Some(11), Some(12), Some(13), Some(14)]
        );
        assert_eq!(jitter.stats().received, 5);
    }

    #[test]
    fn an_underrun_plays_silence_and_the_missing_packet_still_plays_if_it_comes() {
        let mut jitter = filled(3, 10, &[0, 1, 2]);
        assert_eq!(
            played(&mut jitter, 5),
            vec![Some(0), Some(1), Some(2), Some(SILENCE), Some(SILENCE)]
        );
        jitter.push(RATE, packet(3));
        jitter.push(RATE, packet(4));
        assert_eq!(played(&mut jitter, 2), vec![Some(3), Some(4)]);
        let stats = jitter.stats();
        assert_eq!((stats.played, stats.silent, stats.late), (5, 2, 0));
    }

    #[test]
    fn a_lost_packet_is_skipped_with_silence_once_a_later_one_is_here() {
        let mut jitter = filled(3, 10, &[0, 1, 3, 4]);
        assert_eq!(
            played(&mut jitter, 4),
            vec![Some(0), Some(1), Some(SILENCE), Some(3)]
        );
    }

    #[test]
    fn a_packet_that_comes_after_its_turn_is_dropped_as_late() {
        let mut jitter = filled(3, 10, &[0, 1, 3, 4]);
        played(&mut jitter, 3);
        jitter.push(RATE, packet(2));
        assert_eq!(jitter.stats().late, 1);
        assert_eq!(jitter.buffered(), 2);
        assert_eq!(played(&mut jitter, 2), vec![Some(3), Some(4)]);
    }

    #[test]
    fn copies_of_a_packet_are_duplicates_not_late() {
        let mut jitter = filled(3, 10, &[0, 1, 2]);
        played(&mut jitter, 2);
        // A frame repeats the two packets before its newest: 1 has played, 2 is waiting.
        for s in [1, 2, 3] {
            jitter.push(RATE, packet(s));
        }
        let stats = jitter.stats();
        assert_eq!((stats.received, stats.duplicates, stats.late), (4, 2, 0));
        assert_eq!(played(&mut jitter, 2), vec![Some(2), Some(3)]);
    }

    #[test]
    fn after_a_long_silence_the_buffer_fills_up_again_before_playing() {
        let mut jitter = filled(3, 10, &[0, 1, 2]);
        played(&mut jitter, 3);
        let quiet = played(&mut jitter, REFILL_AFTER_MISSES as usize);
        assert!(quiet.iter().all(|turn| *turn == Some(SILENCE)), "{quiet:?}");
        assert!(!jitter.is_playing());
        assert_eq!(jitter.pop(), None);
        // The sender speaks again: its packets wait until the buffer has filled up.
        jitter.push(RATE, packet(3));
        jitter.push(RATE, packet(4));
        assert_eq!(jitter.pop(), None);
        jitter.push(RATE, packet(5));
        assert_eq!(played(&mut jitter, 3), vec![Some(3), Some(4), Some(5)]);
    }

    #[test]
    fn a_full_buffer_drops_the_oldest_packets() {
        let mut jitter = filled(3, 5, &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(jitter.buffered(), 5);
        assert_eq!(jitter.stats().overflow, 3);
        assert_eq!(
            played(&mut jitter, 5),
            vec![Some(3), Some(4), Some(5), Some(6), Some(7)]
        );

        // While playing, dropping the oldest moves the next turn past them.
        let mut jitter = filled(2, 3, &[0, 1]);
        assert_eq!(played(&mut jitter, 1), vec![Some(0)]);
        for s in 2..=5 {
            jitter.push(RATE, packet(s));
        }
        assert_eq!(jitter.stats().overflow, 2);
        assert_eq!(played(&mut jitter, 3), vec![Some(3), Some(4), Some(5)]);
    }

    #[test]
    fn a_sample_rate_change_starts_the_buffer_over() {
        let mut jitter = filled(1, 10, &[0, 1]);
        jitter.push(
            16_000,
            Packet {
                sequence: 2,
                samples: vec![7; 320],
            },
        );
        assert_eq!(jitter.sample_rate(), 16_000);
        assert_eq!(jitter.buffered(), 1);
        assert_eq!(jitter.pop(), Some(vec![7; 320]));
        assert_eq!(jitter.pop(), Some(vec![0; 320]), "silence at the new rate");
    }

    #[test]
    fn clearing_drops_the_waiting_packets_and_keeps_the_counts() {
        let mut jitter = filled(3, 10, &[0, 1, 2, 3]);
        played(&mut jitter, 1);
        jitter.clear();
        assert_eq!(jitter.buffered(), 0);
        assert!(!jitter.is_playing());
        assert_eq!(jitter.pop(), None);
        assert_eq!(jitter.stats().received, 4);
        assert_eq!(jitter.stats().played, 1);
    }

    #[test]
    fn a_packetized_stream_plays_back_intact_when_every_other_frame_is_skipped() {
        let mut packetizer = Packetizer::new();
        let signal: Vec<f32> = (0..N * 20)
            .map(|i| (i % 200) as f32 / 200.0 - 0.5)
            .collect();
        let frames: Vec<Vec<u8>> = signal
            .chunks(441)
            .flat_map(|chunk| packetizer.push(RATE, 1, chunk))
            .collect();
        assert_eq!(frames.len(), 20);

        let mut jitter = JitterBuffer::new(TARGET_PACKETS, MAX_PACKETS);
        let mut out: Vec<i16> = Vec::new();
        for (i, frame) in frames.iter().enumerate() {
            // Latest-wins: every odd frame is replaced before it leaves (the last one arrives).
            let arrives = i % 2 == 0 || i == frames.len() - 1;
            if arrives {
                let wire = decode_frame(frame).expect("a frame");
                for p in wire.packets {
                    jitter.push(wire.sample_rate, p);
                }
            }
            // One turn per 20 ms.
            if let Some(samples) = jitter.pop() {
                out.extend(samples);
            }
        }
        while jitter.buffered() > 0 {
            out.extend(jitter.pop().expect("a playing buffer plays"));
        }
        let expected: Vec<i16> = signal.iter().map(|s| to_pcm16(*s)).collect();
        assert_eq!(out.len(), expected.len());
        assert!(out == expected, "the samples arrive unchanged and in order");
        let stats = jitter.stats();
        assert_eq!((stats.played, stats.silent, stats.late), (20, 0, 0));
    }

    #[test]
    fn the_playout_clock_plays_one_packet_every_20_ms() {
        let mut clock = PlayoutClock::new();
        assert_eq!(clock.due(0), 1);
        assert_eq!(clock.wait_ms(0), 20);
        assert_eq!(clock.due(5), 0);
        assert_eq!(clock.wait_ms(5), 15);
        assert_eq!(clock.due(20), 1);
        assert_eq!(clock.due(61), 2);
        assert_eq!(clock.wait_ms(61), 19);
    }

    #[test]
    fn a_stalled_playout_skips_turns_instead_of_bursting() {
        let mut clock = PlayoutClock::new();
        assert_eq!(clock.due(0), 1);
        assert_eq!(clock.due(1_000), MAX_CATCH_UP_MS / u64::from(PACKET_MS));
        assert_eq!(clock.due(1_020), 1);
    }

    #[test]
    fn the_test_tone_is_a_440_hz_sine_at_a_fifth_of_full_scale() {
        let mut tone = ToneSource::new(440.0, RATE);
        assert_eq!(tone.sample_rate(), RATE);
        assert!(tone.take(0).is_empty());
        let second: Vec<f32> = (1..=50).flat_map(|i| tone.take(i * 20)).collect();
        assert_eq!(second.len(), 48_000);
        let peak = second.iter().fold(0.0_f32, |m, s| m.max(s.abs()));
        assert!((peak - TONE_AMPLITUDE).abs() < 1e-3, "peak {peak}");
        let cycles = second
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count();
        assert!((439..=441).contains(&cycles), "{cycles} cycles in a second");
        assert!(tone.take(1_000).is_empty(), "nothing new is due");
    }

    #[test]
    fn after_a_stall_the_tone_catches_up_by_at_most_the_catch_up_limit() {
        let mut tone = ToneSource::new(440.0, RATE);
        assert_eq!(tone.take(20).len(), 960);
        assert_eq!(tone.take(5_000).len(), (MAX_CATCH_UP_MS * 48) as usize);
        assert_eq!(tone.take(5_020).len(), 960);
    }

    #[test]
    fn mute_and_deafen_travel_as_a_two_byte_message() {
        for (muted, deafened, flags) in [
            (false, false, 0_u8),
            (true, false, 1),
            (false, true, 2),
            (true, true, 3),
        ] {
            let state = PeerState { muted, deafened };
            let bytes = encode_state(state);
            assert_eq!(bytes, vec![1, flags]);
            assert_eq!(decode_control(&bytes), Some(Control::State(state)));
        }
    }

    #[test]
    fn an_unknown_or_short_control_message_is_ignored_and_extra_bytes_are_allowed() {
        assert_eq!(decode_control(&[]), None);
        assert_eq!(decode_control(&[1]), None);
        assert_eq!(decode_control(&[0, 1]), None);
        assert_eq!(decode_control(&[9, 1]), None);
        assert_eq!(
            decode_control(&[1, 0b101, 42]),
            Some(Control::State(PeerState {
                muted: true,
                deafened: false
            }))
        );
    }

    #[test]
    fn a_muted_peer_reads_ben_connected_muted() {
        let muted = PeerState {
            muted: true,
            deafened: false,
        };
        assert_eq!(
            person_line("Ben · connected", Some(muted)),
            "Ben · connected · muted"
        );
        assert_eq!(
            person_line(
                "Ben · connected",
                Some(PeerState {
                    muted: true,
                    deafened: true
                })
            ),
            "Ben · connected · muted · deafened"
        );
        assert_eq!(
            person_line("Ben · connected", Some(PeerState::default())),
            "Ben · connected"
        );
        assert_eq!(person_line("Ben · connecting", None), "Ben · connecting");
        assert_eq!(
            person_line(
                "Ada (you)",
                Some(PeerState {
                    muted: false,
                    deafened: true
                })
            ),
            "Ada (you) · deafened"
        );
    }

    #[test]
    fn the_audio_line_counts_what_arrived_and_what_played() {
        let stats = JitterStats {
            received: 250,
            duplicates: 500,
            late: 1,
            overflow: 0,
            played: 240,
            silent: 3,
        };
        assert_eq!(
            audio_line("Ben", &stats, 3),
            "Audio from Ben: 250 packets, 240 played, 3 silent, 1 late, 3 buffered"
        );
    }
}
