import {
  InviteCode,
  Mnemonic,
  Notes,
  Sdk,
  createFedimintSdk,
  type MnemonicLike,
  type SdkLike,
} from '@fedimint/react-native-bindings'

export * from '@fedimint/react-native-bindings'

export interface OpenOptions {
  /** Directory the SDK keeps its data in; the app's documents directory is the usual choice. */
  dataDir: string
  /**
   * Seed words to establish; omit to use the stored seed, or to generate one over an empty
   * directory.
   */
  mnemonic?: string[]
}

export interface SdkSession {
  sdk: SdkLike
  InviteCode: { parse: typeof InviteCode.parse }
  Notes: { parse: typeof Notes.parse }
  Mnemonic: {
    fromWords: typeof Mnemonic.fromWords
    generate: typeof Mnemonic.generate
  }
  /**
   * Shuts the SDK down and lets go of it. Every object from this session is dead after; a
   * second call does nothing.
   *
   * Letting go is what gives the data directory back. The SDK holds a directory for as long as
   * anything built on it is alive, so an application that closes a wallet and opens one over the
   * same directory has to close the session first, and destroy any other object it still holds
   * from it: a federation, an operation, a subscription. Opening a directory that is still held
   * fails with `StorageInUse` rather than waiting for it to come free.
   */
  close(): Promise<void>
}

/** Opens the SDK over `options.dataDir`. */
export async function openSdk(options: OpenOptions): Promise<SdkSession> {
  const mnemonic: MnemonicLike | undefined = options.mnemonic
    ? Mnemonic.fromWords(options.mnemonic)
    : undefined
  // Typed to the interface by the generated bindings; the object is the generated class, whose
  // teardown `close` needs.
  const sdk = (await createFedimintSdk(options.dataDir, mnemonic)) as Sdk
  let closed = false
  return {
    sdk,
    InviteCode: { parse: (code) => InviteCode.parse(code) },
    Notes: { parse: (notes) => Notes.parse(notes) },
    Mnemonic: {
      fromWords: (words) => Mnemonic.fromWords(words),
      generate: () => Mnemonic.generate(),
    },
    async close() {
      // A second close is a no-op rather than a rejection: the SDK is already gone.
      if (closed) return
      closed = true
      try {
        await sdk.shutdown()
      } finally {
        // Shutting down only ends the SDK's work. Destroying it is what releases the native
        // object behind it, and with that this session's hold on the data directory. Left to the
        // garbage collector, that hold lasts until the collector happens to run. It runs even
        // when the shutdown fails, because a failed flush still leaves an instance that is
        // closed, and nothing would ever hand the directory back otherwise.
        sdk.uniffiDestroy()
      }
    },
  }
}
