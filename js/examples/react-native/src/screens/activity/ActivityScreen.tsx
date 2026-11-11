import React, { useMemo, useState } from 'react'
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native'
import { OperationKind, type ActivityItem } from '@fedimint/react-native'
import { useFocusEffect } from '@react-navigation/native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { Caption } from '../../components/Labels'
import { Card, CardList, Screen } from '../../components/Surfaces'
import { ActivityRow, ActivityTime } from '../../components/Activity'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

type Filter = 'All' | 'Lightning' | 'Ecash' | 'Onchain'
const filters: Filter[] = ['All', 'Lightning', 'Ecash', 'Onchain']

function includes(filter: Filter, kind: OperationKind): boolean {
  switch (filter) {
    case 'All':
      return true
    case 'Lightning':
      return kind === OperationKind.LnSend || kind === OperationKind.LnReceive
    case 'Ecash':
      return (
        kind === OperationKind.EcashSend || kind === OperationKind.EcashReceive
      )
    case 'Onchain':
      return (
        kind === OperationKind.OnchainSend ||
        kind === OperationKind.OnchainReceive
      )
  }
}

/** 08 Activity: the federation's local history, a page at a time. */
export function ActivityScreen({
  showOperation,
}: {
  showOperation: (item: ActivityItem) => void
}) {
  const state = useDemo()
  const [filter, setFilter] = useState<Filter>('All')

  // On every appearance, as iOS's `.task`: a list still empty, after a load that failed, is
  // tried again on coming back.
  useFocusEffect(
    React.useCallback(() => {
      const now = model.getState()
      if (now.hasFederation && now.activityItems.length === 0) model.activity()
    }, []),
  )

  /**
   * The loaded rows that pass the filter, grouped by local day, newest first. The filter only
   * narrows what is loaded; it never fetches.
   */
  const days = useMemo(() => {
    const groups = new Map<number, ActivityItem[]>()
    for (const item of state.activityItems) {
      if (!includes(filter, item.kind)) continue
      const date = ActivityTime.date(item.time)
      const day = new Date(
        date.getFullYear(),
        date.getMonth(),
        date.getDate(),
      ).getTime()
      groups.set(day, [...(groups.get(day) ?? []), item])
    }
    return [...groups.keys()]
      .sort((a, b) => b - a)
      .map((day) => ({
        title: ActivityTime.day(new Date(day)),
        items: groups.get(day)!,
      }))
  }, [state.activityItems, filter])

  const empty = (text: string) => (
    <Card padding={20}>
      <Text style={[Typeface.callout, { color: Palette.muted }]}>{text}</Text>
    </Card>
  )

  return (
    <Screen onRefresh={() => model.activity()}>
      <ScrollView
        horizontal
        showsHorizontalScrollIndicator={false}
        contentContainerStyle={styles.filters}
      >
        {filters.map((option) => {
          const selected = option === filter
          return (
            <Pressable
              key={option}
              accessibilityRole="button"
              accessibilityState={{ selected }}
              onPress={() => setFilter(option)}
              style={[
                styles.chip,
                selected ? styles.chipSelected : styles.chipIdle,
              ]}
            >
              <Text
                style={[
                  Typeface.geist('Medium', 15),
                  { color: selected ? '#FFFFFF' : Palette.ink },
                ]}
              >
                {option}
              </Text>
            </Pressable>
          )
        })}
      </ScrollView>

      {!state.hasFederation
        ? empty('Join a federation to see its history.')
        : days.length === 0
          ? empty(
              filter === 'All'
                ? 'No activity yet.'
                : `No ${filter.toLowerCase()} activity loaded.`,
            )
          : days.map((day) => (
              <View key={day.title} style={{ gap: Metrics.gap }}>
                <Caption text={day.title} />
                <CardList>
                  {day.items.map((item) => (
                    <Pressable
                      key={item.operationId}
                      onPress={() => showOperation(item)}
                    >
                      <ActivityRow item={item} />
                    </Pressable>
                  ))}
                </CardList>
              </View>
            ))}

      {state.activityNext !== undefined && (
        <Button
          title="Load more"
          kind="secondary"
          onPress={() => model.moreActivity()}
        />
      )}
    </Screen>
  )
}

const styles = StyleSheet.create({
  filters: { gap: 8 },
  chip: {
    paddingHorizontal: 16,
    height: 36,
    borderRadius: 18,
    justifyContent: 'center',
  },
  chipSelected: { backgroundColor: Palette.navy },
  chipIdle: {
    backgroundColor: Palette.surface,
    borderWidth: 1,
    borderColor: Palette.border,
  },
})
