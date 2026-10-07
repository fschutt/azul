//! MP4 -> H.264 Annex-B demuxer for the video decoder.
//!
//! gpu-video (Vulkan Video) decodes raw H.264 *elementary streams* (the Annex-B
//! byte-stream format) only — it does NOT parse MP4 containers. This module
//! pulls the H.264 track out of an MP4, converts its AVCC (4-byte-length-
//! prefixed NAL) samples into Annex-B (start-code-prefixed), and prepends the
//! SPS/PPS (from the `avcC` box) before each keyframe so a decoder can start on
//! any IDR. The output chunks feed straight into gpu-video's
//! `EncodedInputChunk { data, pts }`.
//!
//! Pure Rust (`mp4` crate), behind the `video-native` feature — unit-testable
//! on any machine, no GPU needed (which matters here: this box's NVK driver
//! exposes no Vulkan Video decode, so the gpu-video decode step is gated, but
//! the demux is fully verifiable).

use std::io::{Cursor, Read, Seek};

use mp4::{MediaType, Mp4Reader};

// AVCC -> Annex-B is the container module's (`container::append_avcc_as_annexb`):
// one rewrite for this demuxer and the VideoToolbox encoder's output.
use super::container::append_avcc_as_annexb;
use crate::desktop::extra::byte_source::{ByteSource, Wait};

/// Where one access unit of an H.264 track is in its file, and when it is
/// shown: what a player needs to schedule it without reading it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SampleInfo {
    /// Where its bytes are in the file, and how many.
    pub offset: u64,
    pub size: u32,
    /// Its presentation time in milliseconds.
    pub pts_ms: f64,
    pub is_keyframe: bool,
}

/// Why [`H264Index::chunk`] handed out no access unit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ChunkError {
    /// Its bytes have not arrived (a download): they are asked for, try again.
    NotYet,
    /// It cannot be read (the file ends inside it, a read failed): why.
    Failed(String),
}

/// An MP4's H.264 track as an INDEX: the stream's parameters and where every
/// access unit is, read from the container's header (`moov`, before or after
/// the media data) alone - the access units stay in the file until
/// [`chunk`](Self::chunk) reads one, so a player starts before a download
/// ends and never holds a movie in memory.
#[derive(Debug, Clone)]
pub(crate) struct H264Index {
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub sps: Vec<u8>,
    pub pps: Vec<u8>,
    /// Every access unit, in decode order.
    pub samples: Vec<SampleInfo>,
    /// A fragmented MP4 (`moof` boxes) is read whole: its access units.
    eager: Option<Vec<H264Chunk>>,
}

