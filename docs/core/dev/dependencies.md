# Updating dependencies

Use [Taze](https://github.com/antfu/taze) by running:

```bash
pnpm deps       # prints outdated deps
pnpm deps patch # print outdated deps with new patch versions
pnpm deps -w    # updates deps (best done with clean working tree)
```

<!-- [Socket](https://socket.dev) checks pull requests for vulnerabilities when new dependencies and versions are added, but you should also be vigilant! When updating dependencies, you should check release notes and source code as well as lock versions when possible. -->
