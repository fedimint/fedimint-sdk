# FedimintWallet Overview

The `FedimintWallet` class serves as the main entry point for the library. It orchestrates the various services and the TransportClient.

::: info
Check out the [Getting Started](../getting-started) guide to get started using the Fedimint Sdk.
:::

<img src="/architecture-diagram.svg" alt="Architecture" />

## Properties

### balance

> **balance**: `BalanceService`

#### Defined in

[FedimintWallet.ts:18](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L18)

---

### federation

> **federation**: `FederationService`

#### Defined in

[FedimintWallet.ts:21](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L21)

---

### lightning

> **lightning**: `LightningService`

#### Defined in

[FedimintWallet.ts:20](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L20)

---

### mint

> **mint**: `MintService`

#### Defined in

[FedimintWallet.ts:19](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L19)

---

### recovery

> **recovery**: `RecoveryService`

#### Defined in

[FedimintWallet.ts:22](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L22)

## Methods

### cleanup()

> **cleanup**(): `Promise`\<`void`\>

This should ONLY be called when UNLOADING the wallet client.
After this call, the FedimintWallet instance should be discarded.

#### Returns

`Promise`\<`void`\>

#### Defined in

[FedimintWallet.ts:134](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L134)

---

### initialize()

> **initialize**(): `Promise`\<`void`\>

#### Returns

`Promise`\<`void`\>

#### Defined in

[FedimintWallet.ts:85](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L85)

---

### isOpen()

> **isOpen**(): `boolean`

#### Returns

`boolean`

#### Defined in

[FedimintWallet.ts:140](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L140)

---

### parseInviteCode()

> **parseInviteCode**(`inviteCode`): `Promise`\<`ParsedInviteCode`\>

Parses an invite code without requiring an open wallet or joined federation.

#### Parameters

• **inviteCode**: `string`

#### Returns

`Promise`\<`ParsedInviteCode`\>

The parsed invite code data containing `federation_id` and `url`.

#### Defined in

[WalletDirector.ts:109](https://github.com/fedimint/fedimint-sdk/tree/main/packages/core/src/WalletDirector.ts#L109)

---

### parseBolt11Invoice()

> **parseBolt11Invoice**(`invoiceStr`): `Promise`\<`ParsedBolt11Invoice`\>

Parses a BOLT11 invoice without requiring an open wallet or joined federation.

#### Parameters

• **invoiceStr**: `string`

#### Returns

`Promise`\<`ParsedBolt11Invoice`\>

The parsed invoice data where `amount` is in satoshis.

#### Defined in

[WalletDirector.ts:137](https://github.com/fedimint/fedimint-sdk/tree/main/packages/core/src/WalletDirector.ts#L137)

---

### joinFederation()

> **joinFederation**(`inviteCode`, `clientName`): `Promise`\<`void`\>

#### Parameters

• **inviteCode**: `string`

• **clientName**: `string` = `DEFAULT_CLIENT_NAME`

#### Returns

`Promise`\<`void`\>

#### Defined in

[FedimintWallet.ts:110](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L110)

---

### previewFederation(inviteCode)

> **previewFederation**(`inviteCode`): `Promise`\<`PreviewFederation`\>

Retrieves federation details before joining a Federation. This allows you to inspect the federation's configuration, endpoints, modules, and metadata.

#### Parameters

• **inviteCode**: `string`

#### Returns

`Promise`\<`PreviewFederation`\>

Federation preview information containing:

- `config`: Detailed JSON configuration including global settings, API endpoints, consensus version, and module configurations
- `federation_id`: The unique federation identifier

#### Defined in

[WalletDirector.ts:137](https://github.com/fedimint/fedimint-sdk/tree/main/packages/core/src/WalletDirector.ts#L71)

---

### parseOobNotes(notes)

> **parseOobNotes**(`notes`): `Promise`\<`ParsedNoteDetails`\>

Parses OOB notes and retrieves their details. It allows you to inspect the contents of OOB notes before redeeming them.

#### Parameters

• **notes**: `string`

#### Returns

`Promise`\<`ParsedNoteDetails`\>

The parsed note details containing `total_amount`, `federation_id_prefix`, `federation_id`, `invite_code`, and `note_counts`.

#### Defined in

[WalletDirector.ts:205](https://github.com/fedimint/fedimint-sdk/tree/main/packages/core/src/WalletDirector.ts#L205)

---

### open()

> **open**(`clientName`): `Promise`\<`any`\>

#### Parameters

• **clientName**: `string` = `DEFAULT_CLIENT_NAME`

#### Returns

`Promise`\<`any`\>

#### Defined in

[FedimintWallet.ts:96](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L96)

---

### setLogLevel()

> **setLogLevel**(`level`): `void`

Sets the log level for the library.

#### Parameters

• **level**: `"debug"` \| `"info"` \| `"warn"` \| `"error"` \| `"none"`

The desired log level ('DEBUG', 'INFO', 'WARN', 'ERROR', 'NONE').

#### Returns

`void`

#### Defined in

[FedimintWallet.ts:148](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L148)

---

### waitForOpen()

> **waitForOpen**(): `Promise`\<`null` \| `void`\>

#### Returns

`Promise`\<`null` \| `void`\>

#### Defined in

[FedimintWallet.ts:91](https://github.com/fedimint/fedimint-sdk/blob/451b02527305a23fec3a269d39bde9a3ec377df2/packages/core/src/FedimintWallet.ts#L91)
