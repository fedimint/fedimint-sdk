# create-fedimint-app

## Scaffolding Your First Fedimint App

> **Compatibility Note:**
> Requires [Node.js](https://nodejs.org/en/) version 18+, 20+

With NPM:

```bash
$ npm create fedimint-app@latest
```

With Yarn:

```bash
$ yarn create fedimint-app
```

With PNPM:

```bash
$ pnpm create fedimint-app
```

Then follow the prompts!

You can also directly specify the project name and the template you want to use via additional command line options. For example, to scaffold a Fedimint + React project, run:

```bash
# npm 7+, extra double-dash is needed:
npm create fedimint-app@latest my-fedimint-app -- --template vite-react-ts

# yarn
yarn create fedimint-app my-fedimint-app --template vite-react-ts

# pnpm
pnpm create fedimint-app my-fedimint-app --template vite-react-ts
```

Currently supported template presets include:

- `vite-react-ts` - Vite + React + TypeScript template with Fedimint integration
- `vite-react` - Vite + React template with Fedimint integration

You can use `.` for the project name to scaffold in the current directory.

## Getting Started

After creating your project, install dependencies and start the dev server:

```bash
cd my-fedimint-app
npm install
npm run dev
```

This will start a development server with hot module replacement. The template includes:

- React + TypeScript setup
- Fedimint wallet integration
- Basic styling with CSS
- Vite for fast development and building
