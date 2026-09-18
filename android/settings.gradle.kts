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

// A demo app that exercises the SDK on a device or emulator. Not published;
// it exists so the native library can be run rather than only linked.
include(":app")
