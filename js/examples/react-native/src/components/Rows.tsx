import React from 'react'
import { StyleSheet, Text, View } from 'react-native'
import { Icon, type IconName } from './Icon'
import { Metrics, Palette, Typeface } from '../theme/tokens'

/** The rounded square an icon sits in at the start of a row. */
export function IconTile({ icon }: { icon: IconName }) {
  return (
    <View style={styles.tile}>
      <Icon name={icon} color={Palette.ink} />
    </View>
  )
}

/** A row in a card list: icon, title over subtitle, and an optional figure on the trailing edge. */
export function ListRow({
  icon,
  title,
  subtitle,
  trailing,
}: {
  icon: IconName
  title: string
  subtitle?: string
  trailing?: React.ReactNode
}) {
  return (
    <View style={styles.row} accessible>
      <IconTile icon={icon} />
      <View style={styles.text}>
        <Text style={[Typeface.headline, { color: Palette.ink }]}>{title}</Text>
        {subtitle && (
          <Text style={[Typeface.footnote, { color: Palette.muted }]}>
            {subtitle}
          </Text>
        )}
      </View>
      {trailing}
    </View>
  )
}

/** A label and its value on one line: the review and detail screens' rows. */
export function ReviewRow({
  label,
  value,
  monospaced = false,
  emphasised = false,
  trailing,
}: {
  label: string
  value: string
  monospaced?: boolean
  emphasised?: boolean
  trailing?: React.ReactNode
}) {
  const valueFont = emphasised
    ? Typeface.geist('SemiBold', 16)
    : monospaced
      ? Typeface.figure
      : Typeface.body
  return (
    <View style={styles.review}>
      <Text style={[Typeface.body, { color: Palette.muted }]}>{label}</Text>
      <View style={styles.reviewValue}>
        <Text selectable style={[valueFont, styles.reviewText]}>
          {value}
        </Text>
        {trailing}
      </View>
    </View>
  )
}

const styles = StyleSheet.create({
  tile: {
    width: 40,
    height: 40,
    borderRadius: 12,
    backgroundColor: Palette.subtle,
    alignItems: 'center',
    justifyContent: 'center',
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: Metrics.gap,
    paddingHorizontal: 16,
    minHeight: Metrics.rowHeight,
  },
  text: { flex: 1, gap: 2 },
  review: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: Metrics.gap,
    paddingHorizontal: 20,
    paddingVertical: 12,
    minHeight: 52,
  },
  reviewValue: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    flexShrink: 1,
  },
  reviewText: { color: Palette.ink, textAlign: 'right', flexShrink: 1 },
})
