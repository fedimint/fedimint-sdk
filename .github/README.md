<p align="center">
  <img src="../docs/public/icon.png" alt="Fedimint Logo" width="300" />
  <!-- Removes the border below the header tag -->
  <div id="toc"><ul align="center" style="list-style: none;"><summary>
    <h1><b>Fedimint Sdk</b></h1>
    <p>A Robust, privacy-focused, and WebAssembly-powered fedimint client for the browser.</p>
  </summary></ul></div>

  <p align="center">
    <a href="https://github.com/fedimint/fedimint-sdk/blob/main/LICENSE"><img src="https://img.shields.io/github/license/fedimint/fedimint-sdk?style=plastic&color=blue" alt="GitHub License" /></a>
    <a href="https://github.com/fedimint/fedimint-sdk/actions"><img src="https://img.shields.io/github/actions/workflow/status/fedimint/fedimint-sdk/.github%2Fworkflows%2Fchangesets.yml?style=plastic&label=CI&color=green" alt="Build Status" /></a>
    <a href="https://sdk.fedimint.org"><img src="https://img.shields.io/github/actions/workflow/status/fedimint/fedimint-sdk/deploy-docs.yml?style=plastic&label=Docs%20Site&color=%2303b1fc" alt="Docs Workflow" /></a>
    <a href="https://deepwiki.com/fedimint/fedimint-sdk"><img src="https://deepwiki.com/badge.svg" alt="Ask DeepWiki" /></a>
  </p>
  
  <!-- Removes the border below the header tag -->
  <div id="toc"><ul align="center" style="list-style: none;"><summary>
    <h2>
        Docs Site: <a href="https://sdk.fedimint.org">sdk.fedimint.org</a>
    </h2>
  </summary></ul></div>

## Packages 📦

### Public packages

| Package                                                                            | Version                                                                                                                                                                         | Description                                                                                      |
| ---------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| [`@fedimint/core`](https://www.npmjs.com/package/@fedimint/core)                   | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Fcore?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Fcore>)                   | Core Fedimint client for JavaScript runtimes (Wasm bindings, high-level API, testing utilities). |
| [`@fedimint/react`](https://www.npmjs.com/package/@fedimint/react)                 | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Freact?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Freact>)                 | React hooks and context for working with Fedimint clients in React applications.                 |
| [`@fedimint/transport-web`](https://www.npmjs.com/package/@fedimint/transport-web) | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Ftransport-web?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Ftransport-web>) | Web worker transport that hosts the Wasm client and communicates with `@fedimint/core`.          |
| [`create-fedimint-app`](https://www.npmjs.com/package/create-fedimint-app)         | ![NPM Version (latest)](<https://img.shields.io/npm/v/create-fedimint-app?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=create-fedimint-app>)                 | CLI tool for scaffolding a Fedimint starter app with Vite, React, and TypeScript.                |

### Internal & legacy

| Package                                                                               | Version                                                                                                                                                                                                       | Description                                                                                                                 |
| ------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| [`@fedimint/core-web`](https://www.npmjs.com/package/@fedimint/core-web) (deprecated) | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Fcore-web?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Fcore-web>)                                         | Legacy shim that re-exports `@fedimint/core`; new projects should depend on `@fedimint/core` directly.                      |
| [`@fedimint/fedimint-client-wasm-web`](../web/wasm-web/README.md)                     | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Ffedimint-client-wasm-web?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Ffedimint-client-wasm-web>)         | Not intended for direct use. Wasm-pack build targeting web environments; consumed by `@fedimint/transport-web`.             |
| [`@fedimint/fedimint-client-wasm-bundler`](../web/wasm-bundler/README.md)             | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Ffedimint-client-wasm-bundler?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Ffedimint-client-wasm-bundler>) | Not intended for direct use. Wasm-pack build targeting bundler environments; used when bundling custom transports or hosts. |
| [`@fedimint/types`](https://www.npmjs.com/package/@fedimint/types)                    | ![NPM Version (latest)](<https://img.shields.io/npm/v/%40fedimint%2Ftypes?style=plastic&logo=npm&logoColor=rgb(187%2C%2054%2C%2057)&label=%40fedimint%2Ftypes>)                                               | Shared TypeScript interfaces for transports and other Fedimint client implementations.                                      |
| [`@fedimint/integration-tests`](../web/integration-tests/README.md) (private)         | —                                                                                                                                                                                                             | Internal Vitest harness for exercising the SDK against embedded nodes.                                                      |

## Structure 🛠️

This monorepo is structured as a pnpm workspace. There are some helpful scripts in the root `package.json` to help manage the workspace.

```bash
fedimint-sdk
├── docs
├── examples
│   ├── vite-core
│   ├── bare-js
│   ├── next-js
│   └── webpack-app
└── packages
    ├── core
    ├── core-web
    ├── create-fedimint-app
    ├── integration-tests
    ├── react
    ├── transport-web
    ├── types
    ├── wasm-bundler
    └── wasm-web
├── scripts
```

### Examples

- [`vite-core`](../examples/vite-core/README.md): React + Vite starter focused on `@fedimint/core` primitives.
- [`next-js`](../examples/next-js/README.md): Example configuration for a Next.js application.
- [`webpack-app`](../examples/webpack-app/README.md): Demonstrates configuring webpack for Fedimint applications.
- [`bare-js`](../examples/bare-js/README.md): Minimal usage of `@fedimint/core` without a bundler.

### Developer Calls

- [SDK Developer Calls](https://meet.jit.si/fedimintdevcall): We have developer calls every Wednesday at 4PM UTC to
  review PRs and discuss current development priorities. As a new developer, this is a great place to find good first
  issues and mentorship from the core team on how to get started contributing.

### Credit

Used the [wagmi](https://github.com/wevm/wagmi) library as a reference for the repo's structure.

> [!NOTE]
> In October 2025, this repository was renamed from "Fedimint Web Sdk" to "Fedimint Sdk" as the react-native packages were added.
> See the [discussion topic](https://github.com/fedimint/fedimint-sdk/discussions/190) for details.
