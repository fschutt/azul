# VIDEOCUT progress (branch wt/videocut from 16d19442c)

Brief: scratchpad VIDEOCUT_go.md; house rules wave4_common.md (no cargo, no LSP, RED first,
commit every unit, keep this file exact). Parse check: scratchpad vc_parse.sh <files>.
Commit messages: bash heredoc into scratchpad vc_msg2.txt, then `git -C <wt> commit -F` as its own
command (the Write tool refused vc_msg.txt once and a commit took an old message; amended locally).
Never put the word git inside a python heredoc (the sandbox refuses the command).

## DONE
- 8b34d1c62 RED / e3d33ba36 GREEN: Mp4Demuxer + Mp4Muxer (dll video_codec/container.rs), VideoChunk
  in core; annexb_nals + append_avcc_as_annexb unified (VT + demux import them).
- beb8ad0ef RED / 6ec784a88 GREEN: VideoEncoder::encode_at (own timestamps for export).
- c66eb59cd RED: timeline widget tests (layout/src/widgets/timeline_tests.rs).
- 994c107cb..7f050f86f GREEN timeline: types, time math, callbacks, build, flat + flora looks
  (appended), manifest entry + CHROME group.
- b4068a34c RED app: examples/azul-videocut skeleton (Cargo.toml, main.rs, lib.rs with modules
  args/decode/export/model/render/store, each = test include only) + *_tests.rs.

## DECISIONS
- MP4 container I/O lives in azul (NO DUPLICATION): video_codec/container.rs (Mp4Demuxer /
  Mp4Muxer over the `mp4` crate already in Cargo.lock, behind `video-native`).
- No encoder on the platform: export falls back to Y4M (uncompressed, plays in ffmpeg / VLC).
- Sample clips: synthetic generator media (Pattern::Bars / Matte / Sweep) always available; with
  an open VideoEncoder `--sample` also encodes two short MP4 clips into the project's media/.
- Data root: `AZVIDEOCUT_DATA` or `<FilePath::get_data_dir()>/azul`, keys `videocut/<uuid>/...`.
- Timeline widget API: see layout/src/widgets/timeline.rs (Timeline, TimelineTrack, TimelineClip,
  TimelineEvent{Kind}, TimelineEdge, TimelineClipTint, TimelineTrackKind, TimelineOnEvent*).

## IN PROGRESS
- GREEN app modules, one commit each, in this order: model.rs (Project/MediaItem/Sequence/Track/
  Clip/Effects/Transition/Edit/EditError/History, SourceMarks, clip_from_marks), render.rs (Canvas,
  generate, FrameSource, Generated, compose, scale_to), export.rs (rgba_to_i420, y4m_header,
  append_y4m_frame, ExportRange, export_frames, even_size, is_keyframe, timestamp_us, OutputFormat,
  output_name + the worker), decode.rs (feed_plan, media_ms, FrameCache, the reader over
  azul::video), store.rs (keys, save/load/list), args.rs; then lib.rs UI in pieces.

## NEXT
1. app GREEN modules (above), then lib.rs (state, layout on TimelineShell, callbacks, workers).
2. scripts/azvideocut_e2e.py; workspace registration (Cargo.toml members, workspace_test_members,
   rust.yml dll_tests step); report scripts/VIDEOCUT_2026_10_01.md.

## OPEN QUESTIONS
- none
