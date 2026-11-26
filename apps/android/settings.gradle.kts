pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // rustls-platform-verifier's Kotlin component (TLS for the cloud engines, ADR-0023).
        // Its maintainers publish it only here; app/build.gradle.kts pins the version to
        // the rustls-platform-verifier-android crate in Cargo.lock.
        exclusiveContent {
            forRepository {
                maven {
                    name = "rustlsPlatformVerifier"
                    url = uri("https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/")
                }
            }
            filter { includeGroup("org.rustls") }
        }
    }
}

rootProject.name = "bolo-android"
include(":app")
