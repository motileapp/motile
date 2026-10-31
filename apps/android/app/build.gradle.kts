import java.util.Properties

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

val root = rootDir.resolve("../..")

/** The first two numbers of the workspace's version, as Cargo.toml sets them, and the release's count. */
val cargoVersion: String = root.resolve("Cargo.toml").readLines()
    .first { it.startsWith("version = ") }.substringAfter('"').substringBefore('"')

/** The Kotlin half of rustls-platform-verifier must be the version of its Rust half in Cargo.lock. */
val verifierVersion: String = root.resolve("Cargo.lock").readLines().let { lines ->
    val at = lines.indexOfFirst { it.trim() == "name = \"rustls-platform-verifier-android\"" }
    lines.drop(at + 1).first { it.startsWith("version = ") }.substringAfter('"').substringBefore('"')
}

/** Signing for a release, from the variables scripts/release.sh is given. */
val release = Properties().apply {
    System.getenv("ANDROID_KEYSTORE")?.takeIf { it.isNotEmpty() }?.let { setProperty("store", it) }
    System.getenv("ANDROID_KEYSTORE_PASSWORD")?.let { setProperty("password", it) }
    System.getenv("ANDROID_KEY_ALIAS")?.let { setProperty("alias", it) }
}

android {
    namespace = "app.motile"
    compileSdk = 37
    ndkVersion = "28.2.13676358"

    defaultConfig {
        applicationId = "app.motile.motile"
        minSdk = 29
        targetSdk = 36
        versionName = cargoVersion
        versionCode = (System.getenv("MOTILE_VERSION_CODE") ?: "1").toInt()
        // The auth server the client signs in with; the dev app is given another when it starts.
        buildConfigField("String", "AUTH_URL", "\"https://auth.motile.app\"")
        // The ABIs the core is built for; MapLibre brings its own library for others too.
        ndk { abiFilters += listOf("arm64-v8a", "x86_64") }
    }

    signingConfigs {
        if (release.getProperty("store") != null) {
            create("release") {
                storeFile = file(release.getProperty("store"))
                storePassword = release.getProperty("password")
                keyAlias = release.getProperty("alias")
                keyPassword = release.getProperty("password")
            }
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = signingConfigs.findByName("release") ?: signingConfigs.getByName("debug")
        }
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    sourceSets["main"].jniLibs.directories.add("build/jniLibs")

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
        unitTests.isIncludeAndroidResources = true
        // The canvas and program tests write their pictures with Roborazzi, to build/ at the repository's
        // root, and lay programs out with the core built for this machine. Both are the task's, so that
        // a new core runs them again and a cached run still leaves their frames.
        unitTests.all {
            val repository = rootDir.resolve("../..")
            val hostLibs = layout.buildDirectory.dir("hostLibs")
            it.inputs.files(hostLibs).withPathSensitivity(PathSensitivity.RELATIVE)
            it.inputs.files(repository.resolve("crates/core/fixtures/openui")).withPathSensitivity(PathSensitivity.RELATIVE)
            it.inputs.files(repository.resolve("crates/core/fixtures/canvas/reference")).withPathSensitivity(PathSensitivity.RELATIVE)
            it.outputs.dir(repository.resolve("build/frames/android"))
            it.outputs.dir(repository.resolve("build/programs/android"))
            it.outputs.dir(repository.resolve("build/canvas/android"))
            it.systemProperty("roborazzi.test.record", "true")
            it.systemProperty("java.library.path", hostLibs.get().asFile.path)
            // Robolectric reaches into FileDescriptor for Android 16's shared memory.
            it.jvmArgs("--add-exports=java.base/jdk.internal.access=ALL-UNNAMED")
        }
    }
}

kotlin {
    jvmToolchain(21)
}

/** Builds the Rust core into build/jniLibs. `-Pmotile.skipCore` leaves the one built last. */
val skipCore = providers.gradleProperty("motile.skipCore").isPresent
val buildCore = tasks.register<Exec>("buildCore") {
    workingDir = rootDir
    commandLine("scripts/build-core.sh")
    enabled = !skipCore
}
tasks.named("preBuild") { dependsOn(buildCore) }

/** The map's styles, which packages/theme generates, as the app's assets. */
abstract class MapStyles : DefaultTask() {
    @get:InputFiles
    abstract val styles: ConfigurableFileCollection

    @get:OutputDirectory
    abstract val output: DirectoryProperty

    @TaskAction
    fun copy() {
        val folder = output.get().asFile
        folder.deleteRecursively()
        folder.mkdirs()
        styles.forEach { it.copyTo(folder.resolve(it.name)) }
    }
}

val mapStyles = tasks.register<MapStyles>("mapStyles") {
    styles.from(root.resolve("packages/theme/map-style-light.json"), root.resolve("packages/theme/map-style-dark.json"))
}

androidComponents {
    onVariants { variant -> variant.sources.assets?.addGeneratedSourceDirectory(mapStyles, MapStyles::output) }
}

dependencies {
    implementation(platform(libs.compose.bom))
    implementation(libs.compose.ui)
    implementation(libs.compose.foundation)
    implementation(libs.compose.material3)
    implementation(libs.androidx.core)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.browser)
    implementation(libs.androidx.webkit)
    implementation(libs.androidx.lifecycle.process)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.coil.compose)
    implementation(libs.coil.video)
    implementation(libs.coil.svg)
    implementation(libs.coroutines)
    implementation(libs.serialization.json)
    implementation(libs.media3.exoplayer)
    implementation(libs.media3.ui)
    implementation(libs.maplibre)
    implementation(libs.okhttp)
    implementation("org.rustls:rustls-platform-verifier:$verifierVersion")
    debugImplementation(libs.compose.ui.tooling)
    debugImplementation(libs.compose.ui.test.manifest)

    testImplementation(libs.junit)
    testImplementation(libs.robolectric)
    testImplementation(libs.roborazzi)
    testImplementation(libs.compose.ui.test)
}
