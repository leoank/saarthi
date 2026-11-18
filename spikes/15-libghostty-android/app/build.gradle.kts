plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "me.leoank.vtspike"
    compileSdk = 35
    defaultConfig {
        applicationId = "me.leoank.vtspike"
        minSdk = 26
        targetSdk = 35
        versionCode = 1
        versionName = "0.1"
        ndk { abiFilters += "arm64-v8a" }
    }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    // Measured as a user would run it.
    buildTypes { getByName("debug") { isDebuggable = false } }
}

kotlin { jvmToolchain(17) }

dependencies {
    val compose = platform("androidx.compose:compose-bom:2025.07.00")
    implementation(compose)
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("com.github.termux.termux-app:terminal-view:v0.118.0")
}
