import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.android)
}

android {
    namespace = "org.fedimint.demo"
    compileSdk = 36
    // See the note in fedimint-sdk/build.gradle.kts: the nix shell ships one
    // build-tools, and AGP's default is not it.
    buildToolsVersion = "36.0.0"

    defaultConfig {
        applicationId = "org.fedimint.demo"
        minSdk = 28
        targetSdk = 36
        versionCode = 1
        versionName = "0.1.0-alpha.1"
    }

    buildTypes {
        // Debug only: this app exists to be run, not shipped, and an unsigned
        // release build would need a keystore to install.
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        viewBinding = false
    }
}

// The Kotlin side of `compileOptions` above, and it must agree with it. Set through the Kotlin
// plugin's own `compilerOptions` rather than AGP's `android.kotlinOptions`, which Kotlin 2.x
// deprecates.
kotlin {
    compilerOptions {
        jvmTarget = JvmTarget.JVM_17
    }
}

dependencies {
    // The SDK under test, as a project dependency rather than the published
    // AAR, so a change to the Rust is one `build-android-sdk.sh` away.
    implementation(project(":fedimint-sdk"))

    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.appcompat)
    implementation(libs.material)
    implementation(libs.androidx.lifecycle.runtime.ktx)
    implementation(libs.kotlinx.coroutines.android)
}