impl H264Index {
    /// The index of the H.264 track of the MP4 `reader` reads (`len` bytes).
    /// Reads the boxes before the media data and the `moov` wherever it is -
    /// never the media data itself (a fragmented MP4 is the exception: its
    /// access units are read whole).
    pub(crate) fn read<R: Read + Seek>(reader: R, len: u64) -> Result<H264Index, String> {
        let mut reader = Mp4Reader::read_header(reader, len)
            .map_err(|e| format!("mp4 header parse failed: {e}"))?;

        // The H.264/AVC video track: its parameters and its sample table.
        let mut found = None;
        for track in reader.tracks().values() {
            if track.media_type().ok() != Some(MediaType::H264) {
                continue;
            }
            let (Ok(sps), Ok(pps)) = (
                track.sequence_parameter_set(),
                track.picture_parameter_set(),
            ) else {
                continue;
            };
            let stbl = &track.trak.mdia.minf.stbl;
            let table = SampleTable {
                sizes: if stbl.stsz.sample_size > 0 {
                    vec![stbl.stsz.sample_size; stbl.stsz.sample_count as usize]
                } else {
                    stbl.stsz.sample_sizes.clone()
                },
                chunk_offsets: match (&stbl.stco, &stbl.co64) {
                    (Some(stco), _) => stco.entries.iter().map(|o| u64::from(*o)).collect(),
                    (None, Some(co64)) => co64.entries.clone(),
                    (None, None) => Vec::new(),
                },
                chunk_runs: stbl
                    .stsc
                    .entries
                    .iter()
                    .map(|e| (e.first_chunk, e.samples_per_chunk))
                    .collect(),
                deltas: stbl
                    .stts
                    .entries
                    .iter()
                    .map(|e| (e.sample_count, e.sample_delta))
                    .collect(),
                offsets: stbl.ctts.as_ref().map_or_else(Vec::new, |c| {
                    c.entries
                        .iter()
                        .map(|e| (e.sample_count, e.sample_offset))
                        .collect()
                }),
                sync: stbl.stss.as_ref().map(|s| s.entries.clone()),
            };
            found = Some((
                track.track_id(),
                u32::from(track.width()),
                u32::from(track.height()),
                sps.to_vec(),
                pps.to_vec(),
                track.sample_count(),
                f64::from(track.timescale().max(1)),
                track.frame_rate() as f32,
                !track.trafs.is_empty(),
                table,
            ));
            break;
        }
        let (track_id, width, height, sps, pps, sample_count, timescale, fps, fragmented, table) =
            found.ok_or_else(|| String::from("no H.264/AVC video track in MP4"))?;
        if sps.is_empty() || pps.is_empty() {
            return Err(String::from("H.264 track has no SPS/PPS in its avcC box"));
        }

        if fragmented {
            // `moof` fragments: the mp4 crate places their samples; read them whole.
            let mut chunks = Vec::with_capacity(sample_count as usize);
            for sid in 1..=sample_count {
                let sample = match reader.read_sample(track_id, sid) {
                    Ok(Some(s)) => s,
                    Ok(None) => continue,
                    Err(e) => return Err(format!("read_sample {sid} failed: {e}")),
                };
                chunks.push(H264Chunk {
                    annexb: annexb_access_unit(&sample.bytes, sample.is_sync, &sps, &pps),
                    pts_ms: presentation_ms(sample.start_time, sample.rendering_offset, timescale),
                    is_keyframe: sample.is_sync,
                });
            }
            let samples = chunks
                .iter()
                .map(|c| SampleInfo {
                    offset: 0,
                    size: 0,
                    pts_ms: c.pts_ms,
                    is_keyframe: c.is_keyframe,
                })
                .collect();
            return Ok(H264Index {
                width,
                height,
                fps,
                sps,
                pps,
                samples,
                eager: Some(chunks),
            });
        }

        let samples = table.samples(timescale)?;
        Ok(H264Index {
            width,
            height,
            fps,
            sps,
            pps,
            samples,
            eager: None,
        })
    }

    /// Access unit `index` (decode order) as Annex-B, the parameter sets in
    /// front of a keyframe, read from `source`: [`ChunkError::NotYet`] while
    /// its bytes have not arrived (with `Wait::No`; they are asked for).
    pub(crate) fn chunk(
        &self,
        source: &dyn ByteSource,
        index: usize,
        wait: Wait,
    ) -> Result<H264Chunk, ChunkError> {
        if let Some(chunks) = &self.eager {
            return chunks
                .get(index)
                .cloned()
                .ok_or_else(|| ChunkError::Failed(format!("there is no access unit {index}")));
        }
        let Some(sample) = self.samples.get(index) else {
            return Err(ChunkError::Failed(format!("there is no access unit {index}")));
        };
        let mut avcc = vec![0u8; sample.size as usize];
        let mut done = 0usize;
        while done < avcc.len() {
            match source.read_at(sample.offset + done as u64, &mut avcc[done..], wait) {
                Ok(0) => {
                    return Err(ChunkError::Failed(format!(
                        "the file ends inside access unit {index}"
                    )))
                }
                Ok(n) => done += n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    return Err(ChunkError::NotYet)
                }
                Err(e) => return Err(ChunkError::Failed(e.to_string())),
            }
        }
        Ok(H264Chunk {
            annexb: annexb_access_unit(&avcc, sample.is_keyframe, &self.sps, &self.pps),
            pts_ms: sample.pts_ms,
            is_keyframe: sample.is_keyframe,
        })
    }
}

