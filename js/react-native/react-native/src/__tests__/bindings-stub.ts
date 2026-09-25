/**
 * Test stand-in for the ubrn-generated `@fedimint/react-native-bindings` module, injected through
 * a Vitest alias (the `react-native` project in vitest.config.ts) and a `paths` mapping
 * (tsconfig.test.json), so `openSdk` can be exercised without a compiled native library.
 */
export class Mnemonic {
  static fromWordsCalls: string[][] = []
  static generated = 0

  constructor(public words: string[]) {}

  static fromWords(words: string[]): Mnemonic {
    Mnemonic.fromWordsCalls.push(words)
    return new Mnemonic(words)
  }

  static generate(): Mnemonic {
    Mnemonic.generated += 1
    return new Mnemonic(['generated'])
  }
}

export class InviteCode {
  static parsed: string[] = []

  static parse(code: string): InviteCode {
    InviteCode.parsed.push(code)
    return new InviteCode()
  }
}

export class Notes {
  static parsed: string[] = []

  static parse(notes: string): Notes {
    Notes.parsed.push(notes)
    return new Notes()
  }
}

export class Sdk {
  static instances: Sdk[] = []
  static failNext: Error | undefined

  shutdowns = 0
  destroys = 0
  failShutdown: Error | undefined

  constructor(
    public dataDir: string,
    public mnemonic: Mnemonic | undefined,
  ) {
    Sdk.instances.push(this)
  }

  async shutdown(): Promise<void> {
    this.shutdowns += 1
    if (this.failShutdown) throw this.failShutdown
  }

  uniffiDestroy(): void {
    this.destroys += 1
  }
}

export type SdkLike = Sdk
export type MnemonicLike = Mnemonic

export async function createFedimintSdk(
  dataDir: string,
  mnemonic: Mnemonic | undefined,
): Promise<Sdk> {
  if (Sdk.failNext) {
    const error = Sdk.failNext
    Sdk.failNext = undefined
    throw error
  }
  return new Sdk(dataDir, mnemonic)
}
