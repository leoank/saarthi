plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android { namespace = "me.leoank.saarthi.yajna" }

dependencies {
    implementation(project(":ui"))
}
