// Secret storage backed by the Android Keystore.
//
// Code contract (keyring/android.rs nativeOnKeyringResult): 0=Stored,
// 1=Deleted, 2=Retrieved (secret in the String argument), 3=NotFound,
// 4=Denied, 5=Unavailable, other=Error.
//
// The Keystore holds KEYS, not arbitrary secrets, so the secret itself is
// AES-GCM encrypted with a Keystore-held key and the ciphertext goes in
// SharedPreferences. That is the standard shape and it is why `store` needs
// both: the key never leaves the TEE, the blob is meaningless without it.

package com.azul.keyring;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyProperties;
import android.util.Base64;

import java.security.KeyStore;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.spec.GCMParameterSpec;

public final class AzulKeyring {

    private AzulKeyring() {}

    private static final int STORED = 0;
    private static final int DELETED = 1;
    private static final int RETRIEVED = 2;
    private static final int NOT_FOUND = 3;
    private static final int DENIED = 4;
    private static final int UNAVAILABLE = 5;
    private static final int ERROR = 6;

    private static final String KEYSTORE = "AndroidKeyStore";
    private static final String KEY_ALIAS = "azul_keyring_master";
    private static final String PREFS = "azul_keyring";
    private static final int GCM_TAG_BITS = 128;
    private static final int IV_BYTES = 12;

    public static void store(Activity activity, long handle, String key, String secret,
                             boolean requireAuth) {
        if (activity == null || key == null || secret == null) {
            nativeOnKeyringResult(handle, ERROR, null);
            return;
        }
        try {
            SecretKey master = masterKey(requireAuth);
            Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
            c.init(Cipher.ENCRYPT_MODE, master);
            byte[] iv = c.getIV();
            byte[] ct = c.doFinal(secret.getBytes("UTF-8"));
            // IV is prepended rather than stored separately: it is not secret,
            // it must never repeat for a given key, and keeping it beside the
            // ciphertext makes the two impossible to mismatch.
            byte[] blob = new byte[iv.length + ct.length];
            System.arraycopy(iv, 0, blob, 0, iv.length);
            System.arraycopy(ct, 0, blob, iv.length, ct.length);
            prefs(activity).edit()
                    .putString(key, Base64.encodeToString(blob, Base64.NO_WRAP))
                    .apply();
            nativeOnKeyringResult(handle, STORED, null);
        } catch (android.security.keystore.UserNotAuthenticatedException e) {
            nativeOnKeyringResult(handle, DENIED, null);
        } catch (Throwable t) {
            nativeOnKeyringResult(handle, unavailableOrError(t), null);
        }
    }

    public static void get(Activity activity, long handle, String key) {
        if (activity == null || key == null) {
            nativeOnKeyringResult(handle, ERROR, null);
            return;
        }
        try {
            String stored = prefs(activity).getString(key, null);
            if (stored == null) {
                nativeOnKeyringResult(handle, NOT_FOUND, null);
                return;
            }
            byte[] blob = Base64.decode(stored, Base64.NO_WRAP);
            if (blob.length <= IV_BYTES) {
                nativeOnKeyringResult(handle, ERROR, null);
                return;
            }
            byte[] iv = new byte[IV_BYTES];
            System.arraycopy(blob, 0, iv, 0, IV_BYTES);
            Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
            c.init(Cipher.DECRYPT_MODE, masterKey(false), new GCMParameterSpec(GCM_TAG_BITS, iv));
            byte[] pt = c.doFinal(blob, IV_BYTES, blob.length - IV_BYTES);
            nativeOnKeyringResult(handle, RETRIEVED, new String(pt, "UTF-8"));
        } catch (android.security.keystore.UserNotAuthenticatedException e) {
            nativeOnKeyringResult(handle, DENIED, null);
        } catch (Throwable t) {
            nativeOnKeyringResult(handle, unavailableOrError(t), null);
        }
    }

    public static void delete(Activity activity, long handle, String key) {
        if (activity == null || key == null) {
            nativeOnKeyringResult(handle, ERROR, null);
            return;
        }
        try {
            // Deleting something that was never there is success, not
            // NotFound: the caller asked for it to be gone and it is gone.
            prefs(activity).edit().remove(key).apply();
            nativeOnKeyringResult(handle, DELETED, null);
        } catch (Throwable t) {
            nativeOnKeyringResult(handle, unavailableOrError(t), null);
        }
    }

    private static SharedPreferences prefs(Context ctx) {
        return ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    /** Fetch or lazily create the Keystore-held AES key. */
    private static SecretKey masterKey(boolean requireAuth) throws Exception {
        KeyStore ks = KeyStore.getInstance(KEYSTORE);
        ks.load(null);
        KeyStore.Entry e = ks.getEntry(KEY_ALIAS, null);
        if (e instanceof KeyStore.SecretKeyEntry) {
            return ((KeyStore.SecretKeyEntry) e).getSecretKey();
        }
        KeyGenerator kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE);
        KeyGenParameterSpec.Builder b = new KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE);
        if (requireAuth) {
            // Gate on the device credential / biometric. Only meaningful when
            // the user actually has one set up; setting it otherwise makes the
            // key permanently unusable.
            b.setUserAuthenticationRequired(true);
        }
        kg.init(b.build());
        return kg.generateKey();
    }

    /**
     * No Keystore (an emulator image without one, a stripped device) is
     * UNAVAILABLE — a permanent, informative answer. Anything else is an
     * error the caller may retry.
     */
    private static int unavailableOrError(Throwable t) {
        return (t instanceof java.security.NoSuchProviderException
                || t instanceof java.security.KeyStoreException) ? UNAVAILABLE : ERROR;
    }

    private static native void nativeOnKeyringResult(long handle, int code, String secretOrNull);
}
