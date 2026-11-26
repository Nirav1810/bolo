package dev.bolo.android

import android.content.Context
import android.content.SharedPreferences
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import dev.bolo.ffi.SecretException
import dev.bolo.ffi.SecretStore
import java.security.GeneralSecurityException
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Bolo's API keys (ADR-0023): each value is encrypted with an AES-256-GCM key that lives
 * in the Android Keystore (never exported, no user authentication, so the keyboard can
 * read keys without a prompt), and the ciphertext is kept in private SharedPreferences.
 *
 * A stored value is base64(iv ‖ ciphertext+tag); the entry's name is the GCM associated
 * data, so a value copied to another name doesn't decrypt. Values never go into a log or
 * an exception message.
 */
class KeystoreSecretStore(
    context: Context,
    private val alias: String = KEY_ALIAS,
    prefsName: String = PREFS,
) : SecretStore {
    private val prefs: SharedPreferences = context.getSharedPreferences(prefsName, Context.MODE_PRIVATE)

    override fun get(key: String): String? = guard("read") {
        val stored = prefs.getString(key, null) ?: return@guard null
        val blob = Base64.decode(stored, Base64.NO_WRAP)
        if (blob.size <= IV_BYTES) throw GeneralSecurityException("stored value is too short")
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, secretKey(), GCMParameterSpec(TAG_BITS, blob, 0, IV_BYTES))
        cipher.updateAAD(key.toByteArray(Charsets.UTF_8))
        String(cipher.doFinal(blob, IV_BYTES, blob.size - IV_BYTES), Charsets.UTF_8)
    }

    override fun set(key: String, value: String) = guard("write") {
        val cipher = Cipher.getInstance(TRANSFORMATION)
        // The Keystore picks a fresh random IV for every encryption.
        cipher.init(Cipher.ENCRYPT_MODE, secretKey())
        cipher.updateAAD(key.toByteArray(Charsets.UTF_8))
        val sealed = cipher.doFinal(value.toByteArray(Charsets.UTF_8))
        val blob = cipher.iv + sealed
        if (!prefs.edit().putString(key, Base64.encodeToString(blob, Base64.NO_WRAP)).commit()) {
            throw GeneralSecurityException("SharedPreferences commit failed")
        }
    }

    override fun delete(key: String): Boolean = guard("delete") {
        if (!prefs.contains(key)) return@guard false
        if (!prefs.edit().remove(key).commit()) throw GeneralSecurityException("SharedPreferences commit failed")
        true
    }

    /** The Keystore key, created on first use. */
    private fun secretKey(): SecretKey {
        val keyStore = KeyStore.getInstance(ANDROID_KEYSTORE).apply { load(null) }
        (keyStore.getKey(alias, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, ANDROID_KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(alias, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .setRandomizedEncryptionRequired(true)
                .build(),
        )
        return generator.generateKey()
    }

    /**
     * Runs [block], turning failures into [SecretException.Failed] that name the operation
     * and the exception type only: a provider's message could quote what it was given.
     */
    private inline fun <T> guard(what: String, block: () -> T): T = try {
        block()
    } catch (e: SecretException) {
        throw e
    } catch (e: Exception) {
        throw SecretException.Failed("Keystore $what failed: ${e.javaClass.simpleName}")
    }

    companion object {
        const val KEY_ALIAS = "bolo-secrets"
        const val PREFS = "bolo_secrets"
        private const val ANDROID_KEYSTORE = "AndroidKeyStore"
        private const val TRANSFORMATION = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12
        private const val TAG_BITS = 128
    }
}
