package dev.bolo.android

import android.content.Context

/**
 * The JNI side of `libbolo_ffi.so`; everything else goes through UniFFI's bindings (JNA).
 *
 * Call [init] once per process before opening the first `Bolo`: the cloud engines,
 * Claude polish and model downloads check certificates with the system trust store
 * through `rustls-platform-verifier`, which needs the app's [Context] (ADR-0023). `Bolo`
 * refuses to open until it has run.
 */
object Native {
    init {
        System.loadLibrary("bolo_ffi")
    }

    /**
     * Hands the application context to the TLS verifier. Idempotent: later calls return
     * at once. Throws a `RuntimeException` if the verifier can't use the context.
     */
    fun init(context: Context) = initTls(context.applicationContext ?: context)

    // `Java_dev_bolo_android_Native_initTls` in crates/bolo-ffi/src/tls.rs. The verifier
    // keeps the context for the life of the process, so it must be the application's.
    @JvmStatic
    private external fun initTls(context: Context)
}
