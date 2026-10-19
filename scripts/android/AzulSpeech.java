// Speech on Android's own engines: saying text aloud (TextToSpeech) and hearing what the user
// says (SpeechRecognizer), for azul's Speech (dll/src/desktop/extra/speech/android.rs).
//
// The Rust side calls the static methods through JNI from any thread. TextToSpeech binds its
// engine service asynchronously: what is said before it is ready waits here and is said once
// it is. SpeechRecognizer must be made and driven on the main thread, so listen() posts there
// and the answer comes back through nativeOnHeard, which the Rust side waits for.
//
// The microphone permission (RECORD_AUDIO) is NOT requested here. The Rust permission manager
// owns prompts (extra/permission/android.rs); without the permission, listening answers
// "denied" at once instead of a second owner racing the first one's dialog.

package com.azul.speech;

import android.app.Activity;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.os.Bundle;
import android.os.Handler;
import android.os.Looper;
import android.speech.RecognitionListener;
import android.speech.RecognizerIntent;
import android.speech.SpeechRecognizer;
import android.speech.tts.TextToSpeech;

import java.util.ArrayList;
import java.util.Locale;

public final class AzulSpeech {

    private AzulSpeech() {}

    // The answers' codes: azul's HeardStatus, in its order.
    private static final int HEARD = 0;
    private static final int NOTHING_HEARD = 1;
    private static final int DENIED = 2;
    private static final int UNAVAILABLE = 3;
    private static final int FAILED = 4;

    // TextToSpeech's state: 0 not made, 1 binding, 2 ready, -1 no engine.
    private static final Object LOCK = new Object();
    private static TextToSpeech tts;
    private static int ttsState = 0;
    private static final ArrayList<String[]> waiting = new ArrayList<>();
    private static long utterance = 0;

    /** Says `text` in `language` (a BCP 47 tag, empty for the system's voice); `interrupt`
     *  cuts off what is being said. False when there is no speech engine. */
    public static boolean say(Activity activity, String text, String language, boolean interrupt) {
        if (activity == null) {
            return false;
        }
        synchronized (LOCK) {
            if (ttsState == -1) {
                return false;
            }
            if (ttsState == 0) {
                ttsState = 1;
                try {
                    tts = new TextToSpeech(activity.getApplicationContext(), status -> {
                        synchronized (LOCK) {
                            if (status == TextToSpeech.SUCCESS) {
                                ttsState = 2;
                                for (String[] w : waiting) {
                                    speakLocked(w[0], w[1], "1".equals(w[2]));
                                }
                            } else {
                                ttsState = -1;
                            }
                            waiting.clear();
                        }
                    });
                } catch (Throwable t) {
                    ttsState = -1;
                    return false;
                }
            }
            if (ttsState == 1) {
                if (interrupt) {
                    waiting.clear();
                }
                waiting.add(new String[] {text, language == null ? "" : language,
                        interrupt ? "1" : "0"});
                return true;
            }
            return speakLocked(text, language, interrupt);
        }
    }

    private static boolean speakLocked(String text, String language, boolean interrupt) {
        try {
            if (language != null && !language.isEmpty()) {
                tts.setLanguage(Locale.forLanguageTag(language));
            }
            utterance += 1;
            int queue = interrupt ? TextToSpeech.QUEUE_FLUSH : TextToSpeech.QUEUE_ADD;
            return tts.speak(text, queue, null, "azul-" + utterance) == TextToSpeech.SUCCESS;
        } catch (Throwable t) {
            return false;
        }
    }

    /** Stops saying anything and drops what waits. */
    public static void stop() {
        synchronized (LOCK) {
            waiting.clear();
            try {
                if (tts != null && ttsState == 2) {
                    tts.stop();
                }
            } catch (Throwable ignored) {
            }
        }
    }

    /** Whether something is being said (or waits for the engine). */
    public static boolean isSpeaking() {
        synchronized (LOCK) {
            if (ttsState == 1) {
                return !waiting.isEmpty();
            }
            try {
                return tts != null && ttsState == 2 && tts.isSpeaking();
            } catch (Throwable t) {
                return false;
            }
        }
    }

    /** Whether anything can be said: false once the engine refused to start. */
    public static boolean isAvailable() {
        synchronized (LOCK) {
            return ttsState != -1;
        }
    }

