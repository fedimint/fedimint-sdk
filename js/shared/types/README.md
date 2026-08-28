# @fedimint/types

Shared TypeScript definitions for transport adapters used by the Fedimint SDK. These types express the contract between `TransportClient` implementations and platform-specific transports (web worker, React Native, etc.).

The package exports:

- `Transport` and supporting handler interfaces
- message payload structures (`TransportMessage`, `TransportRequest`)
- `TRANSPORT_MESSAGE_TYPES`

Use this from both core logic and transport adapters to stay type-aligned without creating circular package dependencies.
