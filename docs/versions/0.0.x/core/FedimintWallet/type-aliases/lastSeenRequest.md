[Core Web](../globals.md) / LastSeenRequest

# Type Alias: LastSeenRequest

> **LastSeenRequest**: `object`

Represents a pagination key for fetching operations in `listOperations` method.

## Type Declaration

### creation_time

> **creation_time**: `object`

Timestamp of the operation used as a pagination cursor.

#### secs_since_epoch

> **secs_since_epoch**: `number`

Seconds since the Unix epoch

#### nanos_since_epoch

> **nanos_since_epoch**: `number`

Nanoseconds for sub-second precision

### operation_id

> **operation_id**: `string`

Unique identifier for the operation

## Defined in

[types/wallet.ts:5](https://github.com/fedimint/fedimint-sdk/blob/main/packages/core/src/types/wallet.ts#122)
