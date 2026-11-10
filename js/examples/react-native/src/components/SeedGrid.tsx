import React from 'react'
import { StyleSheet, Text, View } from 'react-native'
import { Metrics, Palette, Typeface } from '../theme/tokens'

/**
 * A recovery phrase as the design lays it out: two columns of numbered cells, 01 | 02 on the
 * first row, ruled like a table. Backup fills the cells with the words; Restore with fields.
 */
export function SeedGrid({
  count,
  highlighted,
  cell,
}: {
  count: number
  /** The cell being typed into, tinted the way the design marks focus. */
  highlighted?: number
  cell: (index: number) => React.ReactNode
}) {
  const rows: number[] = []
  for (let left = 0; left < count; left += 2) rows.push(left)

  const slot = (index: number) => (
    <View style={[styles.slot, index === highlighted && styles.highlighted]}>
      <Text style={[Typeface.mono('Regular', 13), { color: Palette.muted }]}>
        {String(index + 1).padStart(2, '0')}
      </Text>
      <View style={{ flex: 1 }}>{cell(index)}</View>
    </View>
  )

  return (
    <View style={styles.grid}>
      {rows.map((left) => (
        <React.Fragment key={left}>
          {left > 0 && <View style={styles.hRule} />}
          <View style={styles.row}>
            {slot(left)}
            <View style={styles.vRule} />
            {left + 1 < count ? slot(left + 1) : <View style={{ flex: 1 }} />}
          </View>
        </React.Fragment>
      ))}
    </View>
  )
}

const styles = StyleSheet.create({
  grid: {
    backgroundColor: Palette.surface,
    borderRadius: Metrics.cardRadius,
    borderWidth: Metrics.borderWidth,
    borderColor: Palette.border,
    overflow: 'hidden',
  },
  row: { flexDirection: 'row', height: Metrics.seedWordHeight },
  slot: {
    flex: 1,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    paddingHorizontal: 16,
  },
  highlighted: { backgroundColor: 'rgba(24,24,132,0.08)' },
  hRule: { height: Metrics.borderWidth, backgroundColor: Palette.border },
  vRule: { width: Metrics.borderWidth, backgroundColor: Palette.border },
})