/// One AVCC sample as an Annex-B access unit: the parameter sets in front of a
/// keyframe, so a decoder can start on it.
fn annexb_access_unit(avcc: &[u8], keyframe: bool, sps: &[u8], pps: &[u8]) -> Vec<u8> {
    let mut annexb = Vec::with_capacity(avcc.len() + 16 + sps.len() + pps.len());
    if keyframe {
        annexb.extend_from_slice(&START_CODE);
        annexb.extend_from_slice(sps);
        annexb.extend_from_slice(&START_CODE);
        annexb.extend_from_slice(pps);
    }
    append_avcc_as_annexb(avcc, &mut annexb);
    annexb
}

/// A track's sample table, as the `stbl` boxes say it (copied out: the mp4
/// crate's box types cannot be named here).
struct SampleTable {
    /// Every sample's size (`stsz`).
    sizes: Vec<u32>,
    /// Where each chunk starts in the file (`stco` / `co64`).
    chunk_offsets: Vec<u64>,
    /// `(first chunk, samples per chunk)` runs, 1-based chunks (`stsc`).
    chunk_runs: Vec<(u32, u32)>,
    /// `(count, ticks)` decode-time steps (`stts`).
    deltas: Vec<(u32, u32)>,
    /// `(count, ticks)` composition offsets (`ctts`; none without B-frames).
    offsets: Vec<(u32, i32)>,
    /// The 1-based numbers of the keyframes (`stss`); `None`: every sample is one.
    sync: Option<Vec<u32>>,
}

impl SampleTable {
    /// Every sample, in decode order: where it is, how long, when shown,
    /// whether a keyframe.
    fn samples(&self, timescale: f64) -> Result<Vec<SampleInfo>, String> {
        let count = self.sizes.len();
        // Offsets: each chunk holds the samples its run says, back to back.
        let mut offsets = Vec::with_capacity(count);
        'runs: for (i, &(first_chunk, per_chunk)) in self.chunk_runs.iter().enumerate() {
            let next_first = self
                .chunk_runs
                .get(i + 1)
                .map_or(self.chunk_offsets.len() as u64 + 1, |r| u64::from(r.0));
            for chunk in u64::from(first_chunk)..next_first {
                let Some(mut at) = chunk
                    .checked_sub(1)
                    .and_then(|c| self.chunk_offsets.get(c as usize))
                    .copied()
                else {
                    break 'runs;
                };
                for _ in 0..per_chunk {
                    let Some(size) = self.sizes.get(offsets.len()) else {
                        break 'runs;
                    };
                    offsets.push(at);
                    at += u64::from(*size);
                }
            }
        }
        if offsets.len() < count {
            return Err(format!(
                "the MP4's sample table places {} of its {count} access units",
                offsets.len()
            ));
        }
        // Decode times (`stts`); a table that ends early continues the last time.
        let mut decode = Vec::with_capacity(count);
        let mut t = 0u64;
        'deltas: for &(n, delta) in &self.deltas {
            for _ in 0..n {
                if decode.len() == count {
                    break 'deltas;
                }
                decode.push(t);
                t += u64::from(delta);
            }
        }
        decode.resize(count, t);
        // Composition offsets (`ctts`): when a decoded frame is SHOWN.
        let mut shown = vec![0i32; count];
        let mut i = 0usize;
        for &(n, offset) in &self.offsets {
            for _ in 0..n {
                if let Some(slot) = shown.get_mut(i) {
                    *slot = offset;
                }
                i += 1;
            }
        }
        // Keyframes (`stss`, 1-based); without the box every sample is one.
        let keyframe: Vec<bool> = match &self.sync {
            Some(numbers) => {
                let mut v = vec![false; count];
                for &n in numbers {
                    if let Some(slot) = (n as usize).checked_sub(1).and_then(|k| v.get_mut(k)) {
                        *slot = true;
                    }
                }
                v
            }
            None => vec![true; count],
        };
        Ok((0..count)
            .map(|k| SampleInfo {
                offset: offsets[k],
                size: self.sizes[k],
                pts_ms: presentation_ms(decode[k], shown[k], timescale),
                is_keyframe: keyframe[k],
            })
            .collect())
    }
}

/// 4-byte Annex-B start code, prefixed before every NAL unit.
const START_CODE: [u8; 4] = [0, 0, 0, 1];

