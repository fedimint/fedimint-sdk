# @fedimint/react-native

## 0.1.1

### Patch Changes

- 516e51b: Upgrade TypeScript to 6.0.3 and fix resulting deprecations
- 516e51b: - Initial Release of react-native for Fedimint-SDK
  - Updated WalletDirector to Accept Path for react-native
  - Bumped packages to match versions across the monorepo
- ed1bd5f: Stop logging request and response payloads in `ReactNativeTransport`: they can carry secrets
  (mnemonic words, invite codes), which ended up in device logs. Only the message type and request
  id are logged now, at debug level, always through the injectable logger.
- 1cc0b3a: flatten RPC payload and use persistent callback in ReactNativeTransport
- Updated dependencies [b43a924]
- Updated dependencies [299e79b]
- Updated dependencies [516e51b]
- Updated dependencies [516e51b]
- Updated dependencies [bdba63f]
- Updated dependencies [48288a9]
- Updated dependencies [69fdcb1]
- Updated dependencies [c65cc13]
- Updated dependencies [cf43f91]
- Updated dependencies [82a1863]
- Updated dependencies [7002c28]
- Updated dependencies [1cc0b3a]
- Updated dependencies [1744c92]
  - @fedimint/core@0.2.0
  - @fedimint/react-native-bindings@0.1.1
  - @fedimint/types@0.0.4
