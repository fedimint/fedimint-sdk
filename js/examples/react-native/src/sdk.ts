import RNFS from 'react-native-fs'
import {
  ErrorCode,
  Exception,
  Mnemonic,
  openSdk,
  type Amount,
  type FederationLike,
  type SdkSession,
} from '@fedimint/react-native'

export const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint`

let session: SdkSession | undefined
let opening: Promise<SdkSession> | undefined
let closing: Promise<void> = Promise.resolve()
let generation = 0

// Screens that render session-derived state (whether a wallet exists, whether a federation
// is joined) subscribe here through `onChange` and re-check that state whenever the module
// singleton is replaced, instead of relying on being told directly by whichever screen
// triggered the change.
const listeners = new Set<() => void>()

/** Subscribes to session changes (open or reset); call the returned function to unsubscribe. */
export function onChange(listener: () => void): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

function notify(): void {
  for (const listener of listeners) listener()
}

/** A directory may exist before setup; only a stored mnemonic counts as a wallet. */
export const walletExists = async (): Promise<boolean> => (await open()).sdk.hasMnemonic()

/** Shares one session for browsing, then saves words chosen during Create or Restore. */
export async function open(mnemonic?: string[]): Promise<SdkSession> {
  const mine = generation
  opening ??= closing.then(() => openSdk({ dataDir })).catch((error: unknown) => {
    if (mine === generation) opening = undefined
    throw error
  })
  const opened = await opening
  if (mine !== generation) throw new Error('wallet open superseded by reset')
  if (mnemonic) {
    await opened.sdk.setMnemonic(opened.Mnemonic.fromWords(mnemonic))
  }
  if (mine !== generation) throw new Error('wallet setup superseded by reset')
  const changed = session !== opened || mnemonic !== undefined
  session = opened
  if (changed) notify()
  return opened
}

/** Waits for the old opener and closes it before deleting data or allowing a new open. */
export function reset(): Promise<void> {
  const pending = opening
  const mine = ++generation
  session = undefined
  opening = undefined
  const run = closing.then(async () => {
    const old = await pending
    if (old) await old.close()
    if (await RNFS.exists(dataDir)) await RNFS.unlink(dataDir)
  })
  // Preserve ordering without making a failed reset poison later attempts.
  closing = run.catch(() => undefined)
  return run.finally(() => {
    if (mine === generation) notify()
  })
}

export const current = (): SdkSession | undefined => session

/** The one federation these demos work with, once joined. */
export const federation = (): FederationLike | undefined => session?.sdk.federations()[0]

export const generateWords = (): string[] => Mnemonic.generate().words()

export const msatToSat = (amount: Amount): number => Number(amount / 1000n)

export function satToMsat(sats: string | number): Amount {
  const n = typeof sats === 'number' ? sats : Number(sats)
  if (!Number.isInteger(n) || n < 0) throw new Error('amount must be a whole number of sats')
  return BigInt(n) * 1000n
}

export function errorMessage(error: unknown): string {
  if (Exception.hasInner(error)) {
    const inner = Exception.getInner(error)
    return `${ErrorCode[inner.code()]}: ${inner.reason()}`
  }
  return error instanceof Error ? error.message : String(error)
}
