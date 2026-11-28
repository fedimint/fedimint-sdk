<p align="center">
  <img src="icon.png" alt="Fedimint Logo" width="300" />
  <!-- Removes the border below the header tag -->
  <div id="toc"><ul align="center" style="list-style: none;"><summary>
    <h1><b>Fedimint Sdk</b></h1>
    <p>One Rust SDK for wallets and apps on Fedimint, and the Kotlin, Swift, React Native and browser SDKs generated from it.</p>
  </summary></ul></div>

  <p align="center">
    <a href="https://github.com/fedimint/fedimint-sdk/blob/main/LICENSE"><img src="https://img.shields.io/github/license/fedimint/fedimint-sdk?style=plastic&color=blue" alt="GitHub License" /></a>
    <a href="https://github.com/fedimint/fedimint-sdk/actions/workflows/pull-request.yml"><img src="https://img.shields.io/github/actions/workflow/status/fedimint/fedimint-sdk/pull-request.yml?branch=main&style=plastic&label=CI&color=green" alt="Build Status" /></a>
    <a href="https://deepwiki.com/fedimint/fedimint-sdk"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki" /></a>
  </p>

## Components 📦

[`fedimint-sdk`](../rust/fedimint-sdk) is a Rust crate: one API for wallets and apps to join
federations, hold ecash, and send and receive over Lightning and on-chain. The SDKs for the other
languages are generated from it with [UniFFI](https://mozilla.github.io/uniffi-rs/), so each of
them is a view of that API and not a copy of it.

| Component            | Source                                      | Published as                                                                                                                                                                                   |
| -------------------- | ------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rust SDK             | [`rust/fedimint-sdk`](../rust/fedimint-sdk) | [`fedimint-sdk`](https://crates.io/crates/fedimint-sdk) on crates.io                                                                                                                           |
| Android SDK (Kotlin) | [`android`](../android)                     | [`org.fedimint:sdk`](https://central.sonatype.com/artifact/org.fedimint/sdk) on Maven Central                                                                                                  |
| iOS SDK (Swift)      | [`ios`](../ios)                             | Not published yet                                                                                                                                                                              |
| React Native         | [`js/react-native`](../js/react-native)     | [`@fedimint/react-native`](https://www.npmjs.com/package/@fedimint/react-native) and [`@fedimint/react-native-bindings`](https://www.npmjs.com/package/@fedimint/react-native-bindings) on npm |
| Browser SDK          | [`js/web/web-sdk`](../js/web/web-sdk)       | Not published yet                                                                                                                                                                              |

The README in each of these directories has the details for its component.

## Structure 🛠️

The Rust crates are in `rust/`, the Android and iOS SDKs in `android/` and `ios/`, and the
JavaScript packages in a pnpm workspace rooted at `js/`.

```bash
fedimint-sdk
├── rust
│   ├── fedimint-sdk
│   ├── ubrn
│   └── uniffi-bindgen
├── android
├── ios
├── js
│   ├── android
│   │   └── integration-tests
│   ├── examples
│   │   ├── expo-app
│   │   ├── react-native
│   │   └── vite-app
│   ├── react-native
│   │   ├── react-native
│   │   └── react-native-bindings
│   └── web
│       └── web-sdk
├── nix
└── scripts
```

### Examples

- [`rust/fedimint-sdk/examples`](../rust/fedimint-sdk/examples): command-line programs that send
  and receive ecash, Lightning and on-chain payments.
- [`android/app`](../android/app): an Android app that calls the SDK through the Kotlin bindings.
- [`ios/Demo`](../ios/Demo): the same app for iOS, in SwiftUI.
- [`react-native`](../js/examples/react-native/README.md) and
  [`expo-app`](../js/examples/expo-app/README.md): React Native apps, without and with Expo.
- [`vite-app`](../js/examples/vite-app/README.md): a React application built with Vite, on the
  browser SDK.

### Contributing

See the [contributing guide](CONTRIBUTING.md).

### Developer Calls

- [SDK Developer Calls](https://meet.jit.si/fedimintdevcall): We have developer calls every Wednesday at 4PM UTC to
  review PRs and discuss current development priorities. As a new developer, this is a great place to find good first
  issues and mentorship from the core team on how to get started contributing.

### Credit

Used the [wagmi](https://github.com/wevm/wagmi) library as a reference for the repo's structure.

> [!NOTE]
> In October 2025, this repository was renamed from "Fedimint Web Sdk" to "Fedimint Sdk" as the react-native packages were added.
> See the [discussion topic](https://github.com/fedimint/fedimint-sdk/discussions/190) for details.

> [!NOTE]
> The TypeScript SDK that this repository published to npm is no longer developed, and no further
> versions of it are released: `@fedimint/core`, `@fedimint/core-web`, `@fedimint/react`,
> `@fedimint/transport-web`, `@fedimint/types`, `@fedimint/fedimint-client-wasm-bundler`,
> `@fedimint/fedimint-client-wasm-web` and `create-fedimint-app`. Its source is in this
> repository's history, for example at commit
> [`e4c7c59`](https://github.com/fedimint/fedimint-sdk/tree/e4c7c59b526aa2cb6e85854efeeb44f6ceddf6b7/js).
