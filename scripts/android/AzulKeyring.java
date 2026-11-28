// Secret storage backed by the Android Keystore, one hardware-backed key PER ENTRY.
//
// What the library promises per protection level (the model of core/src/keyring.rs; this file is what ENFORCES it on
// Android, so the table is "enforced by hardware" wherever the device has a TEE or StrongBox):
//
//   0 DeviceUnlocked: an AES-GCM key in the Keystore without a user check. Never shows a sheet. The secret is
//     meaningless without the key, and the key never leaves the secure hardware.
//   1 UserPresence and 2 Biometric: the secret is encrypted with a random AES data key in software and the data key is
//     WRAPPED with the public half of an RSA key pair made in the Keystore for the entry. Wrapping needs no user check
//     (public-key operations never do), so storing shows no sheet; the PRIVATE half needs one for every use
//     (setUserAuthenticationRequired, timeout 0): the Keystore refuses to unwrap until a BiometricPrompt that carries the
//     unwrapping cipher as its CryptoObject succeeded. UserPresence allows BIOMETRIC_STRONG or DEVICE_CREDENTIAL from
//     API 30 (API 28-29 cannot offer the device credential for a crypto-bound prompt: biometric only there, and
//     support() says Library, not Hardware). Biometric allows BIOMETRIC_STRONG only and sets
//     setInvalidatedByBiometricEnrollment(true): a new finger or face makes the private key unusable for good
//     (KeyPermanentlyInvalidatedException -> Invalidated: the app stores the secret again).
//
// StrongBox is asked for when the device has it (FEATURE_STRONGBOX_KEYSTORE) and dropped when the Keystore says it
// cannot (StrongBoxUnavailableException).
//
// The ciphertext (IV + GCM ciphertext) is in SharedPreferences under "b.<id>" (and the wrapped data key under "w.<id>" for a
// protected entry), and a META record under "m.<id>" holds the level and when the entry was made, readable WITHOUT any user
// check and kept as long as the item is: a protected item whose key is gone or invalidated is Invalidated, not NotFound.
// <id> is the SHA-256 of the key name (any characters, any length).
//
// Result codes (keyring/android.rs nativeOnKeyringResult): 0 Stored, 1 Deleted, 2 Retrieved (the secret in the String
// argument), 3 NotFound, 4 Denied (the system or the model refused: a quiet read of a protected item, an unauthenticated
// key), 5 Unavailable (no Keystore, no hardware), 6 Error, 7 Cancelled (the user closed the sheet), 8 Failed, 9 LockedOut,
// 10 NotEnrolled (nothing to bind the key to), 11 Invalidated (the enrolled set changed), 12 WouldPrompt (a read WITHOUT the
// user of a protected item: no sheet is shown, the app calls unlock()).
//
// A sheet appears only for unlock() of a protected entry: store(), get() and delete() never show one.

package com.azul.keyring;

import android.app.Activity;
import android.content.Context;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.hardware.biometrics.BiometricManager;
import android.hardware.biometrics.BiometricPrompt;
import android.os.Build;
import android.os.CancellationSignal;
import android.security.keystore.KeyGenParameterSpec;
import android.security.keystore.KeyInfo;
import android.security.keystore.KeyPermanentlyInvalidatedException;
import android.security.keystore.KeyProperties;
import android.security.keystore.StrongBoxUnavailableException;
import android.security.keystore.UserNotAuthenticatedException;
import android.util.Base64;

import java.security.KeyStore;
import java.security.MessageDigest;
import java.security.PrivateKey;
import java.security.PublicKey;
import java.security.SecureRandom;
import java.security.KeyPairGenerator;
import java.security.spec.MGF1ParameterSpec;

import javax.crypto.Cipher;
import javax.crypto.KeyGenerator;
import javax.crypto.SecretKey;
import javax.crypto.SecretKeyFactory;
import javax.crypto.spec.GCMParameterSpec;
import javax.crypto.spec.OAEPParameterSpec;
import javax.crypto.spec.PSource;
import javax.crypto.spec.SecretKeySpec;

