import { useCallback, useMemo, useState } from 'react'
import { Pressable, ScrollView, StyleSheet, Text, View } from 'react-native'
import { OperationKind, type ActivityItem } from '@fedimint/react-native'
import { useFocusEffect } from '@react-navigation/native'
import { model, useDemo } from '../../model/DemoModel'
import { Button } from '../../components/Buttons'
import { Caption, FailureText } from '../../components/Labels'
import { Card, CardList, Screen } from '../../components/Surfaces'
import {
  ActivityRow,
  ActivityTime,
  useHistoryFailure,
} from '../../components/Activity'
import { Metrics, Palette, Typeface } from '../../theme/tokens'

const filters = ['All', 'Lightning', 'Ecash', 'Onchain'] as const
type Filter = (typeof filters)[number]

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

  // On every appearance: a list still empty, after a load that failed, is tried again on coming
  // back.
  useFocusEffect(
    useCallback(() => {
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

  const loading = state.working.has('activity')
  const loadFailure = useHistoryFailure(state.failures.activity, loading)

  const failedLoad = (reason: string) => (
    <Card padding={20} style={{ gap: Metrics.gap }}>
      <Text selectable style={[Typeface.callout, { color: Palette.danger }]}>
        Couldn't load the history: {reason}
      </Text>
      <Button
        title="Try again"
        kind="secondary"
        busy={loading}
        disabled={loading}
        onPress={() => model.activity()}
      />
    </Card>
  )

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
        : loadFailure !== undefined && state.activityItems.length === 0
          ? // Not "No activity yet.": nothing loaded, so nothing is known about the history.
            failedLoad(loadFailure)
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
                        accessibilityRole="button"
                        onPress={() => showOperation(item)}
                      >
                        <ActivityRow item={item} />
                      </Pressable>
                    ))}
                  </CardList>
                </View>
              ))}

      {/* A refresh or a further page that failed, under the rows already loaded. */}
      {state.hasFederation && state.activityItems.length > 0 && (
        <FailureText text={loadFailure} />
      )}

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
    borderWidth: Metrics.borderWidth,
    borderColor: Palette.border,
  },
})
