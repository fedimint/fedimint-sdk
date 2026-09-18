import { WorkerSession } from './client'
import type { WorkerLike } from './client'
import type {
  InviteCode,
  Mnemonic,
  MnemonicLike,
  Notes,
  SdkLike,
} from './generated/fedimint_sdk'
import type { Proxied, ProxiedValue } from './types'

export { SdkError, SessionClosed, WorkerCrashed } from './errors'
export type { Proxied, ProxiedValue } from './types'
export type * from './generated/fedimint_sdk'
export {
  ActivityStatus,
  Direction,
  EcashReceiveState_Tags,
  EcashSendState,
  ErrorCode,
  FederationStatus_Tags,
  LightningRoute_Tags,
  LnReceiveState_Tags,
  LnSendState_Tags,
  Network,
  OnchainReceiveState_Tags,
  OnchainSendState_Tags,
  OperationKind,
  OperationSupport,
  RecoveryState_Tags,
} from './generated/fedimint_sdk'

export interface OpenOptions {
  /**
   * Name of this instance's origin-private store: 1 to 64 chars of letters, digits, `-`, `_`,
   * `.`.
   */
  storage: string
  /**
   * Seed words to establish; omit to use the stored seed, or to generate one over empty
   * storage.
   */
  mnemonic?: string[]
  /** Supplies the worker; defaults to this package's own `./worker.js` as a module worker. */
  createWorker?: () => WorkerLike
}

type Static<F> = F extends (...args: infer A) => infer R
  ? (
      ...args: { [I in keyof A]: ProxiedValue<A[I]> }
    ) => Promise<ProxiedValue<Awaited<R>>>
  : never

export interface SdkSession {
  sdk: Proxied<SdkLike>
  InviteCode: { parse: Static<typeof InviteCode.parse> }
  Notes: { parse: Static<typeof Notes.parse> }
  Mnemonic: {
    fromWords: Static<typeof Mnemonic.fromWords>
    generate: Static<typeof Mnemonic.generate>
  }
  /**
   * Shuts the SDK down and terminates the worker. Every handle from this session is dead
   * after; a second call does nothing.
   */
  close(): Promise<void>
}

const defaultWorker = (): WorkerLike =>
  new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })

/** Opens the SDK over `options.storage` in a dedicated worker. */
export async function openSdk(options: OpenOptions): Promise<SdkSession> {
  // Constructed inside the try: a throwing createWorker or constructor must leave nothing
  // behind, so termination on the catch path only ever applies to a session that exists.
  let session: WorkerSession | undefined
  try {
    session = new WorkerSession((options.createWorker ?? defaultWorker)())
    // A `const` alias so the closures below capture a type the compiler knows is defined; the
    // outer `session` stays reassignable only to let the catch below terminate it.
    const opened = session
    let closed = false
    const mnemonic = options.mnemonic
      ? ((await opened.callStatic('Mnemonic', 'fromWords', [
          options.mnemonic,
        ])) as Proxied<MnemonicLike>)
      : undefined
    const sdk = (await opened.callStatic(undefined, 'createFedimintSdk', [
      options.storage,
      mnemonic,
    ])) as Proxied<SdkLike>
    const statik =
      (cls: string, fn: string) =>
      (...args: unknown[]) =>
        opened.callStatic(cls, fn, args)
    return {
      sdk,
      InviteCode: {
        parse: statik('InviteCode', 'parse') as Static<typeof InviteCode.parse>,
      },
      Notes: { parse: statik('Notes', 'parse') as Static<typeof Notes.parse> },
      Mnemonic: {
        fromWords: statik('Mnemonic', 'fromWords') as Static<
          typeof Mnemonic.fromWords
        >,
        generate: statik('Mnemonic', 'generate') as Static<
          typeof Mnemonic.generate
        >,
      },
      async close() {
        // A second close is a no-op rather than a rejection: the worker is already gone.
        if (closed) return
        closed = true
        try {
          await sdk.shutdown()
        } finally {
          opened.terminate()
        }
      },
    }
  } catch (error) {
    session?.terminate()
    throw error
  }
}
