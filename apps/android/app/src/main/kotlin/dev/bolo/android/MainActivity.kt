package dev.bolo.android

import android.Manifest
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.content.pm.PackageManager
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.util.Log
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import dev.bolo.ffi.Bolo
import dev.bolo.ffi.BoloListener
import dev.bolo.ffi.BoloPaths
import dev.bolo.ffi.BoloSettings
import dev.bolo.ffi.Dictation
import dev.bolo.ffi.ErrorKind
import dev.bolo.ffi.FfiException
import dev.bolo.ffi.FieldKind
import dev.bolo.ffi.State
import java.io.File

/**
 * Ticket 01's debug screen: hold the button to dictate with the engine. The model must
 * already be in `noBackupFilesDir/models` (see apps/android/README.md). The keyboard
 * itself is ticket 06.
 *
 * Bolo reads `settings.toml` from `filesDir` and its keys from [KeystoreSecretStore]
 * (ticket 04); the engine field writes the settings' engine chain.
 */
class MainActivity : ComponentActivity() {
    private val main = Handler(Looper.getMainLooper())
    private var bolo: Bolo? = null
    private val recorder = Recorder { frame -> bolo?.pushAudio(frame) }

    private var engine by mutableStateOf("local:moonshine-tiny")
    private var status by mutableStateOf("Hold the button and speak.")
    private var text by mutableStateOf("")
    private var level by mutableFloatStateOf(0f)
    private var listening by mutableStateOf(false)
    @Volatile private var replaying = false

