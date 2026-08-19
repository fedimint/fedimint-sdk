# Contributing

Thanks for your interest in contributing to the Fedimint Sdk! Please take a moment to review this document **before submitting a pull request.**

## Overview

This guide is intended to help you get started with contributing. By following these steps, you will understand the development process and workflow.

:::warning
**Please ask first before starting work on any significant new features. This includes things like adding new services, features, or changing the behavior of existing features.**

<!-- It's never a fun experience to have your pull request declined after investing time and effort into a new feature. To avoid this from happening, we request that contributors first create a [feature request](https://github.com/wevm/wagmi/discussions/new?category=ideas) to discuss any API changes or significant new ideas. -->

:::

## Development Workstreams

This repository houses two distinctly architected SDKs designed for different environments. Because their underlying native components and build toolchains differ, their developer instructions and testing suites are separated:

1. **Web SDK**: Targets browsers and Node.js. Depends on a WASM compilation (`fedimint-client-wasm`) from the core Fedimint repo.
2. **React Native SDK**: Targets mobile iOS & Android apps. Depends on the UniFFI bridge (`fedimint-client-uniffi`) located in the [fedimint-sdk-ffi repository](https://github.com/fedimint/fedimint-sdk-ffi).

---

## 1. Set up Nix

Fedimint uses Nix for managing the development environment. It is **highly recommended** to use Nix to ensure you have the correct tools and versions.

For detailed instructions on setting up Nix, see [Nix Setup](./nix_setup.md).

Once Nix is installed, you can enter the development shell for standard web/JS operations:

```bash
nix develop
```

## 2. Cloning the repository

To start contributing to the project, clone it to your local machine using git:

```bash
git clone https://github.com/fedimint/fedimint-sdk.git
```

Or the [GitHub CLI](https://cli.github.com):

```bash
gh repo clone fedimint/fedimint-sdk
```

## 3. Installing dependencies

Once in the project's root directory, run the following command to install pnpm and the project's dependencies (assuming you are inside the `nix develop` shell which provides them):

```bash
pnpm install
```

After the install completes, pnpm links packages across the project for development and [git hooks](https://github.com/toplenboren/simple-git-hooks) are set up.

## 4. Development Guides

The setup process, commands, and architecture drastically differ depending on which part of the SDK you are contributing to. **Before building**, select the relevant platform below:

1. **[Web SDK Development](./web.md)** - Guide for working on strictly browser, Node.js packages, and `fedimint-client-wasm` integrations.
2. **[React Native SDK Development](./react-native.md)** - Guide for working on native iOS/Android wrappers and `fedimint-client-uniffi` integrations using `just`.

## 5. Next Steps

Once you have your environment set up and your dependencies installed, please review the following workflows before submitting changes:

- **[Writing Documentation](./documentation.md)**: How to run the local VitePress documentation server.
- **[Submitting a Pull Request](./pull-requests.md)**: Naming conventions and PR submission guidelines.
- **[Versioning](./versioning.md)**: How to use Changesets to manage package version bumps and release notes.
- **[Updating Dependencies](./dependencies.md)**: Keeping packages up-to-date using `taze`.
