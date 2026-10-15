// The tests that use the client with fingers. They run in a process of their own, as the iOS app's
// UI tests do, so that they can stop the client and open it again.
plugins {
    alias(libs.plugins.android.test)
}

android {
    namespace = "app.motile.gestures"
    compileSdk = 37
    targetProjectPath = ":app"

    defaultConfig {
        minSdk = 29
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    experimentalProperties["android.experimental.self-instrumenting"] = true
}

kotlin {
    jvmToolchain(21)
}

dependencies {
    implementation(libs.androidx.test.runner)
    implementation(libs.androidx.test.junit)
    implementation(libs.uiautomator)
}
