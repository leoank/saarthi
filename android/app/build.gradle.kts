plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "me.leoank.saarthi"
    compileSdk = 35

    defaultConfig {
        applicationId = "me.leoank.saarthi"
        minSdk = 26
        targetSdk = 35
        versionCode = 27
        versionName = "0.21.0"
        ndk { abiFilters += "arm64-v8a" }
    }
    buildFeatures { compose = true }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
}

dependencies {
    // The shell and its shared pieces, and a module per workspace (ADR-0077).
    implementation(project(":ui"))
    implementation(project(":agents"))
    implementation(project(":yajna"))
    implementation(project(":machines"))
    // Notifications through the phone's UnifiedPush distributor (ADR-0026).
    implementation("org.unifiedpush.android:connector:3.0.10")
}
