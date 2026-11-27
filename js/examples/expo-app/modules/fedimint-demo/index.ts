import { requireNativeModule } from 'expo'

/** Device-owner authentication and the ecash copy, which no library the app uses offers. */
export const FedimintDemo = requireNativeModule<{
  /** Resolves `ok`, or `unavailable` on a device with no passcode; rejects when declined. */
  confirmOwner(reason: string): Promise<'ok' | 'unavailable'>
  /** Copies `value` to this device only, expiring after `lifetime` seconds. */
  copyBearer(value: string, lifetime: number): Promise<void>
}>('FedimintDemo')
