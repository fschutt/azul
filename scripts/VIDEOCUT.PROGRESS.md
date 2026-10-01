# VIDEOCUT progress (branch wt/videocut from 16d19442c)

Brief: scratchpad VIDEOCUT_go.md; house rules wave4_common.md (no cargo, no LSP, RED first).

## DONE
- 8b34d1c62 RED / e3d33ba36 GREEN: Mp4Demuxer + Mp4Muxer (dll video_codec/container.rs), VideoChunk
  in core; annexb_nals + append_avcc_as_annexb unified (VT + demux import them).
- beb8ad0ef RED / 6ec784a88 GREEN: VideoEncoder::encode_at (own timestamps for export).
- reading done: plan doc, SHELLS report, VIDEO_PATH / VIDEO_REVIEW, video api,
  dll video_codec (demux.rs is behind `video-native`, which build-dll turns on; `mp4` 0.14 has
  `Mp4Writer`), TimelineShell, AzDrive structure, message_list / info_bar / split_pane patterns.

## DECISIONS
- MP4 container I/O lives in azul (NO DUPLICATION): `dll/src/desktop/extra/video_codec/mux.rs`
  (Mp4Muxer, over the `mp4` crate already in Cargo.lock) next to demux.rs, and the demuxer is
  exposed per access unit (`Mp4Demuxer`) so an app can seek to a keyframe and decode forward.
  Handles always present (invalid without `video-native`), like VideoEncoder.
- No encoder on the platform: export falls back to Y4M (uncompressed, plays in ffmpeg / VLC).
- Sample clips: synthetic generator media ("color bars" / "color matte") always available; with
  an open VideoEncoder `--sample` also encodes two short MP4 clips into the project's media/.
- Data root: `AZVIDEOCUT_DATA` or `<FilePath::get_data_dir()>/azul`, keys `videocut/<uuid>/...`.

## IN PROGRESS
- layout timeline widget (RED tests next).

## NEXT
1. dll: mux.rs + Mp4Demuxer/Mp4Muxer handles (+ wasm / no-video-native stubs), RED then GREEN.
2. layout/src/widgets/timeline.rs + flat/flora looks + manifest, RED then GREEN.
3. examples/azul-videocut: model (edits + undo), render (composite), decode cache worker,
   export worker, storage, UI on TimelineShell, args, unit tests.
4. scripts/azvideocut_e2e.py; workspace registration; report.

## OPEN QUESTIONS
- none yet
