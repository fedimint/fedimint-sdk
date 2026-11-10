import { Platform } from 'react-native'
import RNFS from 'react-native-fs'

// Where the wallet lives on the device, and the two small facts kept beside it: whether this
// device created the seed, and which federation was attached last.

/**
 * The SDK's directory. On iOS it is Application Support, excluded from backup, not Documents,
 * which the Files app shows and iCloud backs up: the seed is in there. Android's files directory
 * is already private to the app.
 */
const root =
  Platform.OS === 'ios'
    ? `${RNFS.LibraryDirectoryPath}/Application Support`
    : RNFS.DocumentDirectoryPath
export const dataDir = `${root}/fedimint`

/** Beside the data directory rather than inside, so the SDK's holds only what it put there. */
const createdMarker = `${root}/fedimint-created`
const lastFederationFile = `${root}/fedimint-last-federation`

/**
 * Moves a wallet the earlier example left in Documents on iOS to `dataDir`, once, before anything
 * looks for one: otherwise the new location reads as empty, Welcome offers a fresh wallet, and the
 * old seed stays in the iCloud-backed folder. Android's location has not changed.
 */
export async function moveLegacyWallet(): Promise<void> {
  if (Platform.OS !== 'ios') return
  const legacy = `${RNFS.DocumentDirectoryPath}/fedimint`
  if (!(await RNFS.exists(legacy)) || (await RNFS.exists(dataDir))) return
  await RNFS.mkdir(root)
  await RNFS.moveFile(legacy, dataDir)
}

/** Creates the data directory, kept out of backups. */
export async function prepareDataDir(): Promise<string> {
  await RNFS.mkdir(dataDir, { NSURLIsExcludedFromBackupKey: true })
  return dataDir
}

/**
 * Whether an earlier launch left a wallet in the data directory. Only a hint for which screen to
 * start on; whether that storage really opens is still the SDK's answer. A directory that cannot
 * be read counts as empty, as on iOS (`try? contentsOfDirectory`).
 */
export async function hasStoredWallet(): Promise<boolean> {
  try {
    if (!(await RNFS.exists(dataDir))) return false
    return (await RNFS.readDir(dataDir)).length > 0
  } catch {
    return false
  }
}

export type WalletOrigin = 'created' | 'restored' | 'reopened'

/**
 * Records how the wallet just opened came about, and answers whether Join should recover for it.
 * Creating a wallet sets the marker and restoring one clears it; a reopen reads what an earlier
 * launch left.
 */
export async function recoversOnJoin(origin: WalletOrigin): Promise<boolean> {
  switch (origin) {
    case 'created':
      await RNFS.writeFile(createdMarker, '').catch(() => {})
      return false
    case 'restored':
      if (await RNFS.exists(createdMarker).catch(() => false))
        await RNFS.unlink(createdMarker).catch(() => {})
      return true
    case 'reopened':
      // Unreadable counts as not created here: recovering a wallet that needed none costs a
      // scan, while skipping a recovery it needed hides its funds.
      return !(await RNFS.exists(createdMarker).catch(() => false))
  }
}

/** The federation attached last, so a relaunch comes back to it. */
export async function readLastFederationId(): Promise<string | undefined> {
  // Unreadable is as good as unset: the wallet still opens, on its first federation.
  const id = await RNFS.readFile(lastFederationFile).catch(() => '')
  return id.trim() || undefined
}

export function writeLastFederationId(id: string): void {
  RNFS.writeFile(lastFederationFile, id).catch(() => {})
}
