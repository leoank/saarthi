// EXP-20261114-raqjf0 steps 2-4: a throwaway app drawing a terminal with libghostty-vt,
// beside Termux's terminal library for comparison. Built in the project's android shell.
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
        // Termux's terminal-emulator and terminal-view libraries.
        maven("https://jitpack.io")
    }
}
rootProject.name = "vtspike"
include(":app")
