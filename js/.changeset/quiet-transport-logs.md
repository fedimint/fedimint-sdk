---
'@fedimint/react-native': patch
---

Stop logging request and response payloads in `ReactNativeTransport`: they can carry secrets
(mnemonic words, invite codes), which ended up in device logs. Only the message type and request
id are logged now, at debug level, always through the injectable logger.
