import { expectTypeOf, it } from 'vitest'
import type {
  BalanceUpdatesLike,
  FederationLike,
  FederationStatus,
  FederationStatus_Tags,
  LnReceiveHandle,
  LnReceiveOperationLike,
  SdkLike,
} from '../generated/fedimint_sdk'
import type { Proxied, ProxiedValue } from '../types'

it('maps objects to handles and records structurally', () => {
  type Sdk = Proxied<SdkLike>
  expectTypeOf<Sdk['exportMnemonic']>().returns.resolves.toMatchTypeOf<{
    words(): Promise<string[]>
  }>()
  expectTypeOf<Sdk['join']>().returns.resolves.toEqualTypeOf<
    Proxied<FederationLike>
  >()
  expectTypeOf<
    Proxied<FederationLike>['balance']
  >().returns.resolves.toEqualTypeOf<bigint>()
  expectTypeOf<ProxiedValue<LnReceiveHandle>>().toEqualTypeOf<{
    invoice: string
    operation: Proxied<LnReceiveOperationLike>
  }>()
})

it('keeps a tagged enum as its tag and data fields, never as a handle', () => {
  type Status = ProxiedValue<FederationStatus>
  expectTypeOf<
    Extract<Status, { tag: FederationStatus_Tags.Running }>
  >().toMatchTypeOf<{
    tag: FederationStatus_Tags.Running
  }>()
  type Quarantined = Extract<Status, { tag: FederationStatus_Tags.Quarantined }>
  expectTypeOf<Quarantined>().toHaveProperty('inner')
  expectTypeOf<Quarantined['inner']>().toHaveProperty('diagnostic')
  // Symbol-keyed and method members of the generated variant classes are gone; `inner` is
  // Quarantined's own data field, so it is not part of the tag-only intersection every variant
  // shares.
  expectTypeOf<keyof Status>().toEqualTypeOf<'tag'>()
})

it('keeps an AbortSignal argument as itself, not a handle', () => {
  // `updates.next({ signal })` must type-check: the record carrying the signal maps
  // structurally, and the signal inside it keeps its own type rather than becoming a `Proxied`
  // handle, because it never crosses to the worker.
  type Next = Proxied<BalanceUpdatesLike>['next']
  expectTypeOf<Next>()
    .parameter(0)
    .toEqualTypeOf<{ signal: AbortSignal } | undefined>()
})
