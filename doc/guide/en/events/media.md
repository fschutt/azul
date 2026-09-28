---
slug: events/media
title: Media Playback
language: en
canonical_slug: events/media
audience: external
maturity: wip
guide_order: 69
topic_only: false
short_desc: Driving audio and video nodes, now-playing metadata, OS media keys and system audio
prerequisites: [hello-world, events, events/callbacks]
tracked_files:
  - layout/src/callbacks.rs
  - core/src/audio.rs
  - core/src/dom.rs
default-search-keys:
  - CallbackInfo
  - PlaybackState
  - NowPlayingInfo
  - MediaControlRequest
  - media_play
  - media_seek
  - set_now_playing
  - get_media_position
---

# Media Playback

`Dom::create_audio()` and `Dom::create_video()` make a media node. This
page is the control surface: a callback drives the node, reads its
state, and tells the OS what is playing.

## Introduction

A media node holds its own playback state. Every method here addresses
one by `DomNodeId`, so a page with three videos is three independent
players and no global "current track" exists unless you make one.
`create_audio()` and `create_video()` take accessibility info, because
a player nobody can announce is a player a screen reader user cannot
operate.

## Transport

```rust,ignore
info.media_play(node);
info.media_pause(node);
info.media_toggle(node);                 // whichever is the opposite
info.media_seek(node, 42.0);             // seconds, absolute
```

Reading back:

```rust,ignore
let playing  = info.is_media_playing(node);
let at       = info.get_media_position(node);   // seconds
let total    = info.get_media_duration(node);
let state    = info.get_media_state(node);      // OptionPlaybackState
```

`is_media_playing()` is the boolean shortcut; `get_media_state()` is
the full state - stopped, playing, paused, buffering, ended - which is
what a progress bar needs to distinguish "stalled" from "paused".

## Volume

```rust,ignore
info.media_set_volume(node, 0.5);   // 0.0 ..= 1.0
info.media_set_muted(node, true);
```

Mute is not volume zero, and the pair is kept separate deliberately:
unmuting restores the volume the user had set, so a mute button must
use `media_set_muted()` rather than saving and restoring a level
itself. `get_media_volume()` and `is_media_muted()` read both back.

## Driving a custom source

If you decode media yourself and push frames into the node, three
methods let you own the clock:

```rust,ignore
info.media_set_duration(node, total_seconds);  // once, when known
info.media_advance(node, dt);                  // per frame
info.media_report_error(node);                 // decode failed
```

`media_advance(node, dt)` moves the node's clock forward by a delta
rather than seeking to an absolute point, which is what keeps it in
step with a timer-driven decode loop. `media_report_error()` moves the
node into its error state so the UI and accessibility layer both see
that playback failed, instead of the node sitting silently at a
position that never advances.

## Telling the OS what is playing

```rust,ignore
info.set_now_playing(now_playing);
```

This populates the platform's media panel - macOS Now Playing, Windows
SMTC, MPRIS on Linux - with title, artist, artwork and duration. Set it
whenever the track changes, and it is what makes the lock screen and
the media keys show your app at all.

The keys then come back as requests:

```rust,ignore
if let Some(req) = info.get_media_control_request().into_option() {
    // play / pause / next / previous / seek, from a headset button
    // or the keyboard's media keys
}
```

These are *requests*, not commands: the OS is asking, and your app
decides. Ignoring one is legitimate - a request to skip when there is
no next track should do nothing rather than stop playback.

## System audio

`is_system_audio_active()` reports whether something else on the
machine is playing, and `get_system_audio_change()` fires when that
changes - a call starting, another app taking the output device. The
usual response is to duck or pause.

`set_system_audio_takeover(true)` declares that your app is now the
primary audio application, which is the other side of the same
negotiation.

## More methods

**Transport** - `media_play`, `media_pause`, `media_toggle`,
`media_seek`, `media_advance`.

**State** - `is_media_playing`, `get_media_state`,
`get_media_position`, `get_media_duration`, `media_set_duration`,
`media_report_error`.

**Volume** - `media_set_volume`, `get_media_volume`, `media_set_muted`,
`is_media_muted`.

**OS integration** - `set_now_playing`, `get_media_control_request`,
`is_system_audio_active`, `get_system_audio_change`,
`set_system_audio_takeover`.
