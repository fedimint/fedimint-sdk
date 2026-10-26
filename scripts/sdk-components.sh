# shellcheck shell=bash
#
# The components an SDK release is made of, for the release scripts to source:
# scripts/sdk-version.sh and scripts/sdk-changelog.sh.
#
# A component is one thing a release publishes. Each has its own version in the tree, its own
# release notes and its own release tags, and is published by its own workflow:
#
#   rust          the `fedimint-sdk` crate, to crates.io
#   android       the Android SDK, `org.fedimint:sdk`, to Maven Central
#   react-native  `@fedimint/react-native` and `@fedimint/react-native-bindings`, to npm
#
# They share one line of versions: a release takes the next version of the line and gives it
# to the components it publishes, which may be all of them or only some.
#
# Sourcing this file defines names only. It runs nothing and reads no file.

COMPONENTS=(rust android react-native)
# The components generated from the crate's UniFFI surface.
# shellcheck disable=SC2034 # Read by the scripts that source this file.
BINDINGS=(android react-native)

# Says on stderr that $1 is not a component, and fails.
not_a_component() {
    local names="" name
    for name in "${COMPONENTS[@]}"; do
        names="${names:+$names, }$name"
    done
    echo "${1:-} is not a component: it must be one of $names" >&2
    return 1
}

# Succeeds if $1 is a component. Otherwise says so on stderr and fails, as does every other
# function here for a name that is not a component.
is_component() {
    local name
    for name in "${COMPONENTS[@]}"; do
        if [[ "${1:-}" == "$name" ]]; then
            return 0
        fi
    done
    not_a_component "${1:-}"
}

# What a release of the component $1 is called, as in "Android SDK 0.2.0".
component_title() {
    case "${1:-}" in
        rust) echo "Rust SDK" ;;
        android) echo "Android SDK" ;;
        react-native) echo "React Native SDK" ;;
        *) not_a_component "${1:-}" ;;
    esac
}

# The prefix of the release tags of the component $1: a release of version X is the tag
# `<prefix>X`.
component_tag_prefix() {
    case "${1:-}" in
        rust) echo "rust-sdk-v" ;;
        android) echo "android-sdk-v" ;;
        react-native) echo "react-native-sdk-v" ;;
        *) not_a_component "${1:-}" ;;
    esac
}

# The release notes of the component $1, relative to the repository.
component_changelog() {
    case "${1:-}" in
        rust) echo "rust/fedimint-sdk/CHANGELOG.md" ;;
        android) echo "android/CHANGELOG.md" ;;
        react-native) echo "js/react-native/CHANGELOG.md" ;;
        *) not_a_component "${1:-}" ;;
    esac
}

# The file name of the workflow that releases the component $1.
component_workflow() {
    case "${1:-}" in
        rust) echo "rust-sdk-release.yaml" ;;
        android) echo "android-sdk-release.yaml" ;;
        react-native) echo "react-native-sdk-release.yaml" ;;
        *) not_a_component "${1:-}" ;;
    esac
}

# What the component $1 is built from, one path per line, relative to the repository. A commit
# that touches none of them cannot change the component, so it does not belong in its notes. The
# directories include their Cargo lockfiles (rust/fedimint-sdk/Cargo.lock,
# rust/uniffi-bindgen/Cargo.lock, rust/ubrn/Cargo.lock), so a commit that only changes a dependency
# touches the component too; scripts/test-sdk-release-scripts.sh checks that.
#
# The Android SDK names nix/ffi.nix, which builds its native library, rather than the nix
# directory. The other file there, nix/web-bindgen.nix, provides the binding generators of the
# React Native packages (ubrn) and of the web SDK (ubrn and wasm-bindgen-cli), which the Android
# SDK does not use, so only the React Native packages list the whole directory.
component_paths() {
    case "${1:-}" in
        rust)
            printf '%s\n' \
                rust/fedimint-sdk
            ;;
        android)
            printf '%s\n' \
                android/fedimint-sdk \
                android/gradle \
                android/build.gradle.kts \
                android/settings.gradle.kts \
                android/gradle.properties \
                rust/fedimint-sdk \
                rust/uniffi-bindgen \
                nix/ffi.nix \
                flake.nix \
                flake.lock \
                scripts/generate-android-bindings.sh \
                scripts/nix-build-android-so.sh
            ;;
        react-native)
            printf '%s\n' \
                js/react-native \
                rust/fedimint-sdk \
                rust/ubrn \
                nix \
                flake.nix \
                flake.lock \
                scripts/generate-sdk-rn-bindings.sh \
                scripts/assemble-rn-ios-xcframework.sh \
                scripts/react-native-sdk-pack.sh
            ;;
        *) not_a_component "${1:-}" ;;
    esac
}

# The scopes that mark a commit as not being about the component $1, one per line. The scope of a
# commit is the text in the parentheses of its conventional commit subject, as in
# `fix(flake): ...`. A commit with one of these scopes is left out of the component's notes, even
# though it touched something the component is built from (see component_paths). A scope is foreign
# to a component when it names
#
#   - the development environment or the repository's tooling: flake, ci, workflows, tests,
#     examples and release;
#   - or a part of the repository that the component does not ship: the web SDK (web, sdk-web and
#     vite-core), the React Native packages (react-native and rn-bindings), js (the JavaScript
#     workspace), and swift and ios (the Swift SDK and iOS).
#
# The React Native scopes and js are foreign to the crate and the Android SDK, but not to the
# React Native packages, which ship them. `ios` is foreign to the Android SDK only: the React
# Native packages ship for iOS, and the crate runs on it. `android` is foreign to none of them,
# because the crate's Android code is in rust/fedimint-sdk, which all three ship.
component_foreign_scopes() {
    case "${1:-}" in
        rust)
            printf '%s\n' \
                flake \
                ci \
                workflows \
                tests \
                examples \
                release \
                web \
                sdk-web \
                vite-core \
                react-native \
                rn-bindings \
                js \
                swift
            ;;
        android)
            printf '%s\n' \
                flake \
                ci \
                workflows \
                tests \
                examples \
                release \
                web \
                sdk-web \
                vite-core \
                react-native \
                rn-bindings \
                js \
                swift \
                ios
            ;;
        react-native)
            printf '%s\n' \
                flake \
                ci \
                workflows \
                tests \
                examples \
                release \
                web \
                sdk-web \
                vite-core \
                swift
            ;;
        *) not_a_component "${1:-}" ;;
    esac
}
