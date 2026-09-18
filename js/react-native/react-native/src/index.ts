import {
  InviteCode,
  Mnemonic,
  Notes,
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
   * Shuts the SDK down. Every object from this session is dead after; a second call does
   * nothing.
   */
  close(): Promise<void>
}

/** Opens the SDK over `options.dataDir`. */
export async function openSdk(options: OpenOptions): Promise<SdkSession> {
  const mnemonic: MnemonicLike | undefined = options.mnemonic
    ? Mnemonic.fromWords(options.mnemonic)
    : undefined
  const sdk = await createFedimintSdk(options.dataDir, mnemonic)
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
      await sdk.shutdown()
    },
  }
}