/// One demuxed access unit (one frame's worth of NALs), Annex-B framed.
#[derive(Debug, Clone)]
pub struct H264Chunk {
    /// Annex-B bytes: start-code-prefixed NALs, with SPS+PPS prepended on
    /// keyframes so a decoder can start mid-stream.
    pub annexb: Vec<u8>,
    /// Presentation timestamp in milliseconds.
    pub pts_ms: f64,
    /// Whether this access unit is a keyframe (IDR).
    pub is_keyframe: bool,
}

/// A fully-demuxed H.264 elementary stream plus the metadata a player needs.
#[derive(Debug, Clone)]
pub struct DemuxedH264 {
    /// Coded picture width in pixels.
    pub width: u32,
    /// Coded picture height in pixels.
    pub height: u32,
    /// Nominal frame rate (fps), best-effort from the track.
    pub fps: f32,
    /// Sequence parameter set (raw NAL bytes, no start code).
    pub sps: Vec<u8>,
    /// Picture parameter set (raw NAL bytes, no start code).
    pub pps: Vec<u8>,
    /// Access units in decode order.
    pub chunks: Vec<H264Chunk>,
}

/// Demux an in-memory MP4 into an Annex-B H.264 stream.
///
/// Returns an error if the bytes aren't a parseable MP4 or carry no H.264/AVC
/// video track. Assumes the standard 4-byte NAL length prefix
/// (`lengthSizeMinusOne == 3`), which every browser/ffmpeg-produced MP4 uses.
pub fn demux_mp4_h264(mp4_bytes: &[u8]) -> Result<DemuxedH264, String> {
    let size = mp4_bytes.len() as u64;
    let mut reader = Mp4Reader::read_header(Cursor::new(mp4_bytes), size)
        .map_err(|e| format!("mp4 header parse failed: {e}"))?;

    // Locate the H.264/AVC video track and pull its config (SPS/PPS/dims) before
    // we start the mutable sample reads.
    let mut found = None;
    for track in reader.tracks().values() {
        if track.media_type().ok() != Some(MediaType::H264) {
            continue;
        }
        let sps = match track.sequence_parameter_set() {
            Ok(s) => s.to_vec(),
            Err(_) => continue,
        };
        let pps = match track.picture_parameter_set() {
            Ok(p) => p.to_vec(),
            Err(_) => continue,
        };
        found = Some((
            track.track_id(),
            track.width() as u32,
            track.height() as u32,
            sps,
            pps,
            track.sample_count(),
            track.timescale(),
            track.frame_rate() as f32,
        ));
        break;
    }
    let (track_id, width, height, sps, pps, sample_count, timescale, fps) =
        found.ok_or_else(|| String::from("no H.264/AVC video track in MP4"))?;
    if sps.is_empty() || pps.is_empty() {
        return Err(String::from("H.264 track has no SPS/PPS in its avcC box"));
    }
    let timescale = timescale.max(1) as f64;

    let mut chunks = Vec::with_capacity(sample_count as usize);
    // mp4 sample ids are 1-based.
    for sid in 1..=sample_count {
        let sample = match reader.read_sample(track_id, sid) {
            Ok(Some(s)) => s,
            Ok(None) => continue,
            Err(e) => return Err(format!("read_sample {sid} failed: {e}")),
        };
        let is_keyframe = sample.is_sync;
        let mut annexb = Vec::with_capacity(sample.bytes.len() + 16);
        if is_keyframe {
            annexb.extend_from_slice(&START_CODE);
            annexb.extend_from_slice(&sps);
            annexb.extend_from_slice(&START_CODE);
            annexb.extend_from_slice(&pps);
        }
        append_avcc_as_annexb(&sample.bytes, &mut annexb);
        let pts_ms = presentation_ms(sample.start_time, sample.rendering_offset, timescale);
        chunks.push(H264Chunk {
            annexb,
            pts_ms,
            is_keyframe,
        });
    }

    Ok(DemuxedH264 {
        width,
        height,
        fps,
        sps,
        pps,
        chunks,
    })
}

