package dev.bolo.android

import android.app.ActivityManager
import android.content.Context
import dev.bolo.ffi.BoloPaths
import java.io.File

/** Where every `Bolo` in the app keeps its files (ADR-0023). */
object BoloFiles {
    /**
     * `settings.toml` and `history.sqlite` in `filesDir`; models in `noBackupFilesDir/models`,
     * which backups skip (they're hundreds of MB and downloadable again).
     */
    fun paths(context: Context): BoloPaths = BoloPaths(
        configDir = context.filesDir.absolutePath,
        dataDir = context.filesDir.absolutePath,
        modelsDir = File(context.noBackupFilesDir, "models").apply { mkdirs() }.absolutePath,
    )

    /** The phone's RAM in MiB, as `MemTotal` reports it: what the model tier goes by. */
    fun totalRamMb(context: Context): Long {
        val info = ActivityManager.MemoryInfo()
        context.getSystemService(ActivityManager::class.java).getMemoryInfo(info)
        return info.totalMem / (1024 * 1024)
    }
}
