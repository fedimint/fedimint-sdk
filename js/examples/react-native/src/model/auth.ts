import { NativeModules } from 'react-native'

/**
 * Device-owner authentication: Face ID, Touch ID or a fingerprint, falling back to the device
 * passcode, the policy the iOS demo uses (`.deviceOwnerAuthentication`). The example's own
 * module, ios/ReactNativeExample/OwnerAuthentication.m and its Android twin, so a failure reads
 * in the system's own words on both platforms.
 */
const OwnerAuthentication = NativeModules.OwnerAuthentication as {
  confirm(reason: string): Promise<'ok' | 'unavailable'>
}

type AuthOutcome = 'ok' | 'unavailable' | { declined: string }

/**
 * Asks the owner to authenticate. Fails closed: a device that cannot authenticate its owner,
 * one with no passcode, answers `unavailable` and nothing is shown.
 */
export async function authenticateOwner(reason: string): Promise<AuthOutcome> {
  try {
    return await OwnerAuthentication.confirm(reason)
  } catch (error) {
    return { declined: error instanceof Error ? error.message : String(error) }
  }
}
