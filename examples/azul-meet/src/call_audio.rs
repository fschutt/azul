//! The call's audio: this side's microphone packed into Opus or PCM packets on the audio track, each
//! peer's packets into a jitter buffer, the playout thread mixing them for the speaker, and the echo
//! canceller between them.

use super::*;

/// Received audio, shared by the UI thread (which pushes each peer's packets) and the playout
/// thread (which takes one turn from every peer's jitter buffer each 20 ms). Either holds the
/// lock only to move packets, never while a device plays.
pub(crate) struct Playout {
    /// One jitter buffer per origin (a peer's key), whichever way its audio comes.
    pub(crate) peers: BTreeMap<u64, audio::JitterBuffer>,
    /// Play through an `AudioSink` per peer. False in a headless run: the buffers are drained
    /// and counted, and no device is opened.
    pub(crate) play: bool,
    /// Cancels the echo of what plays from the microphone (at `MIC_RATE`): fed every turn the
    /// playout plays, used by `send_audio`. `None` when nothing plays on a device, or turned off
    /// (`AZMEET_ECHO_CANCEL=0`).
    pub(crate) echo: Option<SharedEcho>,
    /// The sound check's packets still to play (the chime, the test clip), one per turn on a
    /// sink of their own ([`SOUND_CHECK_SINK`]), and the rate they are at. Never a jitter
    /// buffer: the whole clip arrives at once, and a jitter buffer keeps only 200 ms.
    pub(crate) check: std::collections::VecDeque<Vec<i16>>,
    pub(crate) check_rate: u32,
}

/// The playout's key for the sound check's sink, next to the peers' keys. A peer's key is a hash
/// of its endpoint id, so it is never this one in practice.
pub(crate) const SOUND_CHECK_SINK: u64 = u64::MAX;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The echo canceller, so the playout state can move to its thread. The generated handle is a raw
/// pointer and therefore not `Send`; the library keeps the canceller's state behind a `Mutex`
/// (`audio/echo.rs`, `EchoCanceller::stream`).
pub(crate) struct SharedEcho(pub(crate) EchoCanceller);

// SAFETY: the canceller's state lives behind a Mutex inside the library, and AzMeet reaches it
// only while holding the `Playout` mutex - the handle is never used from two threads at once,
// and it is dropped (closed) exactly once, with the `Playout`.
unsafe impl Send for SharedEcho {}

/// Starts the playout thread. It ends once the returned handle (kept in `MeetState`) is gone.
pub(crate) fn start_playout(play: bool) -> Arc<Mutex<Playout>> {
    let cancel = setting("AZMEET_ECHO_CANCEL").map_or(true, |v| v != "0");
    let echo = (play && cancel)
        .then(|| EchoCanceller::create(MIC_RATE, ECHO_TAIL_MS))
        .filter(EchoCanceller::is_open)
        .map(SharedEcho);
    let shared = Arc::new(Mutex::new(Playout {
        peers: BTreeMap::new(),
        play,
        echo,
        check: std::collections::VecDeque::new(),
        check_rate: MIC_RATE,
    }));
    let weak = Arc::downgrade(&shared);
    let spawned = std::thread::Builder::new()
        .name(String::from("azmeet_playout"))
        .spawn(move || playout_loop(weak));
    if let Err(e) = spawned {
        eprintln!("[azmeet] could not start the audio playout thread: {e}");
    }
    shared
}

