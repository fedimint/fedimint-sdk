plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
}

// Publishing (maven-publish / signing / the Maven Central publication) is not
// wired up yet — this module only builds the AAR and is verified by
// .github/workflows/android-sdk.yaml. `libs.versions.fedimintSdk` still names
// the version for whenever it is.

android {
    namespace = "org.fedimint.sdk"
    compileSdk = 36

    // Pinned rather than left to AGP's default (35.0.0). The `.#android` nix
    // shell supplies exactly one build-tools, 36.0.0, in a read-only nix
    // store, so a default AGP cannot find would send it off to download one
    // and fail on an unwritable SDK directory. Keep this in step with
    // `buildToolsVersions` in flake.nix.
    buildToolsVersion = "36.0.0"

    defaultConfig {
        // Keep in sync with MIN_SDK in scripts/generate-android-so.sh, which
        // passes it to cargo-ndk as the native library's target platform.
        minSdk = 28

        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        consumerProguardFiles("consumer-rules.pro")

        // The ABIs the Nix build ships. Anything else would package an empty
        // ABI directory and fail at load time on that device.
        ndk {
            abiFilters += listOf("arm64-v8a", "x86_64")
        }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = "11"
    }

    // src/main/jniLibs is AGP's default JNI location, so the .so files the Nix
    // build (or scripts/nix-build-kotlin.sh) places there are packaged with no
    // extra configuration. They are build outputs and are gitignored.
}

dependencies {
    // JNA loads libfedimint_sdk.so and marshals calls across the
    // boundary. It must be the `aar` artifact: the plain jar has no Android
    // native libraries and fails at runtime.
    implementation(libs.jna) {
        artifact { type = "aar" }
    }

    // The generated bindings expose the crate's async methods as `suspend fun`
    // and import kotlinx.coroutines directly (GlobalScope, Job, launch,
    // suspendCancellableCoroutine), so this is a compile requirement of the
    // generated code, not a convenience. `-android` is not needed here: only
    // the demo touches Dispatchers.Main.
    implementation(libs.kotlinx.coroutines.core)

    implementation(libs.androidx.core.ktx)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
}
