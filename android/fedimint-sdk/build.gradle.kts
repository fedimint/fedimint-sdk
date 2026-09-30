import com.vanniktech.maven.publish.AndroidSingleVariantLibrary
import org.jetbrains.kotlin.gradle.dsl.JvmTarget

plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.maven.publish)
}

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
        // Keep in sync with MIN_SDK in scripts/build-android-sdk.sh, which
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
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    // src/main/jniLibs is AGP's default JNI location, so the .so files the Nix
    // build (or scripts/build-android-sdk.sh) places there are packaged with no
    // extra configuration. They are build outputs and are gitignored.
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
    // the wallet app (android/app) touches Dispatchers.Main.
    implementation(libs.kotlinx.coroutines.core)

    implementation(libs.androidx.core.ktx)

    testImplementation(libs.junit)
    androidTestImplementation(libs.androidx.junit)
    androidTestImplementation(libs.androidx.espresso.core)
}

// Maven Central, through the Sonatype Central Portal. The coordinates are
// org.fedimint:sdk, so an app depends on `org.fedimint:sdk:<version>` and
// imports `org.fedimint.sdk.*`: the artifact name matches the Kotlin package
// (uniffi.toml's package_name), with nothing added. The Gradle module is
// still :fedimint-sdk; only the published name differs.
//
// The org.fedimint namespace has to be verified on central.sonatype.com
// before the first upload. The version is `libs.versions.fedimintSdk`, the
// Android SDK's own, which does not follow rust/fedimint-sdk's.
//
// `-Psnapshot=<name>` publishes `<name>-SNAPSHOT` instead, to Central's
// snapshots repository. .github/workflows/android-sdk-snapshot.yaml passes
// the branch and the commit, so a snapshot names exactly what it was built
// from, e.g. `main-85bd33d6df4b-SNAPSHOT`. It is not tied to the release
// version, so it is never bumped along with the releases.
// Snapshots have to be enabled for the namespace on central.sonatype.com.
//
// Credentials and the signing key are never in the repository. Gradle reads
// them from project properties, which CI supplies as environment variables
// (see .github/workflows/android-sdk-release.yaml):
//
//   ORG_GRADLE_PROJECT_mavenCentralUsername      Central Portal user token name
//   ORG_GRADLE_PROJECT_mavenCentralPassword      Central Portal user token secret
//   ORG_GRADLE_PROJECT_signingInMemoryKey        ASCII-armored GPG private key
//   ORG_GRADLE_PROJECT_signingInMemoryKeyPassword
//
// A release version is always signed, so `publishToMavenLocal` needs a
// signing key too, though any throwaway key will do. That is how to check the
// POM and the artifact set before a release. A snapshot is signed only if a
// key is given; Central does not require it.
val snapshotName = providers.gradleProperty("snapshot").orNull
require(snapshotName == null || Regex("[A-Za-z0-9._-]+").matches(snapshotName)) {
    "-Psnapshot=$snapshotName: use only letters, digits, '.', '_' and '-'"
}

mavenPublishing {
    // Only the release variant. The AAR carries jniLibs. The publishing
    // workflows assemble it from the same native libraries and generated
    // bindings that android-sdk.yaml tested, though not as the same archive
    // file. The sources jar is the generated bindings. AGP builds the javadoc
    // jar with its bundled Dokka, from those same bindings. Central requires
    // both jars.
    configure(
        AndroidSingleVariantLibrary(
            variant = "release",
            sourcesJar = true,
            publishJavadocJar = true,
        ),
    )

    // Uploads the deployment, but does not release it. It then waits in the
    // Central Portal until it is published there by hand, so a bad upload can
    // still be dropped. Release on Central cannot be undone. The build also
    // does not wait for Central to validate the deployment, so check its
    // status in the portal. A snapshot has no deployment: it goes straight to
    // the snapshots repository.
    publishToMavenCentral(automaticRelease = false)

    // Central rejects unsigned releases.
    signAllPublications()

    coordinates(
        groupId = "org.fedimint",
        artifactId = "sdk",
        version = snapshotName?.let { "$it-SNAPSHOT" } ?: libs.versions.fedimintSdk.get(),
    )

    pom {
        name.set("Fedimint SDK for Android")
        description.set(
            "Kotlin bindings for Android over fedimint-sdk, the high-level Fedimint client SDK, " +
                "generated with UniFFI.",
        )
        inceptionYear.set("2024")
        url.set("https://github.com/fedimint/fedimint-sdk")
        licenses {
            license {
                name.set("MIT License")
                url.set("https://opensource.org/licenses/MIT")
                distribution.set("repo")
            }
        }
        developers {
            developer {
                id.set("fedimint")
                name.set("The Fedimint Developers")
                url.set("https://github.com/fedimint")
            }
        }
        scm {
            url.set("https://github.com/fedimint/fedimint-sdk")
            connection.set("scm:git:https://github.com/fedimint/fedimint-sdk.git")
            developerConnection.set("scm:git:ssh://git@github.com/fedimint/fedimint-sdk.git")
        }
    }
}
