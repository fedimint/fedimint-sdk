---
'@fedimint/react-native': patch
'@fedimint/react-native-bindings': patch
---

Initial release of React Native packages

### Fixes

- Fix binary download tag format for consistent checksum verification
- Add `FEDIMINT_SKIP_BINARY_DOWNLOAD=true` environment variable support to skip binary downloads during npm install
- Fix package name in Expo plugin to correctly locate `@fedimint/react-native-bindings` for binary artifacts
