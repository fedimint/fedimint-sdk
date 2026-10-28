# Fedimint SDK docs

The documentation site, built with [VitePress](https://vitepress.dev). Each SDK has a folder:

```
docs/
  .vitepress/          site config and theme
  index.md             the home page
  react-native/        the React Native guide and API pages, nav.json, VERSION, CONVENTIONS.md
  tools/               extract_snippets.py, set_version.py, build_llms.py
  tests/               tests for the tools
  fixtures/react-native/  every code block on the pages, as files tsc checks
skills/                (repo root) agent skills for people who use the SDK
```

## Run it

```sh
cd docs
pnpm install
pnpm dev        # http://localhost:5173
pnpm build      # writes .vitepress/dist; fails on a dead link
```

`pnpm dev` and `pnpm build` first run `tools/build_llms.py`. It writes the plain-Markdown copy of
each page, `llms.txt` and `llms-full.txt` into `public/react-native/` (generated, not committed).

## Check the code on the pages

Every code block on a React Native page is a file that compiles against this repository's own
packages. The pages are the source; the fixtures are generated from them.

```sh
cd docs
python3 tools/extract_snippets.py                 # pages -> fixtures/react-native/ (not committed)
cd fixtures/react-native && npm install && npm run check   # extract again, then tsc
```

The type check needs the `js/` workspace installed (`pnpm --dir js install`), because the
fixtures link to `js/react-native/react-native` and `js/react-native/react-native-bindings`.

How to mark a code block: `react-native/CONVENTIONS.md`.

## Change the package version

```sh
cd docs
python3 tools/set_version.py 0.1.0-beta.3   # VERSION, every install command, then rebuild
python3 tools/set_version.py --check        # what CI runs
```

CI (`.github/workflows/docs.yaml`) runs the tool tests, the version check, the fixture check and
`vitepress build` on every pull request that touches `docs/` or `skills/`, and on every
`react-native-sdk-v*` tag. Nothing deploys the site yet.

## Tests

```sh
cd docs && python3 -m unittest discover -s tests
```
