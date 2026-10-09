pluginManagement {
    repositories {
        google {
            content {
                includeGroupByRegex("com\\.android.*")
                includeGroupByRegex("com\\.google.*")
                includeGroupByRegex("androidx.*")
            }
        }
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

rootProject.name = "fedimint-android"

// The published library — this is what becomes the AAR.
include(":fedimint-sdk")

// The reference wallet (android/app/DECISION.md): a real app on the SDK, run
// on a device or emulator. Not published. Compiling it in CI proves the
// generated bindings are usable from ordinary Kotlin.
include(":app")
