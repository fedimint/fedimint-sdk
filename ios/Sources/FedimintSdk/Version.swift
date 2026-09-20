// The only hand-written Swift in this SDK. Two declarations, both here for a
// build reason rather than to add API.
//
// Everything else in this directory is generated: `FedimintSdk.swift` is
// written by uniffi-bindgen from the UniFFI metadata baked into the native
// library, and is gitignored. The rule this SDK follows is the one
// android/README.md states for Kotlin — the generated bindings *are* the API,
// because the crate's `#[uniffi::export]`s hand out fedimint-sdk's real types
// rather than binding-only copies that could drift. Anything ergonomic added
// here would be a Swift-only surface with no Kotlin counterpart; see
// rust/uniffi-bindgen/DECISION.md, which is where to revisit that.

/// The error every fallible call in this SDK throws.
///
/// The Rust type is `crate::Error`, and UniFFI's Swift backend does not rename
/// it, so the generated class is literally `Error` — which is ambiguous with
/// `Swift.Error` in any file that writes `catch let e as Error` or names
/// `Result<_, Error>`. This alias is what callers should use, and it is the
/// direct counterpart of the `import org.fedimint.sdk.Exception as SdkException`
/// that android/README.md tells Kotlin callers to write, for the same reason.
///
/// A type alias rather than a `rename` in `uniffi.toml`: that config key exists
/// and is broken for constructors on uniffi 0.32.0 — see the note in
/// `rust/fedimint-sdk/uniffi.toml`.
public typealias SdkError = FedimintSdk.Error

/// The version of the `fedimint-sdk` crate these bindings were generated from.
///
/// Also the reason this file has to exist at all: SwiftPM refuses to resolve a
/// target whose source directory is empty, and `FedimintSdk.swift` is
/// gitignored, so a fresh clone — before anyone has run `just build-swift` —
/// would fail on `swift build` with a confusing "Source files for target
/// FedimintSdk should be located under ..." rather than on the missing
/// XCFramework.
///
/// Keep in step with `version` in `rust/fedimint-sdk/Cargo.toml` — the same
/// role `fedimintSdk` plays in `android/gradle/libs.versions.toml`.
public let fedimintSdkVersion = "0.1.0-alpha.1"
