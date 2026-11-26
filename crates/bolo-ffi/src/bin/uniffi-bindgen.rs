//! UniFFI's bindings generator, built from this crate so its version always matches the
//! `uniffi` the library was built with. Used by `scripts/build-android.py`.

fn main() {
    uniffi::uniffi_bindgen_main()
}
