# Fedimint SDK FFI

UniFFI bindings for [fedimint-sdk](../fedimint-sdk/): the language-binding layer
for Swift, Kotlin, and other targets.

This crate wraps the SDK's public API into types that `uniffi` can generate
bindings from. Every type here is a thin wrapper; the logic lives in
`fedimint-sdk` and this crate adds only the FFI annotations and the `Arc<Self>`
ownership that UniFFI requires.

## Status

Prototype. Currently wraps `Mnemonic` as the T4 FFI shape spike proof-of-concept.
Full bindings will be added as the SDK facades are implemented.

## Why a separate crate?

See the [T4 spike discussion](#380) for the full analysis. In short:
`fedimint-sdk` enforces `#![deny(missing_docs)]` and a `wasm32-unknown-unknown`
check gate. UniFFI's macro expansions generate items that conflict with both, and
adding UniFFI as even an optional dependency would pull its proc-macro tree into
the SDK's `Cargo.lock`. Keeping the boundary here means the SDK crate stays lean
and portable, and this crate can set its own lint policy.
