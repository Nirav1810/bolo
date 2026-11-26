package dev.bolo.android

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.bolo.ffi.Bolo
import dev.bolo.ffi.BoloListener
import dev.bolo.ffi.BoloPaths
import dev.bolo.ffi.BoloSettings
import dev.bolo.ffi.Dictation
import dev.bolo.ffi.ErrorKind
import dev.bolo.ffi.Notice
import dev.bolo.ffi.SecretException
import dev.bolo.ffi.State
import java.io.File
import java.security.KeyStore
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * The Keystore-backed [KeystoreSecretStore] on a real device or emulator
 * (`./gradlew connectedDebugAndroidTest`; CI runs only the JVM tests). It uses its own
 * key alias and preferences file, so the app's stored keys are never touched.
 */
@RunWith(AndroidJUnit4::class)
class KeystoreSecretStoreTest {
    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext
    private lateinit var store: KeystoreSecretStore

    @Before
    fun setUp() {
        Native.init(context)
        cleanUp()
        store = KeystoreSecretStore(context, ALIAS, PREFS)
    }

    @After
    fun cleanUp() {
        context.deleteSharedPreferences(PREFS)
        KeyStore.getInstance("AndroidKeyStore").apply { load(null) }.deleteEntry(ALIAS)
        File(context.cacheDir, "bolo-ffi-test").deleteRecursively()
    }

    @Test
    fun valuesRoundTripAndDelete() {
        assertNull(store.get("bolo/deepgram"))
        store.set("bolo/deepgram", "dg-secret-1234")
        store.set("bolo/anthropic", "sk-ant-ünïcode")
        assertEquals("dg-secret-1234", store.get("bolo/deepgram"))
        assertEquals("sk-ant-ünïcode", store.get("bolo/anthropic"))

        store.set("bolo/deepgram", "dg-replaced")
        assertEquals("dg-replaced", store.get("bolo/deepgram"))
        assertEquals("a new instance reads the same key", "dg-replaced", KeystoreSecretStore(context, ALIAS, PREFS).get("bolo/deepgram"))

        assertTrue(store.delete("bolo/deepgram"))
        assertFalse(store.delete("bolo/deepgram"))
        assertNull(store.get("bolo/deepgram"))
        assertEquals("sk-ant-ünïcode", store.get("bolo/anthropic"))
    }

    @Test
    fun valuesAreEncryptedAndBoundToTheirName() {
        store.set("bolo/groq", "gsk-plaintext-check")
        val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        val stored = prefs.getString("bolo/groq", null)!!
        assertFalse("ciphertext only", stored.contains("plaintext"))

        // Same value twice: a fresh IV each time.
        store.set("bolo/sarvam", "gsk-plaintext-check")
        assertFalse(stored == prefs.getString("bolo/sarvam", null))

        // A value copied under another name doesn't decrypt (the name is the GCM AAD).
        prefs.edit().putString("bolo/sarvam", stored).commit()
        try {
            store.get("bolo/sarvam")
            fail("a moved value decrypted")
        } catch (e: SecretException.Failed) {
            assertFalse("no value in the message", e.message.orEmpty().contains("plaintext"))
            assertTrue(e.message.orEmpty(), e.message.orEmpty().contains("Keystore read failed: AEADBadTagException"))
        }
    }

    @Test
    fun boloReadsAndWritesKeysThroughTheStore() {
        val dir = File(context.cacheDir, "bolo-ffi-test").apply { mkdirs() }
        val paths = BoloPaths(dir.absolutePath, dir.absolutePath, File(dir, "models").absolutePath)
        val first = Bolo(paths, store, Quiet)
        assertNull(first.apiKeyHint("deepgram"))
        assertEquals("abcd", first.setApiKey("deepgram", " dg-test-key-abcd "))
        assertEquals("dg-test-key-abcd", store.get("bolo/deepgram"))
        first.destroy()

        // A later Bolo (say, the keyboard after a restart) finds the key.
        val second = Bolo(paths, KeystoreSecretStore(context, ALIAS, PREFS), Quiet)
        assertEquals("abcd", second.apiKeyHint("deepgram"))
        second.setApiKey("deepgram", "")
        assertNull(store.get("bolo/deepgram"))
        second.destroy()
    }

    private object Quiet : BoloListener {
        override fun onLevel(rms: Float) {}
        override fun onPartial(text: String) {}
        override fun onFinal(dictation: Dictation) {}
        override fun onPolished(dictation: Dictation) {}
        override fun onState(state: State) {}
        override fun onError(kind: ErrorKind, message: String) {}
        override fun onSettingsChanged(settings: BoloSettings) {}
        override fun onNotice(notice: Notice) {}
    }

    private companion object {
        const val ALIAS = "bolo-secrets-test"
        const val PREFS = "bolo_secrets_test"
    }
}