/// Every 20 ms, one turn (a packet, or silence) from each peer's jitter buffer, played through
/// that peer's own `AudioSink`, opened at the peer's rate, so the system mixes the peers. A sink's
/// `play` may block (an ALSA write does); that paces this thread, never the UI thread.
pub(crate) fn playout_loop(shared: Weak<Mutex<Playout>>) {
    let started = std::time::Instant::now();
    let mut clock = audio::PlayoutClock::new();
    let mut sinks: BTreeMap<u64, (u32, AudioSink)> = BTreeMap::new();
    loop {
        let turns = clock.due(started.elapsed().as_millis() as u64);
        let mut out: Vec<(u64, u32, Vec<i16>)> = Vec::new();
        let (play, live) = {
            let Some(strong) = shared.upgrade() else {
                return;
            };
            let mut playout = lock(&strong);
            for _ in 0..turns {
                // What this turn plays at the microphone's rate, mixed: the echo canceller's far
                // end (peers at another rate play uncancelled - there is no resampler).
                let mut mix: Vec<f32> = Vec::new();
                for (handle, jitter) in playout.peers.iter_mut() {
                    if let Some(samples) = jitter.pop() {
                        if jitter.sample_rate() == MIC_RATE {
                            if mix.len() < samples.len() {
                                mix.resize(samples.len(), 0.0);
                            }
                            for (m, v) in mix.iter_mut().zip(&samples) {
                                *m += audio::from_pcm16(*v);
                            }
                        }
                        out.push((*handle, jitter.sample_rate(), samples));
                    }
                }
                // The sound check plays like one more peer, and is cancelled from the
                // microphone like one: in a call nobody hears the test clip a second time.
                let check_rate = playout.check_rate;
                if let Some(samples) = playout.check.pop_front() {
                    if check_rate == MIC_RATE {
                        if mix.len() < samples.len() {
                            mix.resize(samples.len(), 0.0);
                        }
                        for (m, v) in mix.iter_mut().zip(&samples) {
                            *m += audio::from_pcm16(*v);
                        }
                    }
                    out.push((SOUND_CHECK_SINK, check_rate, samples));
                }
                if let (Some(echo), false) = (playout.echo.as_ref().map(|e| &e.0), mix.is_empty()) {
                    let _ = echo.far_end(AudioFrame {
                        sample_rate: MIC_RATE,
                        channels: 1,
                        samples: F32Vec::from(mix),
                    });
                }
            }
            let play = playout.play;
            let mut live: BTreeSet<u64> = playout.peers.keys().copied().collect();
            // The sound check's sink stays open while it has packets left, and for the turn
            // that played its last one.
            if !playout.check.is_empty() || out.iter().any(|(key, _, _)| *key == SOUND_CHECK_SINK) {
                live.insert(SOUND_CHECK_SINK);
            }
            (play, live)
        };
        // Close the sinks of peers that are gone, outside the lock.
        sinks.retain(|handle, _| live.contains(handle));
        if play {
            for (handle, rate, samples) in out {
                let reopen = sinks
                    .get(&handle)
                    .map_or(true, |(open_rate, _)| *open_rate != rate);
                if reopen {
                    let sink = AudioSink::open(AudioConfig {
                        sample_rate: rate,
                        channels: 1,
                    });
                    eprintln!(
                        "[azmeet] playing connection {handle} at {rate} Hz: {}",
                        if sink.is_open() {
                            "output open"
                        } else {
                            "no audio output"
                        }
                    );
                    sinks.insert(handle, (rate, sink));
                }
                if let Some((_, sink)) = sinks.get(&handle) {
                    sink.play(AudioFrame {
                        sample_rate: rate,
                        channels: 1,
                        samples: F32Vec::from(audio::pcm_to_f32(&samples)),
                    });
                }
            }
        }
        let wait = clock.wait_ms(started.elapsed().as_millis() as u64);
        std::thread::sleep(std::time::Duration::from_millis(wait.max(1)));
    }
}

/// Whether this run may open devices (microphone, speakers, camera, screen): not under
/// `AZ_BACKEND=headless`, where a test must never capture or play anything real.
pub(crate) fn devices_allowed() -> bool {
    std::env::var("AZ_BACKEND").map_or(true, |backend| {
        !backend.trim().eq_ignore_ascii_case("headless")
    })
}

/// The audio settings of this run: `--test-tone` (`AZMEET_TEST_TONE=1`) makes the microphone a
/// tone and starts it unmuted; a headless run uses the tone too (muted until switched on) and
/// plays nothing.
pub(crate) fn configure_audio(s: &mut MeetState) {
    let devices = devices_allowed();
    let tone = setting_on("AZMEET_TEST_TONE");
    s.tone_mic = tone || !devices;
    s.play_audio = devices;
    if tone {
        s.mic_on = true;
    }
    sync_mic(s);
    if !devices {
        eprintln!(
            "[azmeet] {}: no audio device is opened: the microphone is a {TONE_HZ} Hz test tone and \
             received audio is counted, not played",
            s.name
        );
    } else if tone {
        eprintln!(
            "[azmeet] {}: the microphone is a {TONE_HZ} Hz test tone (--test-tone)",
            s.name
        );
    }
}

/// Starts or stops the test tone with the microphone (and while the sound check records); a
/// mute forgets the unfinished packet.
pub(crate) fn sync_mic(s: &mut MeetState) {
    if !s.mic_on {
        s.packetizer.reset();
        s.opus_framer.reset();
    }
    if !s.mic_on && !s.sound_check.records() {
        s.tone = None;
    } else if s.tone_mic && s.tone.is_none() {
        s.tone = Some((
            audio::ToneSource::new(TONE_HZ, MIC_RATE),
            std::time::Instant::now(),
        ));
    }
}