public final class AzulKeyring {

    private AzulKeyring() {}

    private static final int STORED = 0;
    private static final int DELETED = 1;
    private static final int RETRIEVED = 2;
    private static final int NOT_FOUND = 3;
    private static final int DENIED = 4;
    private static final int UNAVAILABLE = 5;
    private static final int ERROR = 6;
    private static final int CANCELLED = 7;
    private static final int FAILED = 8;
    private static final int LOCKED_OUT = 9;
    private static final int NOT_ENROLLED = 10;
    private static final int INVALIDATED = 11;
    private static final int WOULD_PROMPT = 12;

    private static final int LEVEL_DEVICE_UNLOCKED = 0;
    private static final int LEVEL_USER_PRESENCE = 1;
    private static final int LEVEL_BIOMETRIC = 2;

    // KeyringNeed, in the order of the enum.
    private static final int NEED_NOT_FOUND = 0;
    private static final int NEED_NOTHING = 1;
    private static final int NEED_UNLOCK = 2;
    private static final int NEED_USER_PRESENCE = 3;
    private static final int NEED_BIOMETRIC = 4;
    private static final int NEED_INVALIDATED = 5;
    private static final int NEED_UNAVAILABLE = 6;
    private static final int NEED_ERROR = 7;

    // KeyringSupport, in the order of the enum.
    private static final int SUPPORT_HARDWARE = 0;
    private static final int SUPPORT_LIBRARY = 1;
    private static final int SUPPORT_NOT_ENROLLED = 2;
    private static final int SUPPORT_UNAVAILABLE = 3;

    // android.hardware.biometrics.BiometricPrompt error codes (some are not public constants on API 28/29).
    private static final int ERR_HW_UNAVAILABLE = 1;
    private static final int ERR_TIMEOUT = 3;
    private static final int ERR_CANCELED = 5;
    private static final int ERR_LOCKOUT = 7;
    private static final int ERR_LOCKOUT_PERMANENT = 9;
    private static final int ERR_USER_CANCELED = 10;
    private static final int ERR_NO_BIOMETRICS = 11;
    private static final int ERR_HW_NOT_PRESENT = 12;
    private static final int ERR_NEGATIVE_BUTTON = 13;
    private static final int ERR_NO_DEVICE_CREDENTIAL = 14;

    private static final String KEYSTORE = "AndroidKeyStore";
    private static final String ALIAS_PREFIX = "azul_keyring.";
    private static final String PREFS = "azul_keyring";
    private static final int GCM_TAG_BITS = 128;
    private static final int IV_BYTES = 12;
    private static final String RSA = "RSA/ECB/OAEPWithSHA-256AndMGF1Padding";
    // The Keystore's OAEP uses SHA-1 as the MGF1 digest whatever the transformation says: the software side has to say so too.
    private static final OAEPParameterSpec OAEP =
            new OAEPParameterSpec("SHA-256", "MGF1", MGF1ParameterSpec.SHA1, PSource.PSpecified.DEFAULT);

    // ---- the entries -----------------------------------------------------------------------------------------------

