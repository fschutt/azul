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
- b4068a34c RED app: examples/azul-videocut skeleton + *_tests.rs.
- GREEN app: model, render, export, decode, store + args, sample, lib.rs pieces A..F (state, jobs,
  pictures + playback, layout, callbacks, start), a48b4d348 fix.
- 0a5978e8e registration (Cargo.toml member, workspace_test_members, rust.yml dll_tests step).
- 6ee874029 scripts/azvideocut_e2e.py.

## IN PROGRESS
- A careful compile-review pass of timeline.rs (callbacks + build) and lib.rs, then the report
  scripts/VIDEOCUT_2026_10_01.md (api.json list, least-sure spots, test commands, what is left).

## NEXT
1. Review pass (fix commits as needed).
2. Report + final progress commit.

## DECISIONS
- MP4 container I/O lives in azul (NO DUPLICATION): video_codec/container.rs (Mp4Demuxer /
  Mp4Muxer over the `mp4` crate already in Cargo.lock, behind `video-native`).
- No encoder on the platform: export falls back to Y4M (uncompressed, plays in ffmpeg / VLC).
- Sample clips: generated (Pattern::Bars / Matte / Sweep); with an open VideoEncoder `--sample`
  encodes the two sweeps to MP4 through the export path (run_export) into videocut/<uuid>/media/.
- Data root: `AZVIDEOCUT_DATA` or `<FilePath::get_data_dir()>/azul`, keys `videocut/<uuid>/...`.
- Short-lived worker jobs (no persistent decode thread): an idle editor runs no thread and no
  timer (the shell polls threads at the frame rate while any exists).

## OPEN QUESTIONS
- none
