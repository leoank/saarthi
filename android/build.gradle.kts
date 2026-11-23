plugins {
    id("com.android.application") version "8.12.0" apply false
    id("com.android.library") version "8.12.0" apply false
    id("org.jetbrains.kotlin.android") version "2.2.0" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.0" apply false
}

// What every module shares, set once here (ADR-0077): the SDK levels, Java 17, Compose and
// the libraries the screens are built from. Each module's own file names what is its alone.
subprojects {
    plugins.withId("com.android.library") {
        extensions.configure<com.android.build.gradle.LibraryExtension> {
            compileSdk = 35
            defaultConfig { minSdk = 26 }
            buildFeatures { compose = true }
            compileOptions {
                sourceCompatibility = JavaVersion.VERSION_17
                targetCompatibility = JavaVersion.VERSION_17
            }
        }
    }
    plugins.withId("org.jetbrains.kotlin.android") {
        extensions.configure<org.jetbrains.kotlin.gradle.dsl.KotlinAndroidProjectExtension> { jvmToolchain(17) }
        dependencies {
            val compose = platform("androidx.compose:compose-bom:2025.07.00")
            add("implementation", compose)
            add("implementation", "androidx.compose.material3:material3")
            add("implementation", "androidx.compose.material:material-icons-extended")
            add("implementation", "androidx.compose.ui:ui")
            add("implementation", "androidx.compose.ui:ui-tooling-preview")
            add("implementation", "androidx.activity:activity-compose:1.10.1")
            add("implementation", "androidx.lifecycle:lifecycle-viewmodel-compose:2.9.2")
            add("implementation", "androidx.lifecycle:lifecycle-runtime-compose:2.9.2")
            add("implementation", "org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
        }
    }
}
