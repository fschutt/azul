---
slug: system/sounds
title: Sounds
language: en
canonical_slug: system/sounds
audience: external
maturity: beta
guide_order: 124
topic_only: false
short_desc: Ask for the sound the platform itself plays for a meaning - a tab change, an error, a notification - and see what became of it
prerequisites: [events, events/callbacks]
tracked_files:
  - core/src/sound.rs
  - layout/src/callbacks.rs
  - layout/src/e2e/full.rs
  - dll/src/desktop/shell2/common/event.rs
  - dll/src/desktop/shell2/android/sounds.rs
  - scripts/android/AzulSounds.java
  - dll/src/desktop/notifications/android.rs
  - dll/src/desktop/notifications/apple.rs
last_generated_rev: a4d19ee5b1a7dd7adb6d84409c56050815f4ec84
generated_at: 2026-11-28T00:00:00Z
default-search-keys:
  - SystemSound
  - play_system_sound
  - get_sound_route
  - get_sound_log
  - set_sounds_muted
---

# Sounds

## Introduction

An app asks for a sound by what it MEANS, and the platform plays what it itself plays for that:

```rust
info.play_system_sound(SystemSound::TabChange);
```

No audio file is bundled and none is played: a tab change on Android is the click of the "Touch sounds" setting, an error is the system's "invalid key" sound, a notification is the tone the user chose for notifications, and a meaning the platform has no sound for plays nothing and SAYS so. The vocabulary is the one of [haptics](../events/pointer.md) (`play_haptic`), and the two pair: on iOS, where a tab change has no sound by design, the same call is the selection tap.

A callback pays one push for the call. The sound is decided and played at the end of the outermost event pass, so a callback that fires per page of a fling or per frame of a drag costs nothing and does not stack.

## The meanings

`SystemSound` has two classes. Input feedback is what the user's own touch causes; the OS has a switch for it. Events are things that happened.

| class | sounds |
|---|---|
| input feedback | `Click`, `Tick`, `ToggleOn`, `ToggleOff`, `MenuOpen`, `MenuClose`, `Expand`, `Collapse`, `TabChange`, `Back`, `DragStart`, `DropAccept`, `DropReject` |
| event | `Notification`, `Success`, `Info`, `Question`, `Warning`, `Error`, `Bell`, `Delete`, `TrashEmpty`, `Send`, `CameraShutter`, `ScreenCapture` |

A meaning that has no route on a platform walks a ladder of the nearest meaning (`DropReject` is an `Error`, `TrashEmpty` a `Delete`, `TabChange` a `Tick`, a `Tick` a `Click`) before it gives up.

## What plays where

| sound | Android | iOS | macOS | Windows | Linux |
|---|---|---|---|---|---|
| `Click`, `ToggleOn`, `ToggleOff`, `TabChange`, `DropAccept` | `AudioManager.FX_KEY_CLICK`, the click of "Touch sounds" | selection tap for the toggles and the tab change, nothing for the click | none | none | none |
| `Tick` | `AudioManager.FX_FOCUS_NAVIGATION_DOWN` (the same file as the click) | selection tap | none | none | none |
| `Back` | `AudioManager.FX_BACK` (Android 12), the click before | none | none | none | none |
| `Notification`, `Success`, `Info`, `Question` | the user's notification tone, played as a notification is (`RingtoneManager`, usage `USAGE_NOTIFICATION`) | success tap for `Success` only | none | `Info` and `Question`: the sound scheme's Asterisk and Question (`MessageBeep`) | none |
| `Warning`, `Error`, `Bell`, `DropReject` | `AudioManager.FX_KEYPRESS_INVALID` | warning or error tap | the alert sound of System Settings (`Bell`: the system beep) | the scheme's Exclamation (`Warning`), Critical Stop (`Error`, `DropReject`), the default beep (`Bell`) | none |
| `CameraShutter`, `ScreenCapture` | `MediaActionSound.SHUTTER_CLICK` | none | none | none | none |
| `MenuOpen`, `MenuClose`, `Expand`, `Collapse`, `DragStart`, `Delete`, `TrashEmpty`, `Send` | none | none | none | none | none |

