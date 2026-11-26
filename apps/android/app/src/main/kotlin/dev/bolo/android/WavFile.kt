package dev.bolo.android

import java.io.File
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Just enough WAV reading for the debug replay: 16 kHz mono PCM16, any chunk order. */
object WavFile {
    fun readPcm16Mono16k(file: File): ByteArray {
        val bytes = file.readBytes()
        val buf = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        require(bytes.size >= 12 && String(bytes, 0, 4) == "RIFF" && String(bytes, 8, 4) == "WAVE") { "not a WAV file" }
        var at = 12
        var format: Triple<Int, Int, Int>? = null
        while (at + 8 <= bytes.size) {
            val id = String(bytes, at, 4)
            val size = buf.getInt(at + 4)
            val body = at + 8
            when (id) {
                "fmt " -> format = Triple(buf.getShort(body + 2).toInt(), buf.getInt(body + 4), buf.getShort(body + 14).toInt())
                "data" -> {
                    val (channels, rate, bits) = format ?: error("data before fmt")
                    require(channels == 1 && rate == Recorder.SAMPLE_RATE && bits == 16) {
                        "need 16 kHz mono 16-bit, got $rate Hz, $channels ch, $bits bit"
                    }
                    return bytes.copyOfRange(body, minOf(body + size, bytes.size))
                }
            }
            at = body + size + (size and 1)
        }
        error("no data chunk")
    }
}
