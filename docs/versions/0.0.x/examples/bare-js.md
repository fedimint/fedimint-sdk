---
aside: false
---

# Bare JS Example

This example shows how to use the `@fedimint/core` package in a bare javascript environment (no bundler).

There's no ui, so open your browser's console to see the library in action.

[Code on Github](https://github.com/fedimint/fedimint-sdk/tree/main/examples/bare-js)

## Live Preview

::: tip
The Live Previews don't always work. If you're having trouble, try running the example locally OR using the [StackBlitz link](https://stackblitz.com/github/fedimint/fedimint-sdk/tree/main/examples/bare-js).

Also, you might need to disable your adblocker or Brave Shields to see the preview.
:::

<br>

<iframe src="https://stackblitz.com/github/fedimint/fedimint-sdk/tree/main/examples/bare-js?embed=1&theme=dark&file=index.html" style=" width: 100%; height: 600px; border: 0;"></iframe>

[![Open in StackBlitz](https://developer.stackblitz.com/img/open_in_stackblitz.svg)](https://stackblitz.com/github/fedimint/fedimint-sdk/tree/main/examples/bare-js)

## Running the Example Locally

Clone the repo

```sh
git clone https://github.com/fedimint/fedimint-sdk.git
cd fedimint-sdk
```

Install the package manager with [Corepack](https://nodejs.org/api/corepack.html) & install dependencies.

```sh
corepack enable
pnpm i
```

Run the Example

```sh
pnpm run dev:bare
```