    /**
     * Store `secret` under `key` at `protection`. No sheet at any level: a protected entry is wrapped with the PUBLIC half
     * of its Keystore key pair (`reason` and `cancel` belong to the unlock that will read it).
     */
    public static void store(Activity activity, long handle, String key, String secret, int protection,
                             String reason, String cancel) {
        if (activity == null || key == null || secret == null || protection < 0 || protection > LEVEL_BIOMETRIC) {
            nativeOnKeyringResult(handle, ERROR, null);
            return;
        }
        String id = null;
        try {
            id = idOf(key);
            if (protection == LEVEL_DEVICE_UNLOCKED) {
                nativeOnKeyringResult(handle, storeUnprotected(activity, id, secret), null);
                return;
            }
            // Never a weaker store: nothing to bind the key to, or no hardware, is an answer of its own.
            int support = supportOf(activity, protection);
            if (support == SUPPORT_NOT_ENROLLED) {
                nativeOnKeyringResult(handle, NOT_ENROLLED, null);
                return;
            }
            if (support == SUPPORT_UNAVAILABLE) {
                nativeOnKeyringResult(handle, UNAVAILABLE, null);
                return;
            }
            // A new key pair for this entry (an old entry under the same name is replaced, key and all). The secret is
            // encrypted with a random data key and the data key is wrapped with the PUBLIC half: no sheet to store.
            deleteKey(id);
            PublicKey pub = createKeyPair(activity, id, protection);
            byte[] dataKey = new byte[32];
            new SecureRandom().nextBytes(dataKey);
            try {
                Cipher aes = Cipher.getInstance("AES/GCM/NoPadding");
                aes.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(dataKey, "AES"));
                byte[] iv = aes.getIV();
                byte[] ct = aes.doFinal(secret.getBytes("UTF-8"));
                Cipher wrap = Cipher.getInstance(RSA);
                wrap.init(Cipher.ENCRYPT_MODE, pub, OAEP);
                byte[] wrapped = wrap.doFinal(dataKey);
                keep(activity, id, iv, ct, wrapped, protection);
            } finally {
                java.util.Arrays.fill(dataKey, (byte) 0);
            }
            nativeOnKeyringResult(handle, STORED, null);
        } catch (Throwable t) {
            if (id != null) {
                deleteKey(id);
            }
            nativeOnKeyringResult(handle, failure(t), null);
        }
    }

    /**
     * Read `key` WITHOUT a user check: an unprotected item answers Retrieved, a protected one answers WouldPrompt (the
     * sheet is unlock()'s), an item whose key is gone or invalidated answers Invalidated, nothing at all NotFound.
     */
    public static void get(Activity activity, long handle, String key) {
        String[] r = quiet(activity, OP_GET, key, null);
        nativeOnKeyringResult(handle, Integer.parseInt(r[0]), r[1]);
    }

    // The operations of quiet(): what the blocking statics (`Keyring::get_blocking` and the quiet ones) ask of a worker thread.
    private static final int OP_GET = 0;
    private static final int OP_STORE = 1;
    private static final int OP_DELETE = 2;

    /**
     * A SYNCHRONOUS answer, never a sheet: `op` is OP_GET, OP_STORE (an UNPROTECTED item: the secret in `secret`) or
     * OP_DELETE. Returns {code, secret or null}: the caller's own thread gets the result, no callback is involved. The
     * async entries get() and delete() are this with the answer handed to nativeOnKeyringResult.
     */
    public static String[] quiet(Activity activity, int op, String key, String secret) {
        if (activity == null || key == null || (op == OP_STORE && secret == null)) {
            return answer(ERROR, null);
        }
        try {
            String id = idOf(key);
            if (op == OP_STORE) {
                return answer(storeUnprotected(activity, id, secret), null);
            }
            if (op == OP_DELETE) {
                prefs(activity).edit().remove("b." + id).remove("w." + id).remove("m." + id).apply();
                deleteKey(id);
                return answer(DELETED, null);
            }
            int level = levelOf(activity, id);
            if (level < 0 && blobOf(activity, id) == null) {
                return answer(NOT_FOUND, null);
            }
            if (level > LEVEL_DEVICE_UNLOCKED) {
                return answer(keyUsable(id) ? WOULD_PROMPT : INVALIDATED, null);
            }
            Opened o = open(activity, id);
            if (o.code != RETRIEVED) {
                return answer(o.code, null);
            }
            return answer(RETRIEVED, new String(o.cipher.doFinal(o.ct), "UTF-8"));
        } catch (Throwable t) {
            return answer(failure(t), null);
        }
    }

    private static String[] answer(int code, String secret) {
        return new String[] {Integer.toString(code), secret};
    }

    /**
     * Read `key` and, when it is protected, show the system sheet first: the cipher of the entry's key is the sheet's
     * CryptoObject and the secret is decrypted ONLY with the cipher the successful sheet hands back.
     */
    public static void unlock(Activity activity, long handle, String key, String reason, String cancel) {
        if (activity == null || key == null) {
            nativeOnKeyringResult(handle, ERROR, null);
            return;
        }
        try {
            String id = idOf(key);
            int level = levelOf(activity, id);
            if (level < 0 && blobOf(activity, id) == null) {
                nativeOnKeyringResult(handle, NOT_FOUND, null);
                return;
            }
            Opened o = open(activity, id);
            if (o.code != RETRIEVED) {
                nativeOnKeyringResult(handle, o.code, null);
                return;
            }
            if (o.wrapped == null) {
                nativeOnKeyringResult(handle, RETRIEVED, new String(o.cipher.doFinal(o.ct), "UTF-8"));
                return;
            }
            sheet(activity, o.cipher, reason, cancel, level, new SheetDone() {
                @Override
                public void done(int code, Cipher unlocked) {
                    if (code != RETRIEVED || unlocked == null) {
                        nativeOnKeyringResult(handle, code == RETRIEVED ? ERROR : code, null);
                        return;
                    }
                    byte[] dataKey = null;
                    try {
                        // Only the cipher the successful sheet hands back may unwrap the data key.
                        dataKey = unlocked.doFinal(o.wrapped);
                        Cipher aes = Cipher.getInstance("AES/GCM/NoPadding");
                        aes.init(Cipher.DECRYPT_MODE, new SecretKeySpec(dataKey, "AES"),
                                new GCMParameterSpec(GCM_TAG_BITS, o.iv));
                        nativeOnKeyringResult(handle, RETRIEVED, new String(aes.doFinal(o.ct), "UTF-8"));
                    } catch (Throwable t) {
                        nativeOnKeyringResult(handle, failure(t), null);
                    } finally {
                        if (dataKey != null) {
                            java.util.Arrays.fill(dataKey, (byte) 0);
                        }
                    }
                }
            });
        } catch (Throwable t) {
            nativeOnKeyringResult(handle, failure(t), null);
        }
    }

    /** Remove the entry: its ciphertext, its meta record and its Keystore key. Deleting what is not there is success. */
    public static void delete(Activity activity, long handle, String key) {
        String[] r = quiet(activity, OP_DELETE, key, null);
        nativeOnKeyringResult(handle, Integer.parseInt(r[0]), r[1]);
    }

    /** What reading `key` would need, without any sheet (KeyringNeed, 0-based in the order of the enum). */
    public static int needsPrompt(Activity activity, String key) {
        if (activity == null || key == null) {
            return NEED_ERROR;
        }
        try {
            String id = idOf(key);
            int level = levelOf(activity, id);
            if (level < 0) {
                return blobOf(activity, id) == null ? NEED_NOT_FOUND : NEED_UNLOCK;
            }
            if (blobOf(activity, id) == null) {
                // The meta record outlived the item: the entry existed and its secret is gone.
                return NEED_INVALIDATED;
            }
            if (level > LEVEL_DEVICE_UNLOCKED && !keyUsable(id)) {
                return NEED_INVALIDATED;
            }
            switch (level) {
                case LEVEL_DEVICE_UNLOCKED:
                    return NEED_NOTHING;
                case LEVEL_USER_PRESENCE:
                    return NEED_USER_PRESENCE;
                default:
                    return NEED_BIOMETRIC;
            }
        } catch (java.security.KeyStoreException | java.security.NoSuchProviderException e) {
            return NEED_UNAVAILABLE;
        } catch (Throwable t) {
            return NEED_ERROR;
        }
    }

    /** Whether `level` can be bound on this device (KeyringSupport, 0-based in the order of the enum). */
    public static int support(Activity activity, int level) {
        if (activity == null || level < 0 || level > LEVEL_BIOMETRIC) {
            return SUPPORT_UNAVAILABLE;
        }
        try {
            return supportOf(activity, level);
        } catch (Throwable t) {
            return SUPPORT_UNAVAILABLE;
        }
    }

    // ---- the storage ------------------------------------------------------------------------------------------------

    private static SharedPreferences prefs(Context ctx) {
        return ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE);
    }

    /** The id of an entry: the SHA-256 of its name as 32 hex digits. The name is never an alias or a file name. */
    private static String idOf(String key) throws Exception {
        byte[] d = MessageDigest.getInstance("SHA-256").digest(key.getBytes("UTF-8"));
        StringBuilder sb = new StringBuilder(32);
        for (int i = 0; i < 16; i++) {
            sb.append(Character.forDigit((d[i] >> 4) & 0xf, 16)).append(Character.forDigit(d[i] & 0xf, 16));
        }
        return sb.toString();
    }

    private static byte[] blobOf(Context ctx, String id) {
        String stored = prefs(ctx).getString("b." + id, null);
        return stored == null ? null : Base64.decode(stored, Base64.NO_WRAP);
    }

    /** The level of the meta record, or -1 when there is none (or it cannot be read). */
    private static int levelOf(Context ctx, String id) {
        String meta = prefs(ctx).getString("m." + id, null);
        if (meta == null) {
            return -1;
        }
        for (String part : meta.split(";")) {
            if (part.startsWith("level=")) {
                try {
                    return Integer.parseInt(part.substring(6));
                } catch (NumberFormatException e) {
                    return -1;
                }
            }
        }
        return -1;
    }

    private static void keep(Context ctx, String id, byte[] iv, byte[] ct, byte[] wrapped, int level) {
        byte[] blob = new byte[iv.length + ct.length];
        System.arraycopy(iv, 0, blob, 0, iv.length);
        System.arraycopy(ct, 0, blob, iv.length, ct.length);
        // The IV is beside the ciphertext so the two cannot be mismatched; the meta record is written WITH the blob.
        SharedPreferences.Editor e = prefs(ctx).edit()
                .putString("b." + id, Base64.encodeToString(blob, Base64.NO_WRAP))
                .putString("m." + id, "v=2;level=" + level + ";created=" + System.currentTimeMillis()
                        + ";api=" + Build.VERSION.SDK_INT);
        if (wrapped != null) {
            e.putString("w." + id, Base64.encodeToString(wrapped, Base64.NO_WRAP));
        } else {
            e.remove("w." + id);
        }
        e.apply();
    }

    private static int storeUnprotected(Context ctx, String id, String secret) throws Exception {
        deleteKey(id);
        SecretKey k = createKey(ctx, id, LEVEL_DEVICE_UNLOCKED);
        Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
        cipher.init(Cipher.ENCRYPT_MODE, k);
        byte[] iv = cipher.getIV();
        byte[] ct = cipher.doFinal(secret.getBytes("UTF-8"));
        keep(ctx, id, iv, ct, null, LEVEL_DEVICE_UNLOCKED);
        return STORED;
    }

    /** What open() found: a cipher ready for the entry's ciphertext (or for unwrapping its data key), or why there is none. */
    private static final class Opened {
        final int code;
        /** Level 0: the AES cipher ready to decrypt `ct`. Protected: the RSA cipher that unwraps `wrapped` (the sheet's). */
        final Cipher cipher;
        final byte[] iv;
        final byte[] ct;
        /** The wrapped data key of a protected entry, null for level 0. */
        final byte[] wrapped;

        Opened(int code, Cipher cipher, byte[] iv, byte[] ct, byte[] wrapped) {
            this.code = code;
            this.cipher = cipher;
            this.iv = iv;
            this.ct = ct;
            this.wrapped = wrapped;
        }

        static Opened failed(int code) {
            return new Opened(code, null, null, null, null);
        }
    }

    private static Opened open(Context ctx, String id) throws Exception {
        byte[] blob = blobOf(ctx, id);
        if (blob == null || blob.length <= IV_BYTES) {
            // A meta record without its item: the entry existed (see needsPrompt).
            return Opened.failed(levelOf(ctx, id) >= 0 ? INVALIDATED : NOT_FOUND);
        }
        byte[] iv = new byte[IV_BYTES];
        System.arraycopy(blob, 0, iv, 0, IV_BYTES);
        byte[] ct = new byte[blob.length - IV_BYTES];
        System.arraycopy(blob, IV_BYTES, ct, 0, ct.length);
        KeyStore ks = KeyStore.getInstance(KEYSTORE);
        ks.load(null);
        String wrappedText = prefs(ctx).getString("w." + id, null);
        try {
            if (wrappedText == null) {
                KeyStore.Entry e = ks.getEntry(ALIAS_PREFIX + id, null);
                if (!(e instanceof KeyStore.SecretKeyEntry)) {
                    return Opened.failed(INVALIDATED);
                }
                Cipher cipher = Cipher.getInstance("AES/GCM/NoPadding");
                cipher.init(Cipher.DECRYPT_MODE, ((KeyStore.SecretKeyEntry) e).getSecretKey(),
                        new GCMParameterSpec(GCM_TAG_BITS, iv));
                return new Opened(RETRIEVED, cipher, iv, ct, null);
            }
            java.security.Key k = ks.getKey(ALIAS_PREFIX + id, null);
            if (!(k instanceof PrivateKey)) {
                return Opened.failed(INVALIDATED);
            }
            Cipher cipher = Cipher.getInstance(RSA);
            cipher.init(Cipher.DECRYPT_MODE, (PrivateKey) k);
            return new Opened(RETRIEVED, cipher, iv, ct, Base64.decode(wrappedText, Base64.NO_WRAP));
        } catch (KeyPermanentlyInvalidatedException inv) {
            return Opened.failed(INVALIDATED);
        }
    }

    // ---- the keys -----------------------------------------------------------------------------------------------------

    private static boolean hasStrongBox(Context ctx) {
        return Build.VERSION.SDK_INT >= Build.VERSION_CODES.P
                && ctx.getPackageManager().hasSystemFeature(PackageManager.FEATURE_STRONGBOX_KEYSTORE);
    }

    /** Whether the entry's Keystore key exists and can still be used (not invalidated by a change of the biometrics). */
    private static boolean keyUsable(String id) {
        try {
            KeyStore ks = KeyStore.getInstance(KEYSTORE);
            ks.load(null);
            java.security.Key k = ks.getKey(ALIAS_PREFIX + id, null);
            if (k instanceof PrivateKey) {
                Cipher c = Cipher.getInstance(RSA);
                c.init(Cipher.DECRYPT_MODE, (PrivateKey) k);
                return true;
            }
            if (k instanceof SecretKey) {
                Cipher c = Cipher.getInstance("AES/GCM/NoPadding");
                c.init(Cipher.ENCRYPT_MODE, (SecretKey) k);
                return true;
            }
            return false;
        } catch (KeyPermanentlyInvalidatedException e) {
            return false;
        } catch (Throwable t) {
            // Anything else (a transient Keystore error) is not a verdict on the key.
            return true;
        }
    }

    private static SecretKey createKey(Context ctx, String id, int level) throws Exception {
        boolean strongBox = hasStrongBox(ctx);
        try {
            return generate(id, level, strongBox);
        } catch (StrongBoxUnavailableException e) {
            return generate(id, level, false);
        }
    }

    /** The unprotected level: an AES-GCM key in the Keystore, no user check. */
    private static SecretKey generate(String id, int level, boolean strongBox) throws Exception {
        KeyGenerator kg = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE);
        KeyGenParameterSpec.Builder b = new KeyGenParameterSpec.Builder(
                ALIAS_PREFIX + id, KeyProperties.PURPOSE_ENCRYPT | KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256);
        if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            b.setIsStrongBoxBacked(true);
        }
        kg.init(b.build());
        return kg.generateKey();
    }

    /**
     * A protected level: an RSA key pair whose PRIVATE half needs the user check for every use (timeout 0). Only the
     * Biometric level is bound to the enrolled set; with the device credential allowed a new finger changes nothing the
     * owner cannot already do with the passcode.
     */
    private static PublicKey createKeyPair(Context ctx, String id, int level) throws Exception {
        boolean strongBox = hasStrongBox(ctx);
        try {
            return generatePair(id, level, strongBox);
        } catch (StrongBoxUnavailableException e) {
            return generatePair(id, level, false);
        }
    }

    private static PublicKey generatePair(String id, int level, boolean strongBox) throws Exception {
        KeyPairGenerator kpg = KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_RSA, KEYSTORE);
        KeyGenParameterSpec.Builder b = new KeyGenParameterSpec.Builder(ALIAS_PREFIX + id, KeyProperties.PURPOSE_DECRYPT)
                .setKeySize(2048)
                .setDigests(KeyProperties.DIGEST_SHA256, KeyProperties.DIGEST_SHA1)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_RSA_OAEP)
                .setUserAuthenticationRequired(true);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // Timeout 0: every use needs a sheet that carries the cipher.
            b.setUserAuthenticationParameters(0, level == LEVEL_BIOMETRIC
                    ? KeyProperties.AUTH_BIOMETRIC_STRONG
                    : (KeyProperties.AUTH_BIOMETRIC_STRONG | KeyProperties.AUTH_DEVICE_CREDENTIAL));
        }
        b.setInvalidatedByBiometricEnrollment(level == LEVEL_BIOMETRIC);
        if (strongBox && Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            b.setIsStrongBoxBacked(true);
        }
        kpg.initialize(b.build());
        return kpg.generateKeyPair().getPublic();
    }

    private static void deleteKey(String id) {
        try {
            KeyStore ks = KeyStore.getInstance(KEYSTORE);
            ks.load(null);
            if (ks.containsAlias(ALIAS_PREFIX + id)) {
                ks.deleteEntry(ALIAS_PREFIX + id);
            }
        } catch (Throwable t) {
            // The entry is gone for the library either way: its ciphertext and meta record are removed by the caller.
        }
    }

    // ---- what the device can do ---------------------------------------------------------------------------------------

    /** KeyringSupport of a protection level here. */
    private static int supportOf(Activity activity, int level) throws Exception {
        if (level == LEVEL_DEVICE_UNLOCKED) {
            return isHardwareBacked(activity) ? SUPPORT_HARDWARE : SUPPORT_LIBRARY;
        }
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.P) {
            return SUPPORT_UNAVAILABLE;
        }
        int can;
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            BiometricManager bm = activity.getSystemService(BiometricManager.class);
            if (bm == null) {
                return SUPPORT_UNAVAILABLE;
            }
            int authenticators = level == LEVEL_BIOMETRIC
                    ? BiometricManager.Authenticators.BIOMETRIC_STRONG
                    : (BiometricManager.Authenticators.BIOMETRIC_STRONG
                            | BiometricManager.Authenticators.DEVICE_CREDENTIAL);
            can = bm.canAuthenticate(authenticators);
        } else if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            BiometricManager bm = activity.getSystemService(BiometricManager.class);
            if (bm == null) {
                return SUPPORT_UNAVAILABLE;
            }
            can = bm.canAuthenticate();
        } else {
            // API 28: no BiometricManager; the fingerprint manager is the only question the platform answers.
            return fingerprintSupport(activity);
        }
        switch (can) {
            case BiometricManager.BIOMETRIC_SUCCESS:
                // Below API 30 the device credential cannot back a crypto-bound sheet: biometric only (Library).
                return level == LEVEL_USER_PRESENCE && Build.VERSION.SDK_INT < Build.VERSION_CODES.R
                        ? SUPPORT_LIBRARY : SUPPORT_HARDWARE;
            case BiometricManager.BIOMETRIC_ERROR_NONE_ENROLLED:
                return SUPPORT_NOT_ENROLLED;
            default:
                return SUPPORT_UNAVAILABLE;
        }
    }

    @SuppressWarnings("deprecation")
    private static int fingerprintSupport(Activity activity) {
        android.hardware.fingerprint.FingerprintManager fm =
                (android.hardware.fingerprint.FingerprintManager) activity.getSystemService(Context.FINGERPRINT_SERVICE);
        if (fm == null || !fm.isHardwareDetected()) {
            return SUPPORT_UNAVAILABLE;
        }
        return fm.hasEnrolledFingerprints() ? SUPPORT_LIBRARY : SUPPORT_NOT_ENROLLED;
    }

    /** Whether a Keystore key lives in secure hardware here: a throwaway key is made and asked (KeyInfo). */
    private static boolean isHardwareBacked(Context ctx) throws Exception {
        String id = "probe";
        try {
            SecretKey k = createKey(ctx, id, LEVEL_DEVICE_UNLOCKED);
            SecretKeyFactory f = SecretKeyFactory.getInstance(k.getAlgorithm(), KEYSTORE);
            KeyInfo info = (KeyInfo) f.getKeySpec(k, KeyInfo.class);
            return info.isInsideSecureHardware();
        } finally {
            deleteKey(id);
        }
    }

    // ---- the sheet ------------------------------------------------------------------------------------------------------

    private interface SheetDone {
        /** `code` is RETRIEVED (here: the sheet succeeded, `unlocked` is the authenticated cipher) or a result code. */
        void done(int code, Cipher unlocked);
    }

    private static void sheet(Activity activity, Cipher cipher, String reason, String cancel, int level,
                              SheetDone done) {
        activity.runOnUiThread(() -> {
            try {
                BiometricPrompt.Builder b = new BiometricPrompt.Builder(activity)
                        .setTitle(reason == null || reason.isEmpty() ? "Confirm it is you" : reason);
                boolean credential = level == LEVEL_USER_PRESENCE && Build.VERSION.SDK_INT >= Build.VERSION_CODES.R;
                if (credential) {
                    b.setAllowedAuthenticators(BiometricManager.Authenticators.BIOMETRIC_STRONG
                            | BiometricManager.Authenticators.DEVICE_CREDENTIAL);
                } else {
                    // A negative button is mandatory when the device credential is not allowed.
                    b.setNegativeButton(cancel == null || cancel.isEmpty() ? "Cancel" : cancel,
                            activity.getMainExecutor(), (dialog, which) -> done.done(CANCELLED, null));
                }
                b.build().authenticate(new BiometricPrompt.CryptoObject(cipher), new CancellationSignal(),
                        activity.getMainExecutor(), new BiometricPrompt.AuthenticationCallback() {
                            @Override
                            public void onAuthenticationSucceeded(BiometricPrompt.AuthenticationResult result) {
                                BiometricPrompt.CryptoObject crypto = result.getCryptoObject();
                                done.done(RETRIEVED, crypto == null ? null : crypto.getCipher());
                            }

                            @Override
                            public void onAuthenticationFailed() {
                                // A rejected finger is not the end: the sheet stays up and the user tries again. The
                                // terminal answer comes from succeeded or error only.
                            }

                            @Override
                            public void onAuthenticationError(int code, CharSequence msg) {
                                done.done(mapError(code), null);
                            }
                        });
            } catch (Throwable t) {
                done.done(failure(t), null);
            }
        });
    }

    private static int mapError(int code) {
        switch (code) {
            case ERR_USER_CANCELED:
            case ERR_CANCELED:
            case ERR_NEGATIVE_BUTTON:
            case ERR_TIMEOUT:
                return CANCELLED;
            case ERR_LOCKOUT:
            case ERR_LOCKOUT_PERMANENT:
                return LOCKED_OUT;
            case ERR_NO_BIOMETRICS:
            case ERR_NO_DEVICE_CREDENTIAL:
                return NOT_ENROLLED;
            case ERR_HW_NOT_PRESENT:
            case ERR_HW_UNAVAILABLE:
                return UNAVAILABLE;
            default:
                return ERROR;
        }
    }

    /** The result code of an exception: the Keystore's own answers first. */
    private static int failure(Throwable t) {
        if (t instanceof KeyPermanentlyInvalidatedException) {
            return INVALIDATED;
        }
        if (t instanceof UserNotAuthenticatedException) {
            return DENIED;
        }
        if (t instanceof java.security.NoSuchProviderException || t instanceof java.security.KeyStoreException
                || t instanceof StrongBoxUnavailableException) {
            return UNAVAILABLE;
        }
        return ERROR;
    }

    private static native void nativeOnKeyringResult(long handle, int code, String secretOrNull);
}
