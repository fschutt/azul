// The system media session: what the lock screen, the notification shade and a
// Bluetooth headset's buttons talk to.
//
// This is the Android half of `azul_core::media_session` (9h-i-a-i-d). The
// Rust side publishes a NowPlayingInfo; this turns it into the MediaMetadata +
// PlaybackState pair Android wants, and forwards the transport buttons back as
// ordinary key codes.
//
// KEY CODE CONTRACT: the callbacks below hand `KeyEvent.KEYCODE_MEDIA_*`
// straight to `nativeOnMediaButton` - ANDROID's own constants, not an
// azul-side numbering. That direction cannot drift, because both sides name
// the same platform values.
//
// The `state` argument to `publish` is the opposite case: those ARE azul's
// `MediaPlaybackState` discriminants, so they are pinned by a test on the Rust
// side, exactly as the sensor kind codes are.

package com.azul.media;

import android.app.Activity;
import android.media.MediaMetadata;
import android.media.session.MediaSession;
import android.media.session.PlaybackState;
import android.view.KeyEvent;
import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioFocusRequest;
import android.media.AudioManager;

public final class AzulMediaSession {

    private AzulMediaSession() {}

    private static MediaSession session;
    /** Audio focus (9h-i-a-i-d-i): the platform's "take over the system audio". */
    private static AudioManager audioManager;
    private static AudioFocusRequest focusRequest;

    /**
     * Request audio focus for media playback. Returns 1 when granted now,
     * 2 when the system will grant it later (the listener reports the grant
     * as AUDIOFOCUS_GAIN), 0 when refused or unavailable.
     */
    public static int requestAudioFocus(Activity activity) {
        if (activity == null) {
            return 0;
        }
        try {
            if (audioManager == null) {
                audioManager = (AudioManager) activity.getSystemService(Context.AUDIO_SERVICE);
            }
            if (audioManager == null) {
                return 0;
            }
            if (focusRequest == null) {
                AudioAttributes attrs = new AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build();
                focusRequest = new AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                        .setAudioAttributes(attrs)
                        .setAcceptsDelayedFocusGain(true)
                        .setOnAudioFocusChangeListener(new AudioManager.OnAudioFocusChangeListener() {
                            @Override
                            public void onAudioFocusChange(int focusChange) {
                                nativeOnAudioFocusChange(focusChange);
                            }
                        })
                        .build();
            }
            int result = audioManager.requestAudioFocus(focusRequest);
            if (result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED) {
                return 1;
            }
            if (result == AudioManager.AUDIOFOCUS_REQUEST_DELAYED) {
                return 2;
            }
            return 0;
        } catch (Throwable t) {
            return 0;
        }
    }

    /** Give the audio back. Safe to call when none was ever requested. */
    public static void abandonAudioFocus() {
        try {
            if (audioManager != null && focusRequest != null) {
                audioManager.abandonAudioFocusRequest(focusRequest);
            }
        } catch (Throwable ignored) {
        }
    }

    /**
     * Claim a media session. Idempotent.
     *
     * Only called when the app opted in via
     * AppConfig::expose_system_media_controls - appearing on the lock screen
     * as a player is right for a music app and wrong for a text editor.
     */
    public static void start(Activity activity) {
        if (activity == null || session != null) {
            return;
        }
        try {
            session = new MediaSession(activity, "azul");
            // WITHOUT A CALLBACK THE SESSION IS INERT for buttons: Android
            // routes media keys to the active session's callback, and a
            // session with none simply swallows them.
            session.setCallback(new MediaSession.Callback() {
                @Override
                public void onPlay() {
                    nativeOnMediaButton(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE);
                }

                @Override
                public void onPause() {
                    nativeOnMediaButton(KeyEvent.KEYCODE_MEDIA_PLAY_PAUSE);
                }

                @Override
                public void onStop() {
                    nativeOnMediaButton(KeyEvent.KEYCODE_MEDIA_STOP);
                }

                @Override
                public void onSkipToNext() {
                    nativeOnMediaButton(KeyEvent.KEYCODE_MEDIA_NEXT);
                }

                @Override
                public void onSkipToPrevious() {
                    nativeOnMediaButton(KeyEvent.KEYCODE_MEDIA_PREVIOUS);
                }

                // THE SEEK BAR (9h-i-a-i-a-i). The system UI sends the new
                // position in MILLISECONDS; it only offers the bar at all when
                // ACTION_SEEK_TO is among the published actions (below).
                @Override
                public void onSeekTo(long pos) {
                    nativeOnMediaSeek(pos);
                }

                @Override
                public boolean onMediaButtonEvent(android.content.Intent intent) {
                    // A headset button arrives as a raw KeyEvent rather than
                    // through the typed callbacks above. Forwarding the DOWN
                    // only: the pair would otherwise report one press twice.
                    KeyEvent event = intent == null
                            ? null
                            : (KeyEvent) intent.getParcelableExtra(android.content.Intent.EXTRA_KEY_EVENT);
                    if (event != null && event.getAction() == KeyEvent.ACTION_DOWN) {
                        nativeOnMediaButton(event.getKeyCode());
                        return true;
                    }
                    return super.onMediaButtonEvent(intent);
                }
            });
            session.setActive(true);
        } catch (Throwable t) {
            session = null;
        }
    }