/// When a sample is SHOWN, in milliseconds: its decode time `start_time` plus
/// its composition offset (`ctts`), over the track's `timescale`.
fn presentation_ms(start_time: u64, rendering_offset: i32, timescale: f64) -> f64 {
    (start_time as f64 + f64::from(rendering_offset)) * 1000.0 / timescale
}

#[cfg(test)]
mod demux_tests {
    use super::*;

    /// Hand-built AVCC buffer (two NALs: lengths 3 and 2) converts to two
    /// start-code-prefixed NALs, no SPS/PPS prepend at this layer.
    #[test]
    fn avcc_to_annexb_splits_length_prefixed_nals() {
        // [len=3][AA BB CC][len=2][DD EE]
        let avcc = [0, 0, 0, 3, 0xAA, 0xBB, 0xCC, 0, 0, 0, 2, 0xDD, 0xEE];
        let mut out = Vec::new();
        append_avcc_as_annexb(&avcc, &mut out);
        assert_eq!(
            out,
            vec![
                0, 0, 0, 1, 0xAA, 0xBB, 0xCC, // first NAL
                0, 0, 0, 1, 0xDD, 0xEE, // second NAL
            ]
        );
    }

    /// A stream with B-frames DECODES a frame before it SHOWS it: the `ctts`
    /// offset is what moves it to its place. Big Buck Bunny's 360p clip has
    /// 194 `ctts` entries; without them its frames play in decode order.
    #[test]
    fn a_presentation_time_includes_the_composition_offset() {
        // 15360 ticks per second (BBB's timescale), 512 ticks per frame.
        assert_eq!(presentation_ms(0, 1024, 15360.0), 1024.0 * 1000.0 / 15360.0);
        assert_eq!(presentation_ms(512, 1536, 15360.0), 2048.0 * 1000.0 / 15360.0);
        assert_eq!(presentation_ms(1024, 0, 15360.0), 1024.0 * 1000.0 / 15360.0);
        assert_eq!(presentation_ms(1536, -512, 15360.0), 1024.0 * 1000.0 / 15360.0);
    }

    /// A truncated length prefix (claims 9 bytes, only 2 present) stops cleanly.
    #[test]
    fn avcc_to_annexb_tolerates_truncated_tail() {
        let avcc = [0, 0, 0, 9, 0x11, 0x22];
        let mut out = Vec::new();
        append_avcc_as_annexb(&avcc, &mut out);
        assert!(out.is_empty(), "an unsatisfiable length must emit nothing");
    }

    /// End-to-end against a real H.264 MP4 (the big-buck-bunny sample the user
    /// pointed at). Soft-skips when the sample isn't present so CI without the
    /// asset still passes.
    #[test]
    fn demux_big_buck_bunny_480p() {
        let path = "/tmp/video-media-samples/big-buck-bunny-480p-30sec.mp4";
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(_) => {
                eprintln!("[demux test] sample {path} absent — skipping");
                return;
            }
        };
        let d = demux_mp4_h264(&bytes).expect("demux must succeed on a valid H.264 MP4");

        assert_eq!(d.width, 854, "big-buck-bunny 480p is 854x480");
        assert_eq!(d.height, 480);
        assert!(d.fps > 20.0 && d.fps < 40.0, "≈30fps, got {}", d.fps);
        assert!(!d.sps.is_empty() && !d.pps.is_empty(), "SPS/PPS extracted");
        // SPS NAL header: forbidden_zero_bit 0 + nal_ref_idc + type 7 (SPS).
        assert_eq!(d.sps[0] & 0x1f, 7, "first SPS byte is a type-7 NAL");
        assert_eq!(d.pps[0] & 0x1f, 8, "first PPS byte is a type-8 NAL");

