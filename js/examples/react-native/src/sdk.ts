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

/** Whether a wallet exists on this device: the SDK seeds its data directory on first open. */
export const walletExists = (): Promise<boolean> => RNFS.exists(dataDir)

/** Opens the SDK once and shares the session; `mnemonic` restores a seed over a fresh directory. */
export function open(mnemonic?: string[]): Promise<SdkSession> {
  if (session) return Promise.resolve(session)
  if (!opening) {
    opening = openSdk({ dataDir, mnemonic }).then(
      (opened) => {
        session = opened
        notify()
        return opened
      },
      (error) => {
        opening = undefined
        throw error
      },
    )
  }
  return opening
}

/** Closes the session and deletes the wallet's data, so a restore can seed the directory anew. */
export async function reset(): Promise<void> {
  // The session is dropped first, so a failing close still leaves the module without one, the
  // data still gets deleted and listeners still hear about it; the error surfaces afterwards.
  const closing = session
  session = undefined
  opening = undefined
  try {
    if (closing) await closing.close()
  } finally {
    if (await RNFS.exists(dataDir)) await RNFS.unlink(dataDir)
    notify()
  }
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
