//! TLS for the cloud engines, polish and model downloads (ADR-0023).
//!
//! Every HTTPS and WebSocket connection checks certificates with
//! `rustls-platform-verifier`, which on Android calls the system's `X509TrustManager`
//! through its Kotlin component (`org.rustls:rustls-platform-verifier`, added by Gradle).
//! It needs the app's `Context` once per process, before the first connection, and
//! panics on a connection made without it (an abort in release builds). UniFFI loads
//! this library through JNA, which has no `JNIEnv`, so Kotlin hands the `Context` over
//! through one plain JNI function: `dev.bolo.android.Native.initTls(context)`.
//! [`crate::Bolo::new`] refuses to open until that has run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::FfiError;

static READY: AtomicBool = AtomicBool::new(!cfg!(target_os = "android"));

/// Whether certificates can be checked: always, except on Android before
/// `Native.initTls` has run.
pub fn ready() -> bool {
    READY.load(Ordering::Acquire)
}

/// `Native.initTls(context)`: hands the app's `Context` to the verifier. Idempotent:
/// the verifier keeps the first `Context` it gets, and later calls return at once. A
/// failure throws a `RuntimeException` in Kotlin.
#[cfg(target_os = "android")]
#[jni::jni_mangle("dev.bolo.android.Native")]
pub fn init_tls<'caller>(
    mut env: jni::EnvUnowned<'caller>,
    _class: jni::objects::JClass<'caller>,
    context: jni::objects::JObject<'caller>,
) {
    use jni::errors::ThrowRuntimeExAndDefault;

    if ready() {
        return;
    }
    env.with_env(|env| -> Result<(), jni::errors::Error> {
        rustls_platform_verifier::android::init_with_env(env, context)?;
        READY.store(true, Ordering::Release);
        crate::init_logging();
        tracing::info!("TLS verifier ready");
        Ok(())
    })
    .resolve::<ThrowRuntimeExAndDefault>();
}

/// Makes one HTTPS request to `url` with the client the cloud engines use (rustls, ring,
/// the platform verifier) and returns the HTTP status. Any status means the TLS
/// handshake and certificate check passed; a bad certificate or no network is
/// [`FfiError::Network`]. For the instrumented TLS test and connection checks. Blocks
/// for up to 10 s: don't call it on the main thread.
#[uniffi::export]
pub fn probe_https(url: String) -> Result<u16, FfiError> {
    if !ready() {
        return Err(not_ready());
    }
    let network = |detail: String| FfiError::Network { detail };
    let client = bolo_stt::cloud::http_client().map_err(|e| network(e.to_string()))?;
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().map_err(|e| network(e.to_string()))?;
    runtime.block_on(async {
        let response =
            client.get(&url).timeout(Duration::from_secs(10)).send().await.map_err(|e| network(describe(&e)))?;
        Ok(response.status().as_u16())
    })
}

pub(crate) fn not_ready() -> FfiError {
    FfiError::Engine {
        kind: crate::ErrorKind::Config,
        detail: "TLS isn't set up: call Native.initTls(context) before opening Bolo".into(),
    }
}

/// The error and its causes: reqwest's own message hides the TLS reason in its source.
fn describe(e: &dyn std::error::Error) -> String {
    let mut text = e.to_string();
    let mut source = e.source();
    while let Some(s) = source {
        text.push_str(": ");
        text.push_str(&s.to_string());
        source = s.source();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_hosts_need_no_init() {
        assert!(ready());
    }

    #[test]
    fn a_bad_address_is_a_network_error_not_a_panic() {
        match probe_https("https://127.0.0.1:9/".into()) {
            Err(FfiError::Network { detail }) => assert!(!detail.is_empty()),
            other => panic!("expected a network error, got {other:?}"),
        }
    }
}
