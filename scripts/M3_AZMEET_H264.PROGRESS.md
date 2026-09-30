# M3 AzMeet H.264: progress

Branch `wt/m3-azmeet-h264` from `c1dbc55a1` (tip of `fix/input-bugs-2026-09-19`).

## DONE

- `3a9c08ad5` test(azmeet): the rules H.264 video travels by between participants (RED)
- `2dda5629b` feat(azmeet): packet header, keyframe requests and send window for video
- `07739df02` test(azmeet): two AzMeet processes see each other's video and recover from a lost packet (RED)

## IN PROGRESS

- GREEN `lib.rs` (NEXT 3).

## NEXT

3. GREEN `lib.rs`: probe H.264 at start, encoder per local track, decoder per remote track, packets as reliable
   messages (H.264) / latest-wins frames (JPEG), caps + keyframe request + acks, test pattern, drop button.
4. Guide + Cargo description, report `scripts/M3_AZMEET_H264_2026_09_29.md`.

## Findings so far

- `send_message` is the reliable, ordered path (one uni stream per direction, priority 1 > frames' 0, unbounded
  queue). Frames are latest-wins at both ends AND each frame is its own stream, so a big keyframe loses the race
  to the next small P-frame and is dropped as older on arrival: the frame path cannot carry H.264.
- `VideoEncoder::open` returns an OPEN handle that never yields a packet on Linux/Windows/Android (stub) and when
  VideoToolbox fails; `VideoDecoder::open` likewise. So AzMeet probes (encode a test frame, decode it back) and
  watches (encoder: 8 frames without a packet; decoder: 30 packets without a frame).
- VideoToolbox: synchronous encode (one Annex-B chunk per frame, SPS/PPS in-band ahead of IDRs),
  `force_keyframe` honoured (`kVTEncodeFrameOptionKey_ForceKeyFrame`), MaxKeyFrameInterval 60, no B-frames.

## Open questions

- none