    /** Release the session. Safe to call when none was ever claimed. */
    public static void stop(Activity activity) {
        try {
            if (session != null) {
                session.setActive(false);
                session.release();
            }
        } catch (Throwable ignored) {
        } finally {
            session = null;
        }
    }

    /**
     * Publish what the app is playing.
     *
     * @param state 0 = stopped, 1 = playing, 2 = paused. These are
     *              MediaPlaybackState's own discriminants, pinned on the Rust
     *              side by a test, NOT Android's PlaybackState constants -
     *              which are different numbers and are mapped below.
     */
    public static void publish(String title, String artist, String album, String artUri,
                               long durationMs, long positionMs, int state) {
        if (session == null) {
            return;
        }
        try {
            MediaMetadata.Builder meta = new MediaMetadata.Builder()
                    .putString(MediaMetadata.METADATA_KEY_TITLE, title)
                    .putString(MediaMetadata.METADATA_KEY_ARTIST, artist)
                    .putString(MediaMetadata.METADATA_KEY_ALBUM, album)
                    // MILLISECONDS here, unlike MPRIS (microseconds) and
                    // WinRT (100ns ticks). Android is the one platform whose
                    // unit matches what NowPlayingInfo stores.
                    .putLong(MediaMetadata.METADATA_KEY_DURATION, durationMs);
            if (artUri != null && !artUri.isEmpty()) {
                // A URI, not a Bitmap: ART_URI is what lets the system fetch
                // and cache the image itself instead of the app decoding one.
                meta.putString(MediaMetadata.METADATA_KEY_ALBUM_ART_URI, artUri);
            }
            session.setMetadata(meta.build());

            int androidState;
            switch (state) {
                case 1: androidState = PlaybackState.STATE_PLAYING; break;
                case 2: androidState = PlaybackState.STATE_PAUSED; break;
                default: androidState = PlaybackState.STATE_STOPPED; break;
            }
            // THE ACTIONS ARE WHAT DRAW THE BUTTONS. A session that advertises
            // none shows a notification with no transport controls at all,
            // however many callbacks it implements.
            long actions = PlaybackState.ACTION_PLAY
                    | PlaybackState.ACTION_PAUSE
                    | PlaybackState.ACTION_PLAY_PAUSE
                    | PlaybackState.ACTION_STOP
                    | PlaybackState.ACTION_SKIP_TO_NEXT
                    | PlaybackState.ACTION_SKIP_TO_PREVIOUS
                    | PlaybackState.ACTION_SEEK_TO;
            // The SPEED matters: the system extrapolates the position from it
            // between updates, so 0 while playing freezes the progress bar.
            float speed = androidState == PlaybackState.STATE_PLAYING ? 1.0f : 0.0f;
            session.setPlaybackState(new PlaybackState.Builder()
                    .setActions(actions)
                    .setState(androidState, positionMs, speed)
                    .build());
        } catch (Throwable ignored) {
        }
    }

    private static native void nativeOnMediaButton(int keycode);
    private static native void nativeOnMediaSeek(long positionMs);
    private static native void nativeOnAudioFocusChange(int focusChange);
}
