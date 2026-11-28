// System sounds on Android: the Java half of dll/src/desktop/shell2/android/sounds.rs.
//
// Rust decides WHAT to play (azul_core::sound::route names the route of a sound and says
// whether the app's mute or an OS switch silences it) and calls ONE static method per route;
// this file does what only Java does well and answers with one line of text, which Rust turns
// into the outcome of the request (SoundOutcome::from_helper_line):
//
//   played            the platform call was made and accepted
//   skipped:<why>     not played on purpose: os:touch-sounds-off, os:ringer-silent,
//                     os:ringer-vibrate, app-muted-and-not-required
//   failed:<why>      the call failed or the constant does not exist on this Android version
//
// The routes, and what the system itself decides about each:
//
//   playEffect  AudioManager.playSoundEffect(FX_*): the UI sound effects of the OS (the "Touch
//               sounds" click). The constant is looked up by NAME (FX_BACK is API 31, FX_KEYPRESS_
//               INVALID API 19): an absent one degrades to the named fallback, never to a wrong
//               sound. AudioService applies Settings.System.SOUND_EFFECTS_ENABLED itself; it is
//               read here too, so that "the switch is off" is an answer and not a silence.
//   playTone    the user's own notification tone, through RingtoneManager, with the usage
//               USAGE_NOTIFICATION: the ringer mode and Do Not Disturb apply to it as to the
//               tone of a notification. A silent or vibrating phone plays nothing and says so.
//   playCamera  MediaActionSound (the shutter), with the shutter sound a region may require:
//               when the app is muted it still plays where MediaActionSound.mustPlayShutterSound()
//               says it must.
//
// No call here needs a permission. Lenient by construction: every entry point catches what it can
// and reports a failure as a string, because an exception escaping into NativeActivity takes the
// app down.

package com.azul.sound;

import android.app.Activity;
import android.content.Context;
import android.media.AudioAttributes;
import android.media.AudioManager;
import android.media.MediaActionSound;
import android.media.Ringtone;
import android.media.RingtoneManager;
import android.net.Uri;
import android.provider.Settings;

public final class AzulSounds {

    private AzulSounds() {}

    /** The tone that is playing, kept so that it is not collected before it ends. */
    private static Ringtone tone;
    /** The camera sounds, created on first use. */
    private static MediaActionSound camera;

    /**
     * Whether the user has "Touch sounds" on (Settings > Sound & vibration; "Tap & click sounds"
     * on GrapheneOS): Settings.System.SOUND_EFFECTS_ENABLED. A row that is missing reads as OFF,
     * as AudioService reads it (it asks with the default 0); true when it cannot be read at all:
     * AudioService applies the switch at the call anyway.
     */
    public static boolean touchSoundsEnabled(Activity activity) {
        try {
            return Settings.System.getInt(
                    activity.getContentResolver(), Settings.System.SOUND_EFFECTS_ENABLED, 0) != 0;
        } catch (Throwable t) {
            return true;
        }
    }

    /** The value of the public static int `name` of `owner`, or -1 when this Android has none. */
    private static int constant(Class<?> owner, String name) {
        try {
            return owner.getField(name).getInt(null);
        } catch (Throwable t) {
            return -1;
        }
    }

    /**
     * AudioManager.playSoundEffect(FX_*) by constant name; `fallback` (may be null) is the name
     * to use on an Android that lacks the first.
     */
    public static String playEffect(Activity activity, String name, String fallback) {
        try {
            AudioManager audio = (AudioManager) activity.getSystemService(Context.AUDIO_SERVICE);
            if (audio == null) {
                return "failed:no AudioManager";
            }
            if (!touchSoundsEnabled(activity)) {
                return "skipped:os:touch-sounds-off";
            }
            int effect = constant(AudioManager.class, name);
            if (effect < 0 && fallback != null) {
                effect = constant(AudioManager.class, fallback);
            }
            if (effect < 0) {
                return "failed:AudioManager has no constant " + name + " on this Android version";
            }
            audio.playSoundEffect(effect);
            return "played";
        } catch (Throwable t) {
            return "failed:" + t;
        }
    }

    /** The user's own tone of this kind ("notification"), played as a notification is. */
    public static String playTone(Activity activity, String kind) {
        try {
            if (!"notification".equals(kind)) {
                return "failed:unknown tone kind " + kind;
            }
            AudioManager audio = (AudioManager) activity.getSystemService(Context.AUDIO_SERVICE);
            if (audio != null) {
                int ringer = audio.getRingerMode();
                if (ringer == AudioManager.RINGER_MODE_SILENT) {
                    return "skipped:os:ringer-silent";
                }
                if (ringer == AudioManager.RINGER_MODE_VIBRATE) {
                    return "skipped:os:ringer-vibrate";
                }
            }
            Uri uri = RingtoneManager.getDefaultUri(RingtoneManager.TYPE_NOTIFICATION);
            if (uri == null) {
                return "failed:the phone has no default notification tone";
            }
            Ringtone next = RingtoneManager.getRingtone(activity, uri);
            if (next == null) {
                return "failed:no tone for " + uri;
            }
            next.setAudioAttributes(new AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_NOTIFICATION)
                    .setContentType(AudioAttributes.CONTENT_TYPE_SONIFICATION)
                    .build());
            synchronized (AzulSounds.class) {
                if (tone != null && tone.isPlaying()) {
                    tone.stop();
                }
                tone = next;
            }
            next.play();
            return "played";
        } catch (Throwable t) {
            return "failed:" + t;
        }
    }

    /** Whether the region requires the shutter sound (API 33), false where it cannot be asked. */
    private static boolean mustPlayShutterSound() {
        try {
            Object answer = MediaActionSound.class.getMethod("mustPlayShutterSound").invoke(null);
            return answer instanceof Boolean && ((Boolean) answer).booleanValue();
        } catch (Throwable t) {
            return false;
        }
    }

    /**
     * A MediaActionSound by constant name (SHUTTER_CLICK). `onlyIfRequired` is the app's mute:
     * the sound then plays only where the region requires it.
     */
    public static String playCamera(Activity activity, String name, boolean onlyIfRequired) {
        try {
            if (onlyIfRequired && !mustPlayShutterSound()) {
                return "skipped:app-muted-and-not-required";
            }
            int sound = constant(MediaActionSound.class, name);
            if (sound < 0) {
                return "failed:MediaActionSound has no constant " + name;
            }
            synchronized (AzulSounds.class) {
                if (camera == null) {
                    camera = new MediaActionSound();
                }
                camera.play(sound);
            }
            return "played";
        } catch (Throwable t) {
            return "failed:" + t;
        }
    }
}
