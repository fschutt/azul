# AzMeet roadmap: recording, transcript, summary, mail

What a meeting leaves behind once it ends: the recording, a transcript made on this computer, a
summary, and that summary sent to the people who were there. None of it is built yet; this is the
plan. The settings page already has the Recording category, which says where recordings will go.

## Where it all goes: the meeting's folder

The cloud ruling (durable data are files, per user, in S3; the database only mints rooms and
holds transient state and access links) gives every meeting ONE folder, keyed by its room id:

```
meet/<room id>/meeting.json      the record (exists: link, server, joined, people)
meet/<room id>/chat.jsonl        the chat (exists)
meet/<room id>/recording/        audio.wav, video.mp4 (or one file once muxed), recording.json
meet/<room id>/transcript.jsonl  one line per utterance: start, end, speaker, text
meet/<room id>/transcript.vtt    the same as WebVTT, for a player
meet/<room id>/summary.md        the summary, as mailed
```

Today the folder lives under the local data root through azul-storage's `LocalDrive`
(`store::save`, one save thread). The Azlin storage is the same keys in the user's bucket: an
`S3Drive` (azul-storage `s3.rs`) with the user's credentials takes the `LocalDrive`'s place; the
keys do not change, so AzDrive browses a meeting's folder like any other.

- A recording goes up after the meeting, on a Thread, through azul-storage's `transfer.rs` (one
  object between a drive and a local file); the local copy stays until the upload is confirmed.
  Long recordings want a multipart, resumable upload, which `S3Drive` does not have yet.
- The `meet` Worker gets no recording tables: at most an access link (who may read
  `meet/<room id>/`) so the other participants can fetch the summary or the recording.

## 1. Recording

- Consent first: starting a recording sends a control message to every peer (a new
  `audio::Control` kind next to mute / deafen); every window shows "Recording" on the recorder's
  tile and in the people list. A peer that leaves stops being recorded.
- Audio: each peer's audio already arrives as its own stream (one jitter buffer per origin,
  `Playout` in `lib.rs`). The recorder writes the mix to `audio.wav` (48 kHz mono 16-bit PCM,
  plain Rust, no new crate) and keeps per-origin timestamps in `recording.json` - the transcript
  needs no speaker detection that way.
- Video: the active speaker's decoded stream (or this side's camera) goes through the H.264
  encoder that exists and azul's `Mp4Muxer` into `video.mp4`.
- Engine gap: `Mp4Muxer` writes an H.264 track only (`writeAnnexb`). An audio track (Opus or AAC)
  is needed for one playable file; until then audio and video are two files, with the offset in
  `recording.json`.
- Recording runs on its own Thread fed from the playout and decode paths; the UI thread only
  hands buffers over (the pump must not grow).

## 2. Transcript (local)

- A whisper-family model on this computer, never a cloud service by default. Candidates: a pure
  Rust whisper (candle's) or whisper.cpp behind a small crate. The model (base.en or small,
  about 150-500 MB) is downloaded once, on request, into the data root (`models/`), not per
  meeting; the Recording settings say which model and how big.
- Runs on an azul `Thread` after the meeting (or in 30 s chunks during it when the machine is
  idle enough): per origin, 16 kHz resampled, timestamps merged into `transcript.jsonl` with the
  speaker's name from `meeting.json` / the peer records.
- The window shows progress in the meeting's entry ("Transcribing, 40 %"); the transcript opens
  in a reading pane.

## 3. Summary

- Input: the transcript and the chat. Output: `summary.md` - decisions, action items with owners,
  open questions.
- Local first (a small instruction model on the same Thread machinery as the transcript); an
  optional cloud model through the Azlin Worker, off by default, named in the settings with what
  leaves the machine.

## 4. Mail the summary

- AzMail owns sending: `examples/azul-mail/src/send.rs` (`send_mail`: micromail's MIME builder,
  the outbox `<AzMail folder>/<account>/outbox/<id>.eml` + `<id>.json`, DKIM, the routes, filing
  in Sent).
- AzMeet hands a message to that outbox (the file contract, as everything else between the
  apps) rather than speaking SMTP itself; AzMail delivers it on its next send. Sharing the
  builder needs `send.rs` reachable from AzMeet: either the outbox format documented as a
  contract, or `send` split out of the azul-mail app crate into a small library crate.
- Recipients: the people of `meeting.json` have names only. Their addresses come from the
  meeting's calendar event (AzCalendar invitees) or AzContacts; the "Send summary" dialog shows
  them and lets the user edit the list before anything is queued.

## Order of work

1. The consent control message and the "Recording" indicator (RED test in `audio.rs` first).
2. `audio.wav` + `recording.json` on a Thread; `video.mp4` through `Mp4Muxer`.
3. Upload of the meeting folder to the S3 drive once the user's storage is set up.
4. The transcript on a Thread, the model download.
5. The summary; then the outbox hand-off to AzMail.
