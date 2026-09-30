# L2_MEDIA_LIB progress

Branch `wt/l2-media-lib` from `748f999af`.

## DONE

- `387fcb813` test(media): a headless run never opens a real microphone, camera, screen or audio output (RED)
- `24dc762a3` feat(media): a headless run opens no real device, only the stand-ins it asks for
- `813b2ebc8` test(video): a codec handle is open only where this build encodes / decodes (RED)
- `343b67db2` fix(video): codec handles open only where this build encodes / decodes
  (resumed after a power loss: the uncommitted half-edit of mod.rs was the encoder side; the decoder side,
  provision, capability and stream were finished before this commit)
- `fe25d628e` test(iroh): a prioritised track outranks messages, and the message backlog is reported (RED)
- `d737e6914` feat(iroh): per-track and message send priorities, and the outgoing message backlog
- report `scripts/L2_MEDIA_LIB_2026_09_29.md`

## IN PROGRESS

- nothing

## NEXT

- parent: api.json autofix (IrohPeerStats fields are REQUIRED), compile, run the report's test commands

## Open questions

- headless permission requests (`permission::apply_diff_events`): which state to report (see report, left 1)