/// Sends captured audio as 20 ms packets on the audio track to whom the plan says: everyone in the
/// full mesh, the backbone parent for a leaf. As Opus (about 32 kbit/s) when this side encodes it
/// and every peer decodes it (`audio::send_opus`; forwarders pass one stream on), else as 16-bit
/// PCM.
pub(crate) fn send_audio(s: &mut MeetState, sample_rate: u32, channels: u16, samples: &[f32]) {
    if !s.mic_on {
        return;
    }
    if s.remotes.is_empty() {
        // Nobody listens: the first frame to the next peer starts with fresh audio.
        s.packetizer.reset();
        s.opus_framer.reset();
        return;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        return;
    };
    let targets = handles_of(s, &s.plan.children(s.me, s.me));
    // The echo of what this side plays, removed (mono from here on).
    let cleaned = cancel_echo(s, sample_rate, channels, samples);
    let (channels, samples) = match cleaned.as_deref() {
        Some(mono) => (1, mono),
        None => (channels, samples),
    };
    let peers_opus: Vec<Option<bool>> = s.remotes.iter().map(|r| r.opus).collect();
    let opus = audio::send_opus(opus_encoder(s, sample_rate).is_some(), &peers_opus);
    let codec = if opus {
        audio::CODEC_OPUS
    } else {
        audio::CODEC_PCM16
    };
    if s.audio_codec_out != Some(codec) {
        s.audio_codec_out = Some(codec);
        // For scripts: the codec this side's voice goes out in from now on.
        println!("AZMEET_AUDIO {}", if opus { "opus" } else { "pcm" });
    }
    let frames = if opus {
        // Should PCM come back (a peer without Opus joins), it starts with fresh audio.
        s.packetizer.reset();
        opus_frames(s, sample_rate, channels, samples)
    } else {
        s.opus_framer.reset();
        s.packetizer.push(sample_rate, channels, samples)
    };
    for frame in frames {
        for handle in &targets {
            endpoint.send_frame(*handle, AUDIO_TRACK, frame.clone());
        }
    }
}

