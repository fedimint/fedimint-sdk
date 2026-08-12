# FedimintWallet constructor

::: warning Direct construction is not a production API
`FedimintWallet` is exported from `@fedimint/core` as a type. Application code
must obtain an instance from `WalletDirector.createWallet()`.
:::

The constructor accepts an initialized, director-owned `TransportClient`. It is
exposed through `@fedimint/core/testing` only for SDK tests and should not be used
by applications.

See [Creating a FedimintWallet](createWallet) for the supported creation flow.
