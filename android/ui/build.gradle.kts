plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android { namespace = "me.leoank.saarthi.ui" }

// The fonts in res/font come from :core's buildCore task.
tasks.named("preBuild") { dependsOn(":core:buildCore") }

dependencies {
    api(project(":core"))
}
