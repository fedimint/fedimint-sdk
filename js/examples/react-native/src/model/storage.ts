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

// MARK: Replacing the wallet
//
// A replace deletes the open wallet, as the earlier example did, but only once the wallet that
// replaces it is open. Until then the old one waits beside `dataDir` as `replacedDir`, and a note
// written before anything moves says whether it was created on this device. A note still there at
// launch means a replace was cut short, and `undoReplace` puts the old wallet back. A later move of
// the wallet to another location has to move these two as well.

const replacingNote = `${root}/fedimint-replacing`
const replacedDir = `${root}/fedimint-replaced`

/** What a replace in progress needs to be undone. */
interface ReplaceNote {
  /** Whether the old wallet carried the created marker, which belongs to it alone. */
  created: boolean
}

/**
 * Moves the open wallet out of `dataDir`, its note written first. The created marker goes with
 * it: left in place it would tell the wallet restored next that it was created here, and Join
 * would skip the recovery that finds its funds.
 */
export async function beginReplace(): Promise<void> {
  const note: ReplaceNote = { created: await RNFS.exists(createdMarker) }
  // Left by an earlier replace whose delete failed: the move needs the name free.
  if (await RNFS.exists(replacedDir)) await RNFS.unlink(replacedDir)
  await RNFS.writeFile(replacingNote, JSON.stringify(note))
  // A marker that will not go fails the replace, which the note then undoes.
  if (note.created) await RNFS.unlink(createdMarker)
  await RNFS.moveFile(dataDir, replacedDir)
}

/**
 * The new wallet is open: the replace is done, so the old wallet is deleted, and the federation
 * it attached last is forgotten. The note goes first: from then on a launch keeps the new wallet,
 * and an old wallet left by a kill in between is deleted by the next replace or launch.
 */
export async function finishReplace(): Promise<void> {
  await RNFS.unlink(replacingNote)
  await RNFS.unlink(lastFederationFile).catch(() => {})
  await RNFS.unlink(replacedDir).catch(() => {})
}

/**
 * Undoes a replace cut short, by a failure or by the app being killed, from wherever it stopped.
 * `dataDir` is touched only once the old wallet is out of it: before the move it is still the old
 * wallet. What a restore wrote there is dropped (the phrase recreates it). Without a note, only an
 * old wallet a finished replace left behind is deleted. The note goes last, so an undo that fails
 * is tried again by the next launch.
 */
export async function undoReplace(): Promise<void> {
  if (!(await RNFS.exists(replacingNote))) {
    // Tidying only, as in `finishReplace`: a leftover that will not go must not stop a launch.
    await RNFS.unlink(replacedDir).catch(() => {})
    return
  }
  // The old wallet out of place is what decides, not the note's contents: a note that cannot be
  // read must never leave it out, where the next launch deletes it as a finished replace's.
  if (await RNFS.exists(replacedDir)) {
    if (await RNFS.exists(dataDir)) await RNFS.unlink(dataDir)
    await RNFS.moveFile(replacedDir, dataDir)
  }
  // Unreadable means not created here: recovering a wallet that needed none costs a scan, while
  // skipping a recovery it needed hides its funds.
  if ((await readNote())?.created) await RNFS.writeFile(createdMarker, '')
  await RNFS.unlink(replacingNote)
}

async function readNote(): Promise<ReplaceNote | undefined> {
  try {
    const note = JSON.parse(await RNFS.readFile(replacingNote))
    return typeof note?.created === 'boolean'
      ? (note as ReplaceNote)
      : undefined
  } catch {
    return undefined
  }
}

/**
 * Whether an earlier launch left a wallet in the data directory. Only a hint for which screen to
 * start on; whether that storage really opens is still the SDK's answer. A directory that cannot
 * be read counts as empty.
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
      // A marker already absent is fine: the unlink's failure is ignored either way.
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
