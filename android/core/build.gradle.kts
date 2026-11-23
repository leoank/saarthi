plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android { namespace = "me.leoank.saarthi.core" }

// The client core: cross-built for arm64 Android into jniLibs, with its Kotlin bindings
// generated from the built library (ADR-0009, ADR-0024), and the fonts for :ui. Runs in
// `nix develop .#android`.
android {
    defaultConfig { ndk { abiFilters += "arm64-v8a" } }
    sourceSets["main"].java.srcDir(layout.buildDirectory.dir("generated/uniffi"))
}

val buildCore by tasks.registering(Exec::class) {
    workingDir = rootDir.parentFile
    commandLine("sh", "android/build-core.sh")
    inputs.dir(rootDir.parentFile.resolve("crates"))
    inputs.dir(rootDir.resolve("vt"))
    outputs.dir(layout.projectDirectory.dir("src/main/jniLibs"))
    outputs.dir(rootDir.resolve("ui/src/main/res/font"))
    outputs.dir(layout.buildDirectory.dir("generated/uniffi"))
}
tasks.named("preBuild") { dependsOn(buildCore) }

dependencies {
    // The UniFFI Kotlin bindings load the core through JNA.
    api("net.java.dev.jna:jna:5.17.0@aar")
}
