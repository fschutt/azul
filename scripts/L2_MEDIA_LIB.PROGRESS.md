# L2_MEDIA_LIB progress

Branch `wt/l2-media-lib` from `748f999af`.

## DONE

- `387fcb813` test(media): a headless run never opens a real microphone, camera, screen or audio output (RED)
- `24dc762a3` feat(media): a headless run opens no real device, only the stand-ins it asks for
- `813b2ebc8` test(video): a codec handle is open only where this build encodes / decodes (RED)
- `343b67db2` fix(video): codec handles open only where this build encodes / decodes
  (resumed after a power loss: the uncommitted half-edit of mod.rs was the encoder side; the decoder side,
  provision, capability and stream were finished before this commit)

## IN PROGRESS

- 3. iroh priorities + message backlog (RED)

## NEXT

- 3. fix: `IrohEndpoint::set_track_priority` / `set_message_priority`, `IrohPeerStats.messages_queued` /
  `message_bytes_queued`
- report `scripts/L2_MEDIA_LIB_2026_09_29.md`

## Open questions

- none yet
