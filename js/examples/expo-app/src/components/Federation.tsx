import type { ReactNode } from 'react'
import { StyleSheet, Text, View } from 'react-native'
import {
  FederationStatus_Tags,
  Network,
  type FederationStatus,
} from '@fedimint/react-native'
import { StatusPill } from './Labels'
import { FederationStatusStyle } from './Status'
import { Metrics, Palette, Typeface } from '../theme/tokens'

/** How a `Network` is named on screen. */
export function networkName(network?: Network): string {
  switch (network) {
    case Network.Bitcoin:
      return 'mainnet'
    case Network.Testnet:
      return 'testnet'
    case Network.Testnet4:
      return 'testnet4'
    case Network.Signet:
      return 'signet'
    case Network.Regtest:
      return 'regtest'
    default:
      return 'unknown'
  }
}

/** A federation's initials on a navy tile, standing in for a logo the SDK does not provide. */
export function FederationAvatar({ name }: { name?: string }) {
  const words = (name ?? '').split(/[^\p{L}\p{N}]+/u).filter(Boolean)
  const initials = words
    .slice(0, 2)
    // A whole code point: `word[0]` would halve one outside the BMP.
    .map((word) => Array.from(word)[0])
    .join('')
    .toUpperCase()
  return (
    <View
      style={styles.avatar}
      accessibilityElementsHidden
      importantForAccessibility="no"
    >
      <Text style={[Typeface.mono('SemiBold', 15), { color: '#FFFFFF' }]}>
        {initials || '?'}
      </Text>
    </View>
  )
}

/** A federation in the list: avatar, name, one line under it, its status, and what tapping does. */
export function FederationRow({
  name,
  status,
  detail,
  failure,
  trailing,
  trailingLabel,
}: {
  name?: string
  status?: FederationStatus
  detail?: string
  /** Why the last switch to this one failed. */
  failure?: string
  trailing?: ReactNode
  /** What `trailing` reads as, when it is the row's action ("Reconnect") rather than decoration. */
  trailingLabel?: string
}) {
  const title = name ?? 'Unnamed federation'
  const statusText = FederationStatusStyle.label(status)
  const diagnostic =
    status?.tag === FederationStatus_Tags.Quarantined
      ? (status.inner as { diagnostic: { message: string } }).diagnostic.message
      : undefined
  return (
    <View
      style={styles.row}
      accessible
      // The text alone: gathered from the children, it would include the initials and the
      // chevron's symbol name.
      accessibilityLabel={[
        title,
        detail,
        statusText,
        diagnostic,
        failure,
        trailingLabel,
      ]
        .filter((part) => part !== undefined)
        .join(', ')}
    >
      <FederationAvatar name={name} />
      <View style={styles.text}>
        <Text style={[Typeface.headline, { color: Palette.ink }]}>{title}</Text>
        {detail && (
          <Text style={[Typeface.footnote, { color: Palette.muted }]}>
            {detail}
          </Text>
        )}
        <StatusPill
          text={statusText}
          tone={FederationStatusStyle.tone(status)}
        />
        {diagnostic !== undefined && (
          <Text style={[Typeface.footnote, { color: Palette.danger }]}>
            {diagnostic}
          </Text>
        )}
        {failure && (
          <Text
            selectable
            style={[Typeface.footnote, { color: Palette.danger }]}
          >
            {failure}
          </Text>
        )}
      </View>
      {trailing}
    </View>
  )
}

const styles = StyleSheet.create({
  avatar: {
    width: 44,
    height: 44,
    borderRadius: 12,
    backgroundColor: Palette.navy,
    alignItems: 'center',
    justifyContent: 'center',
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Metrics.gap,
    padding: 16,
  },
  text: { flex: 1, gap: 4 },
})
