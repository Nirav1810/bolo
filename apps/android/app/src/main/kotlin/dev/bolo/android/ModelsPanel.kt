package dev.bolo.android

import android.Manifest
import android.content.pm.PackageManager
import android.net.ConnectivityManager
import android.os.Build
import android.os.storage.StorageManager
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import dev.bolo.android.ModelDownloadService.Companion.megabytes
import dev.bolo.ffi.Bolo
import dev.bolo.ffi.DownloadPhase
import dev.bolo.ffi.FfiException
import dev.bolo.ffi.ModelDownload
import dev.bolo.ffi.ModelInfo

/**
 * Ticket 05's debug Models section: the phone's tier, the recommended model, and
 * download / cancel / delete with a progress bar. The real Models page (ticket 07) waits
 * for the approved mockups.
 *
 * Before a download it checks free space (Rust's `spaceNeeded`: the archive plus the
 * unpacked model and 20%) and asks first on a metered network. On API 33+ it asks for
 * the notification permission so the download's progress notification shows; the
 * download runs either way.
 */
class ModelsPanel(private val activity: ComponentActivity, private val bolo: () -> Bolo) {
    private var models by mutableStateOf(emptyList<ModelInfo>())
    private val progress = mutableStateMapOf<String, ModelDownload>()
    private var summary by mutableStateOf("")
    private var message by mutableStateOf("")
    private var confirm by mutableStateOf<ModelInfo?>(null)
    private var waiting: ModelInfo? = null
    private val ramMb by lazy { BoloFiles.totalRamMb(activity) }

    private val askNotifications = activity.registerForActivityResult(ActivityResultContracts.RequestPermission()) {
        waiting?.let { start(it, asked = true) }
        waiting = null
    }

    fun refresh() {
        try {
            val b = bolo()
            models = b.models()
            val language = b.settings().language
            val tier = b.tier(ramMb.toULong())
            val recommended = b.recommend(ramMb.toULong(), language)?.id ?: "nothing local fits"
            summary = "$ramMb MiB: $tier tier. For $language: $recommended."
        } catch (e: FfiException) {
            message = "Error: ${e.message}"
        }
    }

    /** `BoloListener.onDownload`, on the main thread. */
    fun onDownload(d: ModelDownload) {
        when (d.phase) {
            DownloadPhase.DOWNLOADING, DownloadPhase.VERIFYING, DownloadPhase.UNPACKING -> progress[d.id] = d
            DownloadPhase.DONE -> end(d.id, "${d.id} is installed.")
            DownloadPhase.FAILED -> end(d.id, "${d.id}: ${d.error}")
            DownloadPhase.CANCELLED -> end(d.id, "${d.id}: cancelled. The next download resumes.")
        }
    }

    private fun end(id: String, text: String) {
        progress.remove(id)
        message = text
        refresh()
    }

    private fun download(m: ModelInfo) {
        // What can be allocated, counting cached data the system would clear for us.
        val storage = activity.getSystemService(StorageManager::class.java)
        val free = storage.getAllocatableBytes(storage.getUuidForPath(activity.noBackupFilesDir)).toULong()
        if (free < m.spaceNeeded) {
            message = "${m.id} needs ${megabytes(m.spaceNeeded)} MB free; there are ${megabytes(free)} MB."
            return
        }
        val network = activity.getSystemService(ConnectivityManager::class.java)
        if (network.isActiveNetworkMetered) confirm = m else start(m)
    }

    private fun start(m: ModelInfo, asked: Boolean = false) {
        if (!asked && Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            waiting = m
            askNotifications.launch(Manifest.permission.POST_NOTIFICATIONS)
            return
        }
        message = ""
        progress[m.id] = ModelDownload(m.id, DownloadPhase.DOWNLOADING, 0uL, m.downloadBytes, null)
        ModelDownloadService.start(activity, m.id)
    }

    private fun delete(m: ModelInfo) {
        message = try {
            bolo().deleteModel(m.id)
            "${m.id} deleted."
        } catch (e: FfiException) {
            "Error: ${e.message}"
        }
        refresh()
    }

    @Composable
    fun Content() {
        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text("Models", style = MaterialTheme.typography.titleMedium)
            Text(summary, style = MaterialTheme.typography.bodySmall)
            for (m in models) ModelRow(m)
            if (message.isNotEmpty()) Text(message, style = MaterialTheme.typography.bodySmall)
        }
        confirm?.let { m ->
            AlertDialog(
                onDismissRequest = { confirm = null },
                title = { Text("Download on mobile data?") },
                text = { Text("${m.name} is ${megabytes(m.downloadBytes)} MB, and you're on mobile data.") },
                confirmButton = {
                    TextButton(onClick = { confirm = null; start(m) }) { Text("Download") }
                },
                dismissButton = { TextButton(onClick = { confirm = null }) { Text("Not now") } },
            )
        }
    }

    @Composable
    private fun ModelRow(m: ModelInfo) {
        val d = progress[m.id]
        Column {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(Modifier.weight(1f)) {
                    Text(m.id + if (m.recommended) " (recommended)" else "", style = MaterialTheme.typography.bodyMedium)
                    val state = when {
                        d != null -> d.phase.name.lowercase()
                        m.installed -> "installed"
                        else -> "${megabytes(m.downloadBytes)} MB"
                    }
                    Text("$state · ${m.license}", style = MaterialTheme.typography.bodySmall)
                }
                when {
                    d != null || m.downloading ->
                        TextButton(onClick = { bolo().cancelDownload(m.id) }) { Text("Cancel") }
                    m.installed -> TextButton(onClick = { delete(m) }) { Text("Delete") }
                    else -> TextButton(onClick = { download(m) }) { Text("Download") }
                }
            }
            if (d != null) {
                if (d.phase == DownloadPhase.DOWNLOADING && d.total > 0uL) {
                    val fraction = d.downloaded.toFloat() / d.total.toFloat()
                    LinearProgressIndicator(progress = { fraction }, modifier = Modifier.fillMaxWidth())
                } else {
                    LinearProgressIndicator(modifier = Modifier.fillMaxWidth())
                }
            }
        }
    }
}