"None" is an answer, not a stub: the platform has no sound of its own for that meaning, no sound is bundled, and the request is recorded as `none:no-system-route`. The macOS alert sound honours the alert volume and the device chosen for alerts, and flashes the screen when the user asked for that; the Windows sounds are the user's sound scheme, so "No Sounds" silences them and nothing is written to the registry. The Linux route (libcanberra and the freedesktop sound theme) needs a connection to the sound server, which belongs to the window, and is not built yet.

What is silent by design, and why:

- iOS plays no sound for a UI event: Apple's guidance is the haptic, `AudioServicesPlaySystemSound` plays only files the app bundles, and the sounds built into the system have undocumented ids that Apple says are not available to an app. A sound of the app's own would also have to say what it does to the user's music: it must not interrupt it, which the system-sound channel does not and the audio engine's default category does.
- A desktop button does not click: Windows, macOS and KDE do not play a sound for a widget, and GNOME's `input-feedback-sounds` is off by default. No widget of the toolkit asks for a sound of its own yet; an app that wants one asks with `play_system_sound`.
- The app's own mute is for kiosks, tests and screen recordings (`set_sounds_muted`); the user's controls are the OS's.

## What gates a sound

In this order:

1. The app's mute (`set_sounds_muted(true)`). On Android the camera sounds survive it where the region requires the shutter sound (`MediaActionSound.mustPlayShutterSound()`).
2. The OS switch of the sound's class: on Android "Touch sounds" (`Settings.System.SOUND_EFFECTS_ENABLED`) for every `AudioManager` effect, and the ringer mode and Do Not Disturb for the notification tone. The window's style carries the switch (`SystemStyle.audio`); Android reads it at the start and at every resume.
3. The platform's route table.
4. The pacing: the same sound again within its minimum interval (40 ms for `Tick` and `TabChange`, 100 ms for the others) is dropped, identical requests of one pass are one sound, and an EVENT sound is not repeated more than twice in a row within a second. Input feedback has no repeat limit: a tick per page of a fling is the point.

## Asking what happened

`get_sound_route(sound)` says what a sound would do right now, as one piece of text: `system:android:FX_KEY_CLICK`, `ringtone:android:notification`, `media-action:android:SHUTTER_CLICK`, `haptic:Selection`, or `none:` and the reason (`none:no-system-route`, `none:app-muted`, `none:os:input-feedback-sounds-off`).

`get_sound_log()` returns the last requests of the window, one line each, with what became of them:

```text
#3 TabChange [input] route=system:android:FX_KEY_CLICK suppressed_by=- outcome=played
#4 TabChange [input] route=system:android:FX_KEY_CLICK suppressed_by=min-interval outcome=skipped:min-interval
```

The outcome of a request is `pending` until the shell has handed it to the platform at the end of the pass, so read it from the next callback. `played` means the platform call was made; `skipped:os:touch-sounds-off`, `skipped:os:ringer-silent` and `skipped:os:ringer-vibrate` are the system's own switches, read at the call.

`AZ_SOUNDS=log` prints one such line per request to the log, and `AZ_SOUNDS=off` records every request and plays none, also in a window that could (a screen recording, a CI machine with a display). On Android there is no environment: `adb shell setprop debug.az.sounds log`.

The AzWidgets demo has a "Sounds" section, the sound board: one button per sound and one per haptic pattern, each printing its route on the device in your hand and the switch that applied.

## Which sound is which on Android

`AudioManager.playSoundEffect(int)` plays one of the UI effects of the OS. The mapping of an id to a file is AOSP's audio_assets.xml of frameworks/base (read from `main`, from the tags `android-14.0.0_r1` and `android-15.0.0_r1`, and from the GrapheneOS branch of Android 17: it is the same in all of them), and `SoundEffectsHelper` looks the file up in `/product/media/audio/ui/` first and in `/system/media/audio/ui/` after.

