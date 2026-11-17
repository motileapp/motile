pluginManagement {
    repositories {
        google()
        // Maven Central through Google's copy first: Maven Central refuses CI runners that ask too often.
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        // Maven Central through Google's copy first: Maven Central refuses CI runners that ask too often.
        maven("https://maven-central.storage-download.googleapis.com/maven2/")
        mavenCentral()
        // The Kotlin half of rustls-platform-verifier, which checks the core's certificates.
        exclusiveContent {
            forRepository {
                maven("https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/")
            }
            filter { includeModule("org.rustls", "rustls-platform-verifier") }
        }
    }
}

rootProject.name = "Motile"
include(":app", ":gestures")
