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
    }
}
rootProject.name = "saarthi"
// The app is its shell and shared pieces, each workspace a module of its own (ADR-0077).
include(":app", ":core", ":ui", ":agents", ":yajna", ":machines")
