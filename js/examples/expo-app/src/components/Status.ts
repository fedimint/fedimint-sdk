import {
  FederationStatus_Tags,
  type Capabilities,
  type FederationStatus,
} from '@fedimint/react-native'
import type { Tone } from './Labels'

/** How a `FederationStatus` reads and which tone it carries, shared by every screen that shows one. */
export const FederationStatusStyle = {
  label(status?: FederationStatus): string {
    switch (status?.tag) {
      case FederationStatus_Tags.Running:
        return 'Running'
      case FederationStatus_Tags.Recovering:
        return 'Recovering'
      case FederationStatus_Tags.Quarantined:
        return 'Quarantined'
      case FederationStatus_Tags.Closed:
        return 'Closed'
      case FederationStatus_Tags.Forgetting:
        return 'Removing'
      case FederationStatus_Tags.Forgotten:
        return 'Removed'
      default:
        return 'Unknown'
    }
  },

  /**
   * What tapping a federation that is not attached does, or `undefined` when nothing can: the SDK
   * reopens neither a federation being removed nor one already removed.
   */
  action(status?: FederationStatus): string | undefined {
    switch (status?.tag) {
      case FederationStatus_Tags.Running:
      case FederationStatus_Tags.Recovering:
        return 'Switch'
      default:
        return FederationStatusStyle.reconnects(status)
          ? 'Reconnect'
          : undefined
    }
  },

  /** Stored and intact but not running, so `reopenFederation` brings it back. */
  reconnects(status?: FederationStatus): boolean {
    return (
      status?.tag === FederationStatus_Tags.Quarantined ||
      status?.tag === FederationStatus_Tags.Closed
    )
  },

  tone(status?: FederationStatus): Tone {
    switch (status?.tag) {
      case FederationStatus_Tags.Running:
        return 'success'
      case FederationStatus_Tags.Recovering:
        return 'warning'
      case FederationStatus_Tags.Quarantined:
        return 'danger'
      default:
        return 'neutral'
    }
  },
}

/** The three ways money moves, as Send and Receive offer them. */
export type PaymentRail = 'lightning' | 'ecash' | 'onchain'

export const PaymentRails = {
  title(rail: PaymentRail): string {
    return { lightning: 'Lightning', ecash: 'Ecash', onchain: 'Onchain' }[rail]
  },
  /** Only the rails the federation has, in the design's order: never shown disabled. */
  available(capabilities?: Capabilities): PaymentRail[] {
    if (!capabilities) return []
    const rails: PaymentRail[] = []
    if (capabilities.lightning) rails.push('lightning')
    if (capabilities.ecash) rails.push('ecash')
    if (capabilities.onchain) rails.push('onchain')
    return rails
  },
}