/// The microphone (`samples`, interleaved `channels`) mixed down to mono with the echo of what
/// this side plays removed; `None` when no echo canceller runs (nothing plays on a device, it is
/// turned off) - then the microphone goes out as it is.
pub(crate) fn cancel_echo(
    s: &MeetState,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Option<Vec<f32>> {
    let shared = s.playout.as_ref()?;
    let playout = lock(shared);
    let echo = &playout.echo.as_ref()?.0;
    let cleaned = echo.process(AudioFrame {
        sample_rate,
        channels: 1,
        samples: F32Vec::from(audio::mix_to_mono(channels, samples)),
    });
    let mono: &[f32] = cleaned.samples.as_ref();
    Some(mono.to_vec())
}

/// This side's Opus encoder for mono audio at `sample_rate`, opened on first use at that rate;
/// `None` where it does not open (then it is not asked again at that rate).
pub(crate) fn opus_encoder(s: &mut MeetState, sample_rate: u32) -> Option<&mut AudioEncoder> {
    let stale = s
        .opus_out
        .as_ref()
        .map_or(true, |(rate, _)| *rate != sample_rate);
    if stale {
        let encoder = AudioEncoder::create(
            AudioConfig {
                sample_rate,
                channels: 1,
            },
            audio::OPUS_KBPS,
        );
        let open = encoder.is_open();
        s.opus_out = Some((sample_rate, open.then_some(encoder)));
    }
    s.opus_out
        .as_mut()
        .and_then(|(_, encoder)| encoder.as_mut())
}

/// `samples` mixed down to mono, through this side's Opus encoder: a frame (the new packet and
/// the two before it) for every packet it completes.
pub(crate) fn opus_frames(
    s: &mut MeetState,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Vec<Vec<u8>> {
    let mono = audio::mix_to_mono(channels, samples);
    let mut packets = Vec::new();
    if let Some(encoder) = opus_encoder(s, sample_rate) {
        let taken = encoder.encode(AudioFrame {
            sample_rate,
            channels: 1,
            samples: F32Vec::from(mono),
        });
        if taken {
            while let Some(packet) = encoder.recv_packet().into_option() {
                packets.push(packet.as_slice().to_vec());
            }
        }
    }
    packets
        .into_iter()
        .map(|packet| s.opus_framer.push(packet))
        .collect()
}

/// The packets of `origin`'s Opus frame this side has not decoded yet, decoded (in order, each
/// once: `audio::OpusOrder`) by that peer's own decoder into 48 kHz PCM packets.
pub(crate) fn decode_opus(s: &mut MeetState, origin: u64, bytes: &[u8]) -> Vec<audio::Packet> {
    let Some(packets) = audio::decode_opus_frame(bytes) else {
        return Vec::new();
    };
    let (decoder, order) = s.opus_in.entry(origin).or_insert_with(|| {
        let decoder = AudioDecoder::create(AudioConfig {
            sample_rate: audio::OPUS_RATE,
            channels: 1,
        });
        (decoder, audio::OpusOrder::new())
    });
    let mut decoded = Vec::new();
    for packet in packets {
        if !order.take(packet.sequence) {
            continue;
        }
        let Some(frame) = decoder.decode(U8Vec::from(packet.data)).into_option() else {
            continue;
        };
        let samples: &[f32] = frame.samples.as_ref();
        decoded.push(audio::Packet {
            sequence: packet.sequence,
            samples: samples
                .iter()
                .map(|sample| audio::to_pcm16(*sample))
                .collect(),
        });
    }
    decoded
}

/// Takes a frame of `origin`'s audio (from it directly, or passed on; Opus or 16-bit PCM) into its
/// jitter buffer. Returns whether the active speaker changed (the speaker view's stage).
pub(crate) fn receive_audio(s: &mut MeetState, origin: u64, bytes: &[u8]) -> bool {
    if !s.remotes.iter().any(|r| r.key == origin) {
        return false;
    }
    let codec = audio::frame_codec(bytes).unwrap_or(audio::CODEC_PCM16);
    let (sample_rate, packets) = if codec == audio::CODEC_OPUS {
        (audio::OPUS_RATE, decode_opus(s, origin, bytes))
    } else {
        match audio::decode_frame(bytes) {
            Some(wire) => (wire.sample_rate, wire.packets),
            None => return false,
        }
    };
    // The two codecs number their packets apart: a switch starts the jitter buffer over.
    let switched = s
        .audio_codec_in
        .insert(origin, codec)
        .is_some_and(|before| before != codec);
    if switched {
        if let Some(shared) = s.playout.as_ref() {
            lock(shared).peers.remove(&origin);
        }
    }
    // Who speaks: the newest packet's level (a frame repeats the two before it). Measured even
    // while deafened, so the stage still follows the conversation.
    let Some(newest) = packets.last() else {
        return false;
    };
    let now = now_ms(s);
    let level = speaker::level_db(&newest.samples);
    let stage_moved = s.speaker.observe(origin, level, now);
    if s.deafened {
        return stage_moved;
    }
    let play = s.play_audio;
    let shared = s.playout.get_or_insert_with(|| start_playout(play));
    let mut playout = lock(shared);
    let jitter = playout
        .peers
        .entry(origin)
        .or_insert_with(|| audio::JitterBuffer::new(audio::TARGET_PACKETS, audio::MAX_PACKETS));
    for packet in packets {
        jitter.push(sample_rate, packet);
    }
    stage_moved
}

/// The active speaker changed: in the speaker view the stage (and with it what this side asks
/// for) moves; in the gallery nothing on screen does. True when the window changes.
pub(crate) fn speaker_moved(s: &mut MeetState) -> bool {
    if let Some(key) = s.speaker.current() {
        eprintln!("[azmeet] {}: {} is speaking", s.name, name_of(s, key));
    }
    if s.view != ViewMode::Speaker {
        return false;
    }
    for r in s.remotes.iter_mut() {
        // Until the tiles are laid out again, their new boxes' heights count.
        r.tile_height = [None; 2];
        r.tile_px = [None; 2];
    }
    network_changed(s, false);
    true
}

/// Forgets the received audio of the peer with key `origin` (it left), or everyone's: its jitter
/// buffer, and its Opus decoder (a peer that comes back numbers its packets from scratch).
pub(crate) fn drop_audio(s: &mut MeetState, origin: Option<u64>) {
    match origin {
        Some(origin) => {
            s.opus_in.remove(&origin);
            s.audio_codec_in.remove(&origin);
        }
        None => {
            s.opus_in.clear();
            s.audio_codec_in.clear();
        }
    }
    let Some(shared) = s.playout.as_ref() else {
        return;
    };
    let mut playout = lock(shared);
    match origin {
        Some(origin) => {
            playout.peers.remove(&origin);
        }
        None => playout.peers.clear(),
    }
}

/// This participant's audio state, as its control message tells the others.
pub(crate) fn my_state(s: &MeetState) -> audio::PeerState {
    audio::PeerState {
        muted: !s.mic_on,
        deafened: s.deafened,
    }
}

/// Sends `message` to the peers behind `handles`, reliably and in order.
pub(crate) fn send_message_to(s: &MeetState, handles: &[u64], message: &[u8]) {
    let Some(endpoint) = s.endpoint.as_ref() else {
        return;
    };
    for handle in handles {
        endpoint.send_message(*handle, message.to_vec());
    }
}

/// The connection handles of every connected peer.
pub(crate) fn all_peers(s: &MeetState) -> Vec<u64> {
    s.remotes.iter().map(|r| r.handle).collect()
}

/// Tells the peers behind `handles` this participant's audio state.
pub(crate) fn send_state(s: &MeetState, handles: &[u64]) {
    send_message_to(s, handles, &audio::encode_state(my_state(s)));
}

pub(crate) fn send_state_to_all(s: &MeetState) {
    send_state(s, &all_peers(s));
}
