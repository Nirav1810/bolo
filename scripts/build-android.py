#!/usr/bin/env python3
"""Build libbolo_ffi.so for Android and generate its Kotlin bindings (ADR-0023).

For each ABI: cargo-ndk builds `bolo-ffi` against sherpa-onnx's Android libraries
(.cache/sherpa-lib-android/<abi>, from `fetch-sherpa-libs.py --android`), and the .so plus
the sherpa/onnxruntime libraries it loads are copied to
apps/android/app/build/rustJniLibs/<abi>/. The Kotlin bindings are generated from the
built library into apps/android/app/build/generated/uniffi/.

Gradle runs this before every build (`cargoBuild`), so `./gradlew assembleDebug` is the
one command. Run it by hand to build without Gradle:

    python scripts/build-android.py [--release] [--abi arm64-v8a --abi x86_64]

Debug builds default to arm64-v8a + x86_64 (the emulator); release builds to arm64-v8a.
Needs: the Android NDK (ANDROID_NDK_HOME, or the newest under ANDROID_HOME/ndk), cargo-ndk,
and `rustup target add aarch64-linux-android x86_64-linux-android`.
"""
import argparse, os, pathlib, shutil, subprocess, sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
APP = ROOT / "apps" / "android" / "app"
JNI_OUT = APP / "build" / "rustJniLibs"
BINDINGS_OUT = APP / "build" / "generated" / "uniffi"
SHERPA_ANDROID = ROOT / ".cache" / "sherpa-lib-android"
MIN_SDK = "28"
# The libraries libbolo_ffi.so needs at run time, from sherpa's Android archive.
RUNTIME_LIBS = ("libsherpa-onnx-c-api.so", "libonnxruntime.so")
TRIPLES = {"arm64-v8a": "aarch64-linux-android", "x86_64": "x86_64-linux-android"}


def run(cmd, env=None):
    print("+", " ".join(cmd), flush=True)
    subprocess.run(cmd, cwd=ROOT, env=env, check=True)


def cargo():
    home = pathlib.Path.home() / ".cargo" / "bin"
    found = shutil.which("cargo") or shutil.which("cargo", path=str(home))
    if not found:
        sys.exit("cargo not found (install Rust, or add ~/.cargo/bin to PATH)")
    return found


def ndk_env():
    env = dict(os.environ)
    env["PATH"] = str(pathlib.Path.home() / ".cargo" / "bin") + os.pathsep + env.get("PATH", "")
    if not env.get("ANDROID_NDK_HOME"):
        sdk = env.get("ANDROID_HOME") or env.get("ANDROID_SDK_ROOT")
        ndks = sorted((pathlib.Path(sdk) / "ndk").glob("*")) if sdk else []
        if not ndks:
            sys.exit("no NDK: set ANDROID_NDK_HOME or install one under ANDROID_HOME/ndk")
        env["ANDROID_NDK_HOME"] = str(ndks[-1])
    return env


def build_abi(abi, release, env):
    sherpa = SHERPA_ANDROID / abi
    if not all((sherpa / lib).is_file() for lib in RUNTIME_LIBS):
        run([sys.executable, "-I", str(ROOT / "scripts" / "fetch-sherpa-libs.py"), "--android"])
    env = dict(env, SHERPA_ONNX_LIB_DIR=str(sherpa))
    cmd = [cargo(), "ndk", "-t", abi, "-P", MIN_SDK, "build", "-p", "bolo-ffi"]
    if release:
        cmd.append("--release")
    run(cmd, env)
    profile = "release" if release else "debug"
    built = ROOT / "target" / TRIPLES[abi] / profile / "libbolo_ffi.so"
    out = JNI_OUT / abi
    if out.exists():
        shutil.rmtree(out)
    out.mkdir(parents=True)
    shutil.copy2(built, out / built.name)
    for lib in RUNTIME_LIBS:
        shutil.copy2(sherpa / lib, out / lib)
    return built


def bindings(library):
    if BINDINGS_OUT.exists():
        shutil.rmtree(BINDINGS_OUT)
    # The generator runs on the host, so it must not see the Android SHERPA_ONNX_LIB_DIR.
    env = {k: v for k, v in os.environ.items() if k != "SHERPA_ONNX_LIB_DIR"}
    run([cargo(), "run", "-q", "-p", "bolo-ffi", "--features", "bindgen", "--bin", "uniffi-bindgen", "--",
         "generate", "--library", str(library), "--language", "kotlin", "--no-format",
         "--out-dir", str(BINDINGS_OUT)], env)


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--release", action="store_true")
    ap.add_argument("--abi", action="append", choices=sorted(TRIPLES))
    args = ap.parse_args()
    abis = args.abi or (["arm64-v8a"] if args.release else ["arm64-v8a", "x86_64"])
    env = ndk_env()
    built = [build_abi(abi, args.release, env) for abi in abis]
    bindings(built[0])
    print(f"built {', '.join(abis)} into {JNI_OUT}, bindings in {BINDINGS_OUT}")


if __name__ == "__main__":
    main()
