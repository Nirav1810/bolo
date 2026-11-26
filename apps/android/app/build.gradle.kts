import org.gradle.internal.os.OperatingSystem

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val repoRoot: File = rootDir.parentFile.parentFile

// One version for every Bolo artifact (ADR-0013): read it from the workspace Cargo.toml.
val boloVersion: String = run {
    val cargoToml = repoRoot.resolve("Cargo.toml").readText()
    val section = cargoToml.substringAfter("[workspace.package]")
    Regex("""(?m)^version\s*=\s*"([^"]+)"""").find(section)?.groupValues?.get(1)
        ?: error("no version in [workspace.package] of Cargo.toml")
}

// major.minor.patch → MMmmpp00; the last two digits are left for rebuilds of one version.
val boloVersionCode: Int = run {
    val (major, minor, patch) = boloVersion.substringBefore('-').split('.').map { it.toInt() }
    major * 1_000_000 + minor * 10_000 + patch * 100
}

android {
    namespace = "dev.bolo.android"
    // AndroidX (core 1.19, Compose 1.13) needs API 37.1+ to compile against.
    compileSdk {
        version = release(37) { minorApiLevel = 2 }
    }
    ndkVersion = providers.environmentVariable("ANDROID_NDK_VERSION").getOrElse("30.0.16248370")

    defaultConfig {
        applicationId = "dev.bolo.android"
        minSdk = 28
        targetSdk = 36
        versionName = boloVersion
        versionCode = boloVersionCode
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    buildTypes {
        debug {
            ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
        }
        release {
            // Signing and store builds are M6.
            isMinifyEnabled = false
            ndk { abiFilters += listOf("arm64-v8a") }
        }
    }

    sourceSets {
        getByName("main") {
            // Built by scripts/build-android.py (the cargoBuild tasks below).
            jniLibs.directories.add("build/rustJniLibs")
            kotlin.directories.add("build/generated/uniffi")
        }
    }

    buildFeatures {
        compose = true
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    lint {
        // UniFFI's generated bindings aren't ours to lint.
        checkGeneratedSources = false
        abortOnError = true
    }
}

kotlin {
    jvmToolchain(17)
}

// The native library and its Kotlin bindings, rebuilt before every build. Cargo decides
// what is stale, so a no-op run takes a few seconds.
fun registerCargoBuild(name: String, release: Boolean) = tasks.register<Exec>(name) {
    group = "build"
    description = "Builds libbolo_ffi.so and its Kotlin bindings" + if (release) " (release)" else ""
    workingDir = repoRoot
    val python = providers.gradleProperty("bolo.python")
        .getOrElse(if (OperatingSystem.current().isWindows) "python" else "python3")
    commandLine(listOf(python, "scripts/build-android.py") + if (release) listOf("--release") else emptyList())
}

val cargoBuildDebug = registerCargoBuild("cargoBuildDebug", release = false)
val cargoBuildRelease = registerCargoBuild("cargoBuildRelease", release = true)

tasks.configureEach {
    when (name) {
        "preDebugBuild" -> dependsOn(cargoBuildDebug)
        "preReleaseBuild" -> dependsOn(cargoBuildRelease)
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.material3)
    implementation(libs.compose.ui.tooling.preview)
    debugImplementation(libs.compose.ui.tooling)
    implementation(libs.jna) { artifact { type = "aar" } }

    testImplementation(libs.junit)
    // Instrumented tests (`connectedDebugAndroidTest`) run on the emulator, not in CI.
    androidTestImplementation(libs.junit)
    androidTestImplementation(libs.androidx.test.ext.junit)
    androidTestImplementation(libs.androidx.test.runner)
}
