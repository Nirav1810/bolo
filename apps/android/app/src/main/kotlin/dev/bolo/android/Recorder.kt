package dev.bolo.android

import android.Manifest
import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.util.Log
import androidx.annotation.RequiresPermission

/**
 * Records 16 kHz mono PCM16 with the `VOICE_RECOGNITION` source and hands ~20 ms frames to
 * [onFrame] on its own thread (ADR-0023). The engine is fed through `Bolo.pushAudio`.
 */
class Recorder(private val onFrame: (ByteArray) -> Unit) {
    private var record: AudioRecord? = null
    private var thread: Thread? = null
    @Volatile private var running = false

    val isRunning: Boolean get() = running

    @RequiresPermission(Manifest.permission.RECORD_AUDIO)
    fun start() {
        if (running) return
        val minBuffer = AudioRecord.getMinBufferSize(SAMPLE_RATE, CHANNEL, ENCODING)
        val r = AudioRecord(
            MediaRecorder.AudioSource.VOICE_RECOGNITION,
            SAMPLE_RATE,
            CHANNEL,
            ENCODING,
            maxOf(minBuffer, FRAME_BYTES * 8),
        )
        if (r.state != AudioRecord.STATE_INITIALIZED) {
            r.release()
            error("the microphone could not be opened")
        }
        r.startRecording()
        record = r
        running = true
        thread = Thread({
            val buffer = ByteArray(FRAME_BYTES)
            while (running) {
                val n = r.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING)
                if (n > 0) onFrame(if (n == buffer.size) buffer.copyOf() else buffer.copyOf(n))
                else if (n < 0) {
                    Log.w(TAG, "AudioRecord.read failed: $n")
                    break
                }
            }
        }, "bolo-recorder").apply {
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    fun stop() {
        if (!running) return
        running = false
        thread?.join(500)
        thread = null
        record?.run {
            stop()
            release()
        }
        record = null
    }

    companion object {
        private const val TAG = "bolo"
        const val SAMPLE_RATE = 16_000
        private const val CHANNEL = AudioFormat.CHANNEL_IN_MONO
        private const val ENCODING = AudioFormat.ENCODING_PCM_16BIT
        /** 20 ms of 16-bit mono at 16 kHz. */
        const val FRAME_BYTES = SAMPLE_RATE / 50 * 2
    }
}
