//! Windows media session - `SystemMediaTransportControls` (SMTC).
//!
//! # Why this exists when `WM_APPCOMMAND` already delivers the keys
//!
//! 9h-i was right that Windows needs nothing extra to RECEIVE media keys.
//! It has nothing to PUBLISH into, though: no title in the volume flyout, no
//! entry on the lock screen, no album art - which is what every other desktop
//! got from 9h-i-a-i and Windows did not.
//!
//! SMTC is that surface. A Win32 app reaches it through
//! `ISystemMediaTransportControlsInterop::GetForWindow(HWND)` rather than the
//! UWP `GetForCurrentView`, which needs a `CoreWindow` this app does not have.
//!
//! # Both transports report the same press, and that is handled in the channel
//!
//! Registering SMTC means the same physical play press can arrive as
//! `WM_APPCOMMAND`, as an SMTC `ButtonPressed`, or as both - and which of
//! those happens is a platform detail that cannot be settled from here.
//! Guessing either way is the dangerous option: subscribe and assume
//! `WM_APPCOMMAND` stops, and a press doubles (play, then immediately pause);
//! do not subscribe and assume it keeps arriving, and the keys go silent.
//!
//! So this subscribes AND leaves `WM_APPCOMMAND` alone, and
//! `push_media_key` drops a key that is already waiting in the current batch.
//! One press produces one key however many transports saw it, with no timer
//! and no assumption about which one fires.
//!
//! # Opt-in, like every other media-session backend
//!
//! `AppConfig::expose_system_media_controls` gates registration: appearing in
//! the volume flyout as a player is right for a music app and wrong for a text
//! editor.

use azul_core::{
    media_session::{MediaPlaybackState, NowPlayingInfo},
    window::VirtualKeyCode,
};
use azul_core::media_session::{MediaSeekKind, MediaSeekRequest};
use azul_css::AzString;
use azul_layout::managers::media_keys::{push_media_key, push_media_seek};
use windows::{
    core::{Interface, HSTRING},
    Media::{
        MediaPlaybackStatus, MediaPlaybackType, PlaybackPositionChangeRequestedEventArgs,
        SystemMediaTransportControls, SystemMediaTransportControlsButton,
        SystemMediaTransportControlsButtonPressedEventArgs,
        SystemMediaTransportControlsTimelineProperties,
    },
    Win32::{Foundation::HWND, System::WinRT::ISystemMediaTransportControlsInterop},
};

/// A WinRT object parked in a static. WinRT objects are agile - callable from
/// any apartment - which is what makes this sound; the sensor backend asserts
/// the same thing for the same reason.
struct Agile<T>(T);
unsafe impl<T> Send for Agile<T> {}
unsafe impl<T> Sync for Agile<T> {}

/// The registered controls, or empty on a machine or build where the
/// registration did not take.
static CONTROLS: std::sync::OnceLock<Agile<SystemMediaTransportControls>> =
    std::sync::OnceLock::new();

/// Claim the transport controls for this window.
///
/// Idempotent and quiet: SMTC is present on every supported Windows, but a
/// failure here means no media session, not a failure to run.
///
/// `hwnd` must be a real top-level window. The controls are attached to a
/// WINDOW, not to a process, which is why this cannot be started before one
/// exists - and why the caller hands its own handle in rather than this
/// guessing with `GetForegroundWindow`, which would attach us to whatever the
/// user happened to be looking at.
pub fn start(hwnd: isize) {
    if !azul_layout::window::expose_system_media_controls() {
        return;
    }
    if hwnd == 0 {
        return;
    }
    static STARTED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if STARTED.set(()).is_err() {
        return;
    }

    let built = (|| -> windows::core::Result<SystemMediaTransportControls> {
        let interop: ISystemMediaTransportControlsInterop =
            windows::core::factory::<SystemMediaTransportControls, ISystemMediaTransportControlsInterop>()?;
        let controls: SystemMediaTransportControls =
            unsafe { interop.GetForWindow(HWND(hwnd as *mut core::ffi::c_void))? };

        // WITHOUT `IsEnabled` NOTHING APPEARS. The controls exist as soon as
        // they are fetched, but stay invisible - and silently deliver no
        // button events - until enabled.
        controls.SetIsEnabled(true)?;
        controls.SetIsPlayEnabled(true)?;
        controls.SetIsPauseEnabled(true)?;
        controls.SetIsStopEnabled(true)?;
        controls.SetIsNextEnabled(true)?;
        controls.SetIsPreviousEnabled(true)?;

        let handler = windows::Foundation::TypedEventHandler::<
            SystemMediaTransportControls,
            SystemMediaTransportControlsButtonPressedEventArgs,
        >::new(|_sender, args| {
            if let Some(args) = args.as_ref() {
                if let Ok(button) = args.Button() {
                    if let Some(key) = button_to_key(button) {
                        // Parked, not handled here: this fires on a system
                        // thread and the engine's key pass belongs to the
                        // main one. Same channel MPRIS and
                        // `MPRemoteCommandCenter` use.
                        push_media_key(key);
                    }
                }
            }
            Ok(())
        });
        controls.ButtonPressed(&handler)?;

        // THE SEEK BAR (9h-i-a-i-a-i). `PlaybackPositionChangeRequested` fires
        // when the user drags the position in the flyout; the args carry
        // `RequestedPlaybackPosition`, a `TimeSpan` in 100-nanosecond ticks
        // (the same unit the timeline is published in), so microseconds are
        // ticks / 10. Registering the handler is what makes the bar draggable
        // - a timeline alone is read-only.
        let seek_handler = windows::Foundation::TypedEventHandler::<
            SystemMediaTransportControls,
            PlaybackPositionChangeRequestedEventArgs,
        >::new(|_sender, args| {
            if let Some(args) = args.as_ref() {
                if let Ok(position) = args.RequestedPlaybackPosition() {
                    push_media_seek(MediaSeekRequest {
                        kind: MediaSeekKind::Absolute,
                        position_us: position.Duration / 10,
                        uri: AzString::from_const_str(""),
                        track_id: AzString::from_const_str(""),
                    });
                }
            }
            Ok(())
        });
        controls.PlaybackPositionChangeRequested(&seek_handler)?;
        Ok(controls)
    })();

    match built {
        Ok(controls) => {
            crate::plog_info!("[media-keys] SMTC registered");
            let _ = CONTROLS.set(Agile(controls));
        }
        Err(e) => {
            crate::plog_info!("[media-keys] SMTC unavailable: {}", e);
        }
    }
}