    /** Listens for up to `timeoutMs` in `language` (empty: the system's); the answer comes
     *  through nativeOnHeard(handle, ...), exactly once. */
    public static void listen(final Activity activity, final long handle, final String language,
                              final int timeoutMs) {
        if (activity == null) {
            nativeOnHeard(handle, UNAVAILABLE, "", "no activity to listen in", 0f);
            return;
        }
        activity.runOnUiThread(() -> {
            try {
                if (!SpeechRecognizer.isRecognitionAvailable(activity)) {
                    nativeOnHeard(handle, UNAVAILABLE, "",
                            "this device has no speech recognizer", 0f);
                    return;
                }
                if (activity.checkSelfPermission(android.Manifest.permission.RECORD_AUDIO)
                        != PackageManager.PERMISSION_GRANTED) {
                    nativeOnHeard(handle, DENIED, "", "the microphone is not allowed", 0f);
                    return;
                }
                final SpeechRecognizer recognizer = SpeechRecognizer.createSpeechRecognizer(activity);
                final boolean[] answered = {false};
                final Handler main = new Handler(Looper.getMainLooper());
                final Runnable timeUp = recognizer::stopListening;
                recognizer.setRecognitionListener(new RecognitionListener() {
                    private void answer(int status, String text, String message, float confidence) {
                        if (answered[0]) {
                            return;
                        }
                        answered[0] = true;
                        main.removeCallbacks(timeUp);
                        nativeOnHeard(handle, status, text, message, confidence);
                        recognizer.destroy();
                    }

                    @Override
                    public void onResults(Bundle results) {
                        ArrayList<String> said = results == null ? null
                                : results.getStringArrayList(SpeechRecognizer.RESULTS_RECOGNITION);
                        float[] sure = results == null ? null
                                : results.getFloatArray(SpeechRecognizer.CONFIDENCE_SCORES);
                        if (said == null || said.isEmpty()) {
                            answer(NOTHING_HEARD, "", "", 0f);
                        } else {
                            float confidence = (sure != null && sure.length > 0) ? sure[0] : 0f;
                            answer(HEARD, said.get(0), "", confidence);
                        }
                    }

                    @Override
                    public void onError(int error) {
                        switch (error) {
                            case SpeechRecognizer.ERROR_NO_MATCH:
                            case SpeechRecognizer.ERROR_SPEECH_TIMEOUT:
                                answer(NOTHING_HEARD, "", "", 0f);
                                break;
                            case SpeechRecognizer.ERROR_INSUFFICIENT_PERMISSIONS:
                                answer(DENIED, "", "the microphone is not allowed", 0f);
                                break;
                            case SpeechRecognizer.ERROR_NETWORK:
                            case SpeechRecognizer.ERROR_NETWORK_TIMEOUT:
                                answer(FAILED, "", "the speech recognizer needs the network", 0f);
                                break;
                            default:
                                answer(FAILED, "", "the speech recognizer failed (" + error + ")", 0f);
                                break;
                        }
                    }

                    @Override public void onReadyForSpeech(Bundle params) {}
                    @Override public void onBeginningOfSpeech() {}
                    @Override public void onRmsChanged(float rmsdB) {}
                    @Override public void onBufferReceived(byte[] buffer) {}
                    @Override public void onEndOfSpeech() {}
                    @Override public void onPartialResults(Bundle partialResults) {}
                    @Override public void onEvent(int eventType, Bundle params) {}
                });
                Intent intent = new Intent(RecognizerIntent.ACTION_RECOGNIZE_SPEECH);
                intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE_MODEL,
                        RecognizerIntent.LANGUAGE_MODEL_FREE_FORM);
                intent.putExtra(RecognizerIntent.EXTRA_MAX_RESULTS, 1);
                if (language != null && !language.isEmpty()) {
                    intent.putExtra(RecognizerIntent.EXTRA_LANGUAGE, language);
                }
                recognizer.startListening(intent);
                // The time is up: what was said so far is the answer (onResults or onError).
                main.postDelayed(timeUp, Math.max(1, timeoutMs));
            } catch (Throwable t) {
                nativeOnHeard(handle, FAILED, "", "the speech recognizer failed: " + t, 0f);
            }
        });
    }

    private static native void nativeOnHeard(long handle, int status, String text, String message,
                                             float confidence);
}