        assert!(
            d.chunks.len() > 100,
            "30s @30fps ≈ 900 frames, got {}",
            d.chunks.len()
        );
        let first = &d.chunks[0];
        assert!(first.is_keyframe, "first access unit must be an IDR");
        assert_eq!(&first.annexb[0..4], &START_CODE, "Annex-B framed");
        // Keyframe carries the prepended SPS (type 7) right after the start code.
        assert_eq!(first.annexb[4] & 0x1f, 7, "keyframe begins with SPS");
        let keyframes = d.chunks.iter().filter(|c| c.is_keyframe).count();
        assert!(keyframes >= 1, "at least one keyframe");
    }

    // ---- the index: a track read from its header, its access units on demand ----

    use std::sync::{Arc, Mutex, PoisonError};

    use crate::desktop::extra::byte_source::SourceReader;

    /// A sequence parameter set (NAL type 7) and a picture parameter set (type 8).
    const SPS: [u8; 8] = [0x67, 0x64, 0x00, 0x1f, 0xac, 0xd9, 0x40, 0x50];
    const PPS: [u8; 4] = [0x68, 0xeb, 0xe3, 0xcb];

    /// An access unit as an encoder hands it out (Annex-B, the parameter sets in
    /// front of a keyframe), and as the demuxer hands it back.
    fn access_unit(keyframe: bool, n: u8) -> Vec<u8> {
        let mut out = Vec::new();
        if keyframe {
            for nal in [&SPS[..], &PPS[..], &[0x65, 0x88, 0x84, n, n, n][..]] {
                out.extend_from_slice(&START_CODE);
                out.extend_from_slice(nal);
            }
        } else {
            out.extend_from_slice(&START_CODE);
            out.extend_from_slice(&[0x41, 0x9a, 0x02, n, n]);
        }
        out
    }

    /// Twelve frames at 25 fps, a keyframe every fourth, as azul's muxer writes
    /// them: the index (`moov`) AFTER the media data, like a file nobody
    /// prepared for streaming.
    fn twelve_frames() -> Vec<u8> {
        let mut muxer = super::super::container::Mp4Muxer::create(320, 180, 25.0);
        for i in 0..12_u8 {
            assert!(muxer.write_annexb(azul_css::U8Vec::from_vec(access_unit(i % 4 == 0, i))));
        }
        muxer.finish().as_ref().to_vec()
    }

    /// The top-level boxes of an MP4: (type, start, end).
    fn top_boxes(file: &[u8]) -> Vec<([u8; 4], usize, usize)> {
        let mut out = Vec::new();
        let mut at = 0usize;
        while at + 8 <= file.len() {
            let size = u32::from_be_bytes([file[at], file[at + 1], file[at + 2], file[at + 3]]);
            let kind = [file[at + 4], file[at + 5], file[at + 6], file[at + 7]];
            let end = if size == 0 { file.len() } else { at + size as usize };
            out.push((kind, at, end));
            if end <= at {
                break;
            }
            at = end;
        }
        out
    }

    /// A file of which only some byte ranges have arrived.
    struct Partial {
        bytes: Vec<u8>,
        here: Mutex<Vec<(usize, usize)>>,
    }

    impl Partial {
        fn arrive(&self, start: usize, end: usize) {
            self.here
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push((start, end));
        }
        /// The end of the arrived range `offset` is in.
        fn here_until(&self, offset: usize) -> Option<usize> {
            self.here
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .iter()
                .filter(|(s, e)| *s <= offset && offset < *e)
                .map(|(_, e)| *e)
                .max()
        }
    }

    impl ByteSource for Partial {
        fn byte_len(&self) -> Option<u64> {
            Some(self.bytes.len() as u64)
        }
        fn read_at(&self, offset: u64, buf: &mut [u8], _wait: Wait) -> std::io::Result<usize> {
            let offset = offset as usize;
            if offset >= self.bytes.len() {
                return Ok(0);
            }
            let Some(until) = self.here_until(offset) else {
                return Err(std::io::Error::from(std::io::ErrorKind::WouldBlock));
            };
            let n = buf.len().min(until - offset);
            buf[..n].copy_from_slice(&self.bytes[offset..offset + n]);
            Ok(n)
        }
        fn has(&self, offset: u64, len: u64) -> bool {
            self.here_until(offset as usize)
                .is_some_and(|until| offset + len <= until as u64)
        }
        fn name(&self) -> String {
            String::from("partial")
        }
    }

    /// A player starts on the header: the index comes from the `moov` (here at
    /// the END of the file) while the media data has not arrived; an access
    /// unit whose bytes are missing is "not yet", not a failure; once they
    /// arrive it is the stream's access unit, exactly as the whole-file
    /// demuxer hands it out.
    #[test]
    fn an_mp4_indexes_from_its_header_and_hands_a_chunk_out_only_when_its_bytes_are_here() {
        let file = twelve_frames();
        let boxes = top_boxes(&file);
        let at = |kind: &[u8; 4]| boxes.iter().position(|b| &b.0 == kind);
        let (_, mdat_start, mdat_end) = boxes[at(b"mdat").expect("an mdat")];
        assert!(at(b"moov") > at(b"mdat"), "the muxer writes the index last");
        let source = Arc::new(Partial {
            bytes: file.clone(),
            here: Mutex::new(Vec::new()),
        });
        // Everything but the media data: the file type, the media box's header,
        // the index at the end.
        source.arrive(0, mdat_start + 16);
        source.arrive(mdat_end, file.len());

        let index = H264Index::read(SourceReader::new(source.clone(), Wait::Yes), file.len() as u64)
            .expect("indexed from the header alone");
        assert_eq!((index.width, index.height), (320, 180));
        assert!((index.fps - 25.0).abs() < 0.01, "fps {}", index.fps);
        assert_eq!(index.samples.len(), 12);
        for (i, s) in index.samples.iter().enumerate() {
            assert_eq!(s.is_keyframe, i % 4 == 0, "sample {i}'s keyframe flag");
            assert!((s.pts_ms - 40.0 * i as f64).abs() < 0.01, "sample {i} at {}", s.pts_ms);
            assert!(
                s.offset >= mdat_start as u64 && s.offset + u64::from(s.size) <= mdat_end as u64,
                "sample {i} lies in the media data"
            );
        }
        assert_eq!(
            index.chunk(&*source, 0, Wait::No).map(|c| c.annexb),
            Err(ChunkError::NotYet),
            "its bytes have not arrived"
        );

        source.arrive(mdat_start, mdat_end);
        let whole = demux_mp4_h264(&file).expect("the whole file demuxes");
        assert_eq!(whole.chunks.len(), 12);
        for i in 0..12 {
            let chunk = index.chunk(&*source, i, Wait::No).expect("here now");
            assert_eq!(chunk.annexb, access_unit(i % 4 == 0, i as u8), "access unit {i}");
            assert_eq!(chunk.annexb, whole.chunks[i].annexb);
            assert_eq!(chunk.is_keyframe, whole.chunks[i].is_keyframe);
            assert!((chunk.pts_ms - whole.chunks[i].pts_ms).abs() < 1e-9);
        }
        assert!(matches!(index.chunk(&*source, 12, Wait::No), Err(ChunkError::Failed(_))));
    }

    /// The index of a real clip (B-frames: `ctts`; an edit list) places and
    /// times every access unit exactly as the mp4 crate's own reading of the
    /// whole file does. Soft-skips without the sample, like the test above.
    #[test]
    fn the_index_of_a_real_clip_matches_the_whole_file_demuxer() {
        let path = "/tmp/video-media-samples/big-buck-bunny-480p-30sec.mp4";
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!("[demux test] sample {path} absent - skipping");
            return;
        };
        let whole = demux_mp4_h264(&bytes).expect("the whole file demuxes");
        let source: Arc<dyn ByteSource> =
            Arc::new(crate::desktop::extra::byte_source::MemorySource::new(bytes.clone()));
        let index = H264Index::read(SourceReader::new(source.clone(), Wait::Yes), bytes.len() as u64)
            .expect("indexed");
        assert_eq!(index.samples.len(), whole.chunks.len());
        assert_eq!((index.width, index.height), (whole.width, whole.height));
        for (i, expected) in whole.chunks.iter().enumerate() {
            let chunk = index.chunk(&*source, i, Wait::Yes).expect("read");
            assert_eq!(chunk.annexb, expected.annexb, "access unit {i}");
            assert_eq!(chunk.is_keyframe, expected.is_keyframe, "access unit {i}");
            assert!((chunk.pts_ms - expected.pts_ms).abs() < 1e-9, "access unit {i}");
        }
    }
}
