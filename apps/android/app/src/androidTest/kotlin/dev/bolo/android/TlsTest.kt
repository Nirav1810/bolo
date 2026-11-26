package dev.bolo.android

import android.content.Context
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import dev.bolo.ffi.FfiException
import dev.bolo.ffi.probeHttps
import org.junit.Assert.assertTrue
import org.junit.Assert.fail
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

/**
 * TLS through `rustls-platform-verifier`'s Android component (ticket 08). Needs the
 * network, so it runs locally on the emulator only, never in CI.
 */
@RunWith(AndroidJUnit4::class)
class TlsTest {
    private val context: Context = InstrumentationRegistry.getInstrumentation().targetContext

    @Before
    fun setUp() {
        Native.init(context)
    }

    @Test
    fun initIsIdempotent() {
        Native.init(context)
        Native.init(context.applicationContext)
    }

    @Test
    fun theProvidersHandshakeAndAnswer() {
        // Without a key each provider answers 401/403/404: any status means the TLS
        // handshake and the certificate check passed.
        for (url in PROVIDERS) {
            val status = probeHttps(url).toInt()
            assertTrue("$url answered $status", status in 200..599)
        }
    }

    @Test
    fun aBadCertificateIsRefused() {
        for (url in listOf("https://expired.badssl.com/", "https://wrong.host.badssl.com/", "https://self-signed.badssl.com/")) {
            try {
                val status = probeHttps(url)
                fail("$url was accepted ($status)")
            } catch (e: FfiException.Network) {
                assertTrue(e.message.orEmpty(), e.message.orEmpty().contains("certificate", ignoreCase = true))
            }
        }
    }

    private companion object {
        val PROVIDERS = listOf(
            "https://api.deepgram.com/v1/projects",
            "https://api.groq.com/openai/v1/models",
            "https://api.sarvam.ai/",
            "https://api.anthropic.com/v1/models",
            // Model downloads (ticket 05).
            "https://github.com/",
            "https://huggingface.co/",
        )
    }
}
