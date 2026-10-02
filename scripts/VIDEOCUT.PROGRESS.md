# VIDEOCUT progress (branch wt/videocut from 16d19442c)

Brief: scratchpad VIDEOCUT_go.md; house rules wave4_common.md (no cargo, no LSP, RED first,
commit every unit, keep this file exact). Parse check: scratchpad vc_parse.sh <files>.
Commit messages: bash heredoc into scratchpad vc_msg2.txt, then `git -C <wt> commit -F` as its own
command. Never put the word git inside a python heredoc (the sandbox refuses the command).

## STATUS: DONE (report committed: scripts/VIDEOCUT_2026_10_01.md)

## DONE
- 8b34d1c62 RED / e3d33ba36 GREEN: Mp4Demuxer + Mp4Muxer (dll video_codec/container.rs), VideoChunk
  in core; annexb_nals + append_avcc_as_annexb unified (VT + demux import them).
- beb8ad0ef RED / 6ec784a88 GREEN: VideoEncoder::encode_at (own timestamps for export).
- c66eb59cd RED / 994c107cb..7f050f86f, 44a07849a GREEN: the Timeline widget (types, time math,
  callbacks, build, flat + flora looks appended, manifest + CHROME group, controlled keys).
- b4068a34c RED / 0de07e40d..a48b4d348 GREEN: examples/azul-videocut (model, render, export,
  decode, store, args, sample, lib.rs state / jobs / playback / layout / callbacks / start).
- 0a5978e8e registration; 6ee874029 scripts/azvideocut_e2e.py; the report; from_vec fix.

## NEXT (for the parent)
- api.json via autofix (list in the report), build, the suites, the AzVideoCut build, the E2E.

## OPEN QUESTIONS
- none