    private val askMic = registerForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        status = if (granted) "Hold the button and speak." else "Bolo needs the microphone to dictate."
    }

    private val listener = object : BoloListener {
        override fun onLevel(rms: Float) = post { level = rms }
        override fun onPartial(text: String) = post { this@MainActivity.text = text }
        override fun onFinal(dictation: Dictation) = post {
            text = dictation.plain
            status = "${dictation.engineId} · ${dictation.language} · release → text ${dictation.totalMs.toInt()} ms"
            Log.i(TAG, "final stt_ms=${dictation.sttMs} total_ms=${dictation.totalMs} engine=${dictation.engineId}")
            // The debug replay prints its text so a run can be checked from logcat.
            if (replaying) Log.i(TAG, "replay text [${dictation.language}]: ${dictation.plain}")
        }
        override fun onPolished(dictation: Dictation) = post { text = dictation.plain }
        override fun onState(state: State) = post { if (state == State.PROCESSING) status = "Finishing…" }
        override fun onError(kind: ErrorKind, message: String) = post { status = "Error ($kind): $message" }
        override fun onSettingsChanged(settings: BoloSettings) {
            Log.i(TAG, "settings changed: engines=${settings.engines} language=${settings.language}")
        }
    }

    private fun post(block: () -> Unit) {
        main.post(block)
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            askMic.launch(Manifest.permission.RECORD_AUDIO)
        }
        setContent { DebugScreen() }
        replayFrom(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        replayFrom(intent)
    }

    /**
     * Debuggable builds only: dictate a 16 kHz mono PCM16 WAV in real time through the same
     * `pushAudio` path the microphone uses, so a dictation runs with no one speaking:
     * `adb shell am start -n dev.bolo.android/.MainActivity --es replay /data/local/tmp/x.wav
     * [--es engine local:nemotron-stream]`. The result is logged under the `bolo` tag.
     */
    private fun replayFrom(intent: Intent?) {
        val path = intent?.getStringExtra("replay") ?: return
        if (applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE == 0) return
        intent.getStringExtra("engine")?.let { engine = it }
        val pcm = try {
            WavFile.readPcm16Mono16k(File(path))
        } catch (e: Exception) {
            status = "Replay failed: ${e.message}"
            return
        }
        val b = try {
            engineFor(engine.trim())
        } catch (e: FfiException) {
            status = "Error: ${e.message}"
            return
        }
        text = ""
        status = "Replaying ${File(path).name}…"
        replaying = true
        Thread({
            try {
                b.start("dev.bolo.replay", FieldKind.NORMAL)
                post { listening = true }
                val silence = ByteArray(Recorder.FRAME_BYTES * 15)
                for (chunk in listOf(pcm, silence)) {
                    var at = 0
                    while (at < chunk.size) {
                        val end = minOf(at + Recorder.FRAME_BYTES, chunk.size)
                        b.pushAudio(chunk.copyOfRange(at, end))
                        at = end
                        Thread.sleep(20)
                    }
                }
                Log.i(TAG, "replay pushed ${pcm.size / 2} samples, stopping")
                b.stop()
            } catch (e: FfiException) {
                Log.w(TAG, "replay failed", e)
                post { status = "Error: ${e.message}" }
            } finally {
                post { listening = false; level = 0f }
            }
        }, "bolo-replay").start()
    }

    override fun onStop() {
        super.onStop()
        recorder.stop()
        listening = false
        bolo?.cancel()
    }

    override fun onTrimMemory(level: Int) {
        super.onTrimMemory(level)
        bolo?.release()
    }

    override fun onDestroy() {
        super.onDestroy()
        bolo?.destroy()
        bolo = null
    }

    /** The one `Bolo`, opened on first use, with `spec` as its engine chain. */
    private fun engineFor(spec: String): Bolo {
        val b = bolo ?: Bolo(
            BoloPaths(
                configDir = filesDir.absolutePath,
                dataDir = filesDir.absolutePath,
                modelsDir = File(noBackupFilesDir, "models").apply { mkdirs() }.absolutePath,
            ),
            KeystoreSecretStore(this),
            listener,
        ).also { bolo = it }
        val settings = b.settings()
        if (settings.engines != listOf(spec)) b.saveSettings(settings.copy(engines = listOf(spec)))
        return b
    }

    private fun press() {
        if (checkSelfPermission(Manifest.permission.RECORD_AUDIO) != PackageManager.PERMISSION_GRANTED) {
            askMic.launch(Manifest.permission.RECORD_AUDIO)
            return
        }
        try {
            val b = engineFor(engine.trim())
            recorder.start()
            b.start(packageName, FieldKind.NORMAL)
            listening = true
            text = ""
            status = "Listening…"
        } catch (e: FfiException) {
            recorder.stop()
            status = "Error: ${e.message}"
        } catch (e: IllegalStateException) {
            status = "Error: ${e.message}"
        }
    }

    private fun release() {
        if (!listening) return
        listening = false
        level = 0f
        try {
            bolo?.stop()
        } catch (e: FfiException) {
            status = "Error: ${e.message}"
        }
        // Keep recording a moment so the engine gets the tail it pads with.
        main.postDelayed({ if (!listening) recorder.stop() }, 300)
    }

    @Composable
    private fun DebugScreen() {
        val dark = isSystemInDarkTheme()
        val scheme = if (dark) darkColorScheme(primary = Color(0xFF7C9BFF)) else lightColorScheme(primary = Color(0xFF2F5BEA))
        MaterialTheme(colorScheme = scheme) {
            Surface(Modifier.fillMaxSize()) {
                Column(
                    Modifier.safeDrawingPadding().padding(24.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    Text("Bolo engine check", style = MaterialTheme.typography.headlineSmall)
                    OutlinedTextField(
                        value = engine,
                        onValueChange = { engine = it },
                        label = { Text("Engine") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                    Text(status, style = MaterialTheme.typography.bodyMedium)
                    Text(
                        text.ifEmpty { "…" },
                        style = MaterialTheme.typography.bodyLarge,
                        modifier = Modifier.fillMaxWidth().weight(1f),
                    )
                    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
                        val live = Color(0xFFE5484D)
                        Box(
                            Modifier
                                .size((96 + 48 * level.coerceIn(0f, 1f)).dp)
                                .clip(CircleShape)
                                .background(if (listening) live else MaterialTheme.colorScheme.primary)
                                .pointerInput(Unit) {
                                    awaitEachGesture {
                                        awaitFirstDown()
                                        press()
                                        waitForUpOrCancellation()
                                        release()
                                    }
                                },
                            contentAlignment = Alignment.Center,
                        ) {
                            Text(if (listening) "Listening" else "Hold", color = Color.White)
                        }
                    }
                    Spacer(Modifier.height(8.dp))
                }
            }
        }
    }

    private companion object {
        const val TAG = "bolo"
    }
}
