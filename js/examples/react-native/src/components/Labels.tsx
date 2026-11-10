import React from 'react'
import { StyleSheet, Text, View } from 'react-native'
import { Palette, Typeface } from '../theme/tokens'

/** The small uppercase label above a title, a balance or a section: "TOTAL BALANCE". */
export function Caption({
  text,
  color = Palette.muted,
}: {
  text: string
  color?: string
}) {
  return <Text style={[Typeface.label, { color }]}>{text}</Text>
}

export type Tone = 'success' | 'warning' | 'danger' | 'neutral'

/** Each tone's colour and the tint behind it. */
export const toneColors: Record<Tone, { fg: string; tint: string }> = {
  success: { fg: Palette.success, tint: Palette.successTint },
  warning: { fg: Palette.warning, tint: Palette.warningTint },
  danger: { fg: Palette.danger, tint: Palette.dangerTint },
  neutral: { fg: Palette.muted, tint: Palette.subtle },
}

/**
 * A coloured dot and a word, for a federation's status or an operation's state. `center` for a
 * pill in a centred stack: SwiftUI places it by its parent's alignment, React Native by its own.
 */
export function StatusPill({
  text,
  tone,
  center = false,
}: {
  text: string
  tone: Tone
  center?: boolean
}) {
  const { fg, tint } = toneColors[tone]
  return (
    <View
      style={[
        styles.pill,
        { backgroundColor: tint, alignSelf: center ? 'center' : 'flex-start' },
      ]}
    >
      <View style={[styles.dot, { backgroundColor: fg }]} />
      <Text style={[Typeface.geist('Medium', 13), { color: fg }]}>{text}</Text>
    </View>
  )
}

/**
 * Amounts in msat as whole sats with grouping, and a sub-sat remainder only when there is one.
 * BigInt throughout: the SDK hands back a full u64, which a JS number would round above 2^53.
 */
export const SatsFormat = {
  /** "21,500". */
  grouped(msats: bigint): string {
    return group(msats / 1000n)
  },
  /** "+250 msat" when `msats` is not a whole number of sats, otherwise `undefined`. */
  remainder(msats: bigint): string | undefined {
    const rest = msats % 1000n
    return rest === 0n ? undefined : `+${rest} msat`
  },
  /** "1,012 sats", or "1,012 sats +250 msat". */
  text(msats: bigint): string {
    const remainder = SatsFormat.remainder(msats)
    return `${SatsFormat.grouped(msats)} sats${remainder ? ` ${remainder}` : ''}`
  },
  /** For a figure the SDK reports in whole sats, such as an on-chain amount. */
  sats(sats: bigint): string {
    return `${group(sats)} sats`
  },
}

/** Thousands separators by hand: Hermes does not group a BigInt in `toLocaleString`. */
function group(value: bigint): string {
  return value.toString().replace(/\B(?=(\d{3})+(?!\d))/g, ',')
}

/** A balance or payment amount the way the design sets it. */
export function AmountView({
  msats,
  sign = '',
}: {
  msats: bigint
  sign?: string
}) {
  const remainder = SatsFormat.remainder(msats)
  return (
    <View accessible style={{ gap: 4 }}>
      <View style={styles.amountRow}>
        <Text style={[Typeface.amount, { color: Palette.ink }]}>
          {sign + SatsFormat.grouped(msats)}
        </Text>
        <Text style={[Typeface.amountUnit, { color: Palette.muted }]}>
          sats
        </Text>
      </View>
      {remainder && (
        <Text style={[Typeface.footnote, { color: Palette.muted }]}>
          {remainder}
        </Text>
      )}
    </View>
  )
}

/**
 * Said once the SDK has stopped reporting an operation's state. The payment may still complete,
 * so this says only that the screen can no longer tell, and where to look instead.
 */
export function TrackingStopped({ reason }: { reason: string }) {
  return (
    <Text selectable style={[Typeface.footnote, styles.tracking]}>
      Stopped following this payment: {reason}. Activity shows how it stands.
    </Text>
  )
}

/** Why the last action failed, in the design's danger colour. */
export function FailureText({ text }: { text?: string }) {
  if (!text) return null
  return (
    <Text selectable style={[Typeface.footnote, { color: Palette.danger }]}>
      {text}
    </Text>
  )
}

const styles = StyleSheet.create({
  pill: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    paddingHorizontal: 10,
    paddingVertical: 4,
    borderRadius: 999,
  },
  dot: { width: 6, height: 6, borderRadius: 3 },
  amountRow: { flexDirection: 'row', alignItems: 'baseline', gap: 8 },
  tracking: { color: Palette.warning, textAlign: 'center' },
})