| id | constant | since | file |
|---|---|---|---|
| 0 | `FX_KEY_CLICK` | API 1 | `Effect_Tick.ogg` |
| 1 to 4 | `FX_FOCUS_NAVIGATION_UP`, `_DOWN`, `_LEFT`, `_RIGHT` | API 1 | `Effect_Tick.ogg`, the same file as the click |
| 5 | `FX_KEYPRESS_STANDARD` | API 3 | `KeypressStandard.ogg` |
| 6 | `FX_KEYPRESS_SPACEBAR` | API 3 | `KeypressSpacebar.ogg` |
| 7 | `FX_KEYPRESS_DELETE` | API 3 | `KeypressDelete.ogg` |
| 8 | `FX_KEYPRESS_RETURN` | API 3 | `KeypressReturn.ogg` |
| 9 | `FX_KEYPRESS_INVALID` | API 19 | `KeypressInvalid.ogg` |
| 10 | `FX_BACK` | API 31 | `Effect_Tick.ogg`: it has a constant of its own and no file of its own |

An ordinary tap on a clickable view is id 0: `View.performClick` calls `playSoundEffect(SoundEffectConstants.CLICK)`, and `ViewRootImpl` turns it into `AudioManager.playSoundEffect(FX_KEY_CLICK)`. That short click is what "the system sound many apps play when you tap" is.

What gates it, from `AudioService`:

- The one-argument `playSoundEffect(int)` plays only when `Settings.System.SOUND_EFFECTS_ENABLED` is on (the setting reads 0 when its row is missing). The toggle is "Touch sounds" in Settings > Sound & vibration, and "Tap & click sounds" on GrapheneOS and on newer Pixels. The overload with a volume is for apps with a panel of their own and does not look at the setting.
- It does not play when the ringer is on vibrate or silent: the stream it plays on (`STREAM_SYSTEM`, usage `USAGE_ASSISTANCE_SONIFICATION`) is muted then.
- The call is a one-way binder call: it returns at once and can be made from any thread. Only the `View` and `ViewRootImpl` route has to be on the UI thread.

Other short system sounds that are not ids of this table: the lock and unlock sounds (`Lock.ogg`, `Unlock.ogg`, played by SystemUI, toggle "Screen locking sound"), the charging sound (`ChargingStarted.ogg`, "Charging sounds and vibration"), the dial pad tones (generated, "Dial pad tones") and the screenshot and camera sounds (`camera_click.ogg`). GrapheneOS changes none of the files or the default of "Touch sounds"; it makes the screenshot sound follow the same setting.

## Notifications

A sound that belongs to a notification is the notification's, not the app's: `Notification::with_sound` and `NotificationSound`. Do not ask for `SystemSound::Notification` as well when you post one: the post plays its own sound, and the system sound is for the in-app event that no notification shows.

- Android posts on a channel. The default channel has the importance "default" (sound, status bar, no heads-up) and the sound the user chose for notifications; a notification with `NotificationSound::Silent` goes to a second channel of low importance, because the sound of a channel cannot change after it was created. `NotificationSound::Named` has no meaning on Android and plays the channel's sound.
- iOS sets `UNNotificationSound.default` on the content unless the notification is silent, and asks for the sound authorization with the others; a notification that arrives while the app is in front is shown as a banner with its sound.

## Testing

Under `AZ_BACKEND=headless`, and in every scenario of the in-crate runner, the window records every request and plays nothing. `assert_sound` and `assert_no_sound` read the recording (see [End-to-end testing](../debugging/e2e-testing.md)); `count` counts the requests, which are the same on every platform, so one scenario runs unchanged on the desktop and on the phones:

```json
{ "op": "click", "text": "TabChange" },
{ "op": "wait_frame" },
{ "op": "assert_sound", "sound": "TabChange", "count": 1 }
```

The routes, the gates and the pacing are pure functions of the sound, the platform and the switches, tested on every host in `core/src/sound_test.rs`.
