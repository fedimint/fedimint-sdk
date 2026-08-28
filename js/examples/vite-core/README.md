# Simple Vite Example

This is an example application demonstrating the usage of Fedimint client in a simple web application. [(demo)](https://fedimint.github.io/fedimint-sdk/)

## Deploy preview

You can preview this example live with [StackBlitz](https://stackblitz.com/github/fedimint/fedimint-sdk/tree/main/js/examples/vite-core)

[![Open in StackBlitz](https://developer.stackblitz.com/img/open_in_stackblitz.svg)](https://stackblitz.com/github/fedimint/fedimint-sdk/tree/main/js/examples/vite-core)

## Technologies Used

- @fedimint/core: The Fedimint client library for JavaScript applications
- Vite: A fast build tool and development server. Check the [`vite.config.ts`](./vite.config.ts) for configurations required for handling wasm.
- React

## Prerequisites

- Node.js
- pnpm (recommended) or npm

## Running the Application Locally

```bash
# from the js/ workspace root

# install dependencies
pnpm i

# run the dev server
pnpm run dev:vite
```
