import { FedimintDemo } from '../../modules/fedimint-demo'

type AuthOutcome = 'ok' | 'unavailable' | { declined: string }

/**
 * Asks the owner to authenticate: a biometric, falling back to the device passcode, through the
 * app's native module, so a refusal reads in the system's own words. Fails closed: a device with
 * no passcode answers `unavailable` and nothing is shown.
 */
export async function authenticateOwner(reason: string): Promise<AuthOutcome> {
  try {
    return await FedimintDemo.confirmOwner(reason)
  } catch (error) {
    return { declined: error instanceof Error ? error.message : String(error) }
  }
}