/// Map an SMTC button onto the ordinary key every other media-key producer
/// delivers.
///
/// `Play` and `Pause` both become `PlayPause`, matching what the keysym table
/// and `WM_APPCOMMAND` already do: a keyboard's play button is a toggle, and
/// an app that bound the toggle must not miss the flyout's separate buttons.
/// The transport buttons this app never enables (record, seek, channel) map to
/// nothing rather than to a wrong key.
fn button_to_key(button: SystemMediaTransportControlsButton) -> Option<VirtualKeyCode> {
    Some(match button {
        SystemMediaTransportControlsButton::Play
        | SystemMediaTransportControlsButton::Pause => VirtualKeyCode::PlayPause,
        SystemMediaTransportControlsButton::Stop => VirtualKeyCode::MediaStop,
        SystemMediaTransportControlsButton::Next => VirtualKeyCode::NextTrack,
        SystemMediaTransportControlsButton::Previous => VirtualKeyCode::PrevTrack,
        _ => return None,
    })
}

/// Publish what the app is playing into the volume flyout and lock screen.
pub fn publish(info: &NowPlayingInfo) {
    let Some(controls) = CONTROLS.get() else {
        return;
    };
    let controls = &controls.0;

    let _ = controls.SetPlaybackStatus(match info.state {
        MediaPlaybackState::Playing => MediaPlaybackStatus::Playing,
        MediaPlaybackState::Paused => MediaPlaybackStatus::Paused,
        MediaPlaybackState::Stopped => MediaPlaybackStatus::Stopped,
    });

    let _ = (|| -> windows::core::Result<()> {
        let updater = controls.DisplayUpdater()?;
        // THE TYPE MUST BE SET BEFORE THE PROPERTIES. `MusicProperties` is
        // only writable once the updater knows it is describing music;
        // setting it afterwards clears what was just written.
        updater.SetType(MediaPlaybackType::Music)?;
        let music = updater.MusicProperties()?;
        music.SetTitle(&HSTRING::from(info.title.as_str()))?;
        music.SetArtist(&HSTRING::from(info.artist.as_str()))?;
        music.SetAlbumTitle(&HSTRING::from(info.album.as_str()))?;

        // ALBUM ART AS A URI, which is what `artwork_url` holds and what
        // MPRIS takes. Windows is the only other platform that accepts one -
        // macOS wants decoded pixels, which is why it drops the field
        // (9h-i-a-i-e). A URI that does not parse is skipped rather than
        // failing the whole update: a missing cover must not cost the title.
        if !info.artwork_url.as_str().is_empty() {
            if let Ok(uri) = windows::Foundation::Uri::CreateUri(&HSTRING::from(
                info.artwork_url.as_str(),
            )) {
                if let Ok(stream) =
                    windows::Storage::Streams::RandomAccessStreamReference::CreateFromUri(&uri)
                {
                    let _ = updater.SetThumbnail(&stream);
                }
            }
        }

        // NOTHING IS SHOWN UNTIL `Update`. Every setter above writes into a
        // staging buffer; forgetting this call is the classic SMTC bug where
        // the flyout keeps showing the previous track.
        updater.Update()?;
        Ok(())
    })();

    // The timeline is a SEPARATE call, not part of the display updater.
    let _ = (|| -> windows::core::Result<()> {
        let timeline = SystemMediaTransportControlsTimelineProperties::new()?;
        // WinRT `TimeSpan` counts 100-NANOSECOND ticks, not milliseconds:
        // publishing milliseconds here makes a 3-minute track show as 18
        // microseconds and the scrubber sit at zero.
        timeline.SetStartTime(ticks(0))?;
        timeline.SetMinSeekTime(ticks(0))?;
        timeline.SetEndTime(ticks(info.duration_ms))?;
        timeline.SetMaxSeekTime(ticks(info.duration_ms))?;
        timeline.SetPosition(ticks(info.position_ms))?;
        controls.UpdateTimelineProperties(&timeline)?;
        Ok(())
    })();
}

/// Milliseconds as a WinRT `TimeSpan` (100ns ticks).
fn ticks(ms: u64) -> windows::Foundation::TimeSpan {
    windows::Foundation::TimeSpan {
        Duration: azul_core::media_session::ms_to_winrt_ticks(ms),
    }
}
