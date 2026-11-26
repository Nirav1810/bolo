package dev.bolo.android

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.os.SystemClock
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import dev.bolo.ffi.Bolo
import dev.bolo.ffi.BoloListener
import dev.bolo.ffi.BoloSettings
import dev.bolo.ffi.Dictation
import dev.bolo.ffi.DownloadPhase
import dev.bolo.ffi.ErrorKind
import dev.bolo.ffi.FfiException
import dev.bolo.ffi.ModelDownload
import dev.bolo.ffi.Notice
import dev.bolo.ffi.State
import java.util.Locale
import java.util.concurrent.Executors

/**
 * Downloads models in a foreground service of type `dataSync` (ticket 05), so leaving the
 * app doesn't kill a 500 MB download. It shows the progress in a notification with a
 * Cancel action and stops once the last queued download ends. Downloads run one at a time
 * through its own [Bolo]; progress also reaches every other `Bolo` in the process (the
 * settings screen's) through `onDownload`.
 *
 * Callers check free space and the metered network before starting one; Rust checks the
 * space again and refuses a download that doesn't fit.
 */
class ModelDownloadService : Service() {
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { Thread(it, "bolo-download") }
    private var bolo: Bolo? = null
    private var queued = 0
    @Volatile private var current: String? = null
    private var lastShown = 0L

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val id = intent?.getStringExtra(EXTRA_ID)
        when (intent?.action) {
            ACTION_DOWNLOAD -> if (id != null) enqueue(id)
            ACTION_CANCEL -> if (id != null) bolo?.cancelDownload(id)
        }
        if (queued == 0) stopSelf(startId)
        return START_NOT_STICKY
    }

    private fun enqueue(id: String) {
        val notification = notification(id, "Waiting to download…", null, ongoing = true)
        // Within 5 s of startForegroundService, before anything else.
        ServiceCompat.startForeground(this, NOTIFICATION_ID, notification, foregroundType())
        queued++
        worker.execute { download(id) }
    }

    private fun download(id: String) {
        current = id
        val started = SystemClock.elapsedRealtime()
        val (text, ok) = try {
            if (open().downloadModel(id)) "Downloaded" to true else "Download cancelled" to false
        } catch (e: FfiException) {
            Log.w(TAG, "download $id failed", e)
            "Download failed: ${e.message}" to false
        } catch (e: Exception) {
            // A Rust panic arrives as UniFFI's InternalException: keep the app alive.
            Log.e(TAG, "download $id crashed", e)
            "Download failed: ${e.message}" to false
        }
        val secs = (SystemClock.elapsedRealtime() - started) / 1000.0
        Log.i(TAG, "download $id: $text in ${String.format(Locale.ROOT, "%.1f", secs)} s")
        current = null
        main.post {
            queued--
            if (queued == 0) {
                ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_DETACH)
                manager().notify(NOTIFICATION_ID, notification(id, text, null, ongoing = false))
                stopSelf()
            }
            if (!ok) Log.i(TAG, "download $id ended without a model")
        }
    }

    /** The system stops a `dataSync` service after 6 h a day (API 35+): give up cleanly. */
    override fun onTimeout(startId: Int, fgsType: Int) {
        Log.w(TAG, "download time limit reached")
        current?.let { bolo?.cancelDownload(it) }
    }

    override fun onDestroy() {
        current?.let { bolo?.cancelDownload(it) }
        worker.shutdown()
        // A call still running keeps the native object alive until it returns.
        bolo?.destroy()
        bolo = null
        super.onDestroy()
    }

    private fun open(): Bolo = synchronized(this) {
        // HTTPS needs the TLS verifier's context first (ticket 08); `Bolo` refuses to open
        // without it. Idempotent, so the activity having done it already is fine.
        Native.init(this)
        bolo ?: Bolo(BoloFiles.paths(this), KeystoreSecretStore(this), listener).also { bolo = it }
    }

    private val listener = object : BoloListener {
        override fun onDownload(download: ModelDownload) = show(download)
        override fun onNotice(notice: Notice) {}
        override fun onLevel(rms: Float) {}
        override fun onPartial(text: String) {}
        override fun onFinal(dictation: Dictation) {}
        override fun onPolished(dictation: Dictation) {}
        override fun onState(state: State) {}
        override fun onError(kind: ErrorKind, message: String) {}
        override fun onSettingsChanged(settings: BoloSettings) {}
    }

    /** Progress into the notification: Android drops updates past a few a second. */
    private fun show(d: ModelDownload) {
        val now = SystemClock.elapsedRealtime()
        if (d.phase == DownloadPhase.DOWNLOADING && now - lastShown < NOTIFY_INTERVAL_MS) return
        lastShown = now
        val (text, percent) = when (d.phase) {
            DownloadPhase.DOWNLOADING -> {
                val percent = if (d.total > 0uL) (d.downloaded * 100uL / d.total).toInt() else 0
                "${megabytes(d.downloaded)} of ${megabytes(d.total)} MB" to percent
            }
            DownloadPhase.VERIFYING -> "Checking the download…" to null
            DownloadPhase.UNPACKING -> "Unpacking…" to null
            else -> return // the end is shown when downloadModel returns
        }
        if (d.phase != DownloadPhase.DOWNLOADING) Log.i(TAG, "download ${d.id}: ${d.phase}")
        manager().notify(NOTIFICATION_ID, notification(d.id, text, percent, ongoing = true))
    }

    private fun notification(id: String, text: String, percent: Int?, ongoing: Boolean): Notification {
        channel()
        val b = NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle("Bolo model: $id")
            .setContentText(text)
            .setOnlyAlertOnce(true)
            .setOngoing(ongoing)
            .setSilent(true)
            .setContentIntent(
                PendingIntent.getActivity(
                    this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
                ),
            )
        if (ongoing) {
            b.setProgress(100, percent ?: 0, percent == null)
            val cancel = Intent(this, ModelDownloadService::class.java).setAction(ACTION_CANCEL).putExtra(EXTRA_ID, id)
            b.addAction(
                0, "Cancel",
                PendingIntent.getService(
                    this, 1, cancel, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
                ),
            )
        } else {
            b.setAutoCancel(true)
        }
        return b.build()
    }

    private fun channel() {
        manager().createNotificationChannel(
            NotificationChannel(CHANNEL, "Model downloads", NotificationManager.IMPORTANCE_LOW),
        )
    }

    private fun manager() = getSystemService(NotificationManager::class.java)

    private fun foregroundType(): Int =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC else 0

    companion object {
        private const val TAG = "bolo"
        private const val CHANNEL = "downloads"
        private const val NOTIFICATION_ID = 5
        private const val NOTIFY_INTERVAL_MS = 500L
        private const val EXTRA_ID = "id"
        private const val ACTION_DOWNLOAD = "dev.bolo.android.action.DOWNLOAD_MODEL"
        private const val ACTION_CANCEL = "dev.bolo.android.action.CANCEL_DOWNLOAD"

        /** Download model [id] in the foreground service. Call it from the visible app. */
        fun start(context: Context, id: String) {
            val intent = Intent(context, ModelDownloadService::class.java).setAction(ACTION_DOWNLOAD).putExtra(EXTRA_ID, id)
            context.startForegroundService(intent)
        }

        fun megabytes(bytes: ULong): Long = ((bytes + 999_999uL) / 1_000_000uL).toLong()
    }
}
