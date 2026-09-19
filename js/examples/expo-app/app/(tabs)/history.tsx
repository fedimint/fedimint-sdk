import React, { useCallback, useState } from 'react'
import { View, Text, FlatList, RefreshControl } from 'react-native'
import {
  ActivityStatus,
  Direction,
  OperationKind,
  type ActivityItem,
} from '@fedimint/react-native'
import { errorMessage, federation, msatToSat } from '../../src/sdk'
import { SectionCard, SectionTitle } from '../../src/components'
import s from '../../src/styles'

export default function HistoryScreen() {
  const [items, setItems] = useState<ActivityItem[]>([])
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState('')

  const fetchActivity = useCallback(async () => {
    setRefreshing(true)
    setError('')
    try {
      const fed = federation()
      if (!fed) {
        setItems([])
        return
      }
      const page = await fed.activity(undefined, 50)
      setItems(page.items)
    } catch (e) {
      setError(errorMessage(e))
    } finally {
      setRefreshing(false)
    }
  }, [])

  const renderItem = ({ item }: { item: ActivityItem }) => {
    const sign =
      item.direction === Direction.Incoming
        ? '+'
        : item.direction === Direction.Outgoing
          ? '-'
          : ''
    const directionStyle =
      item.direction === Direction.Incoming
        ? s.txIncoming
        : item.direction === Direction.Outgoing
          ? s.txOutgoing
          : s.value

    return (
      <View style={s.txItem}>
        <Text style={[s.txType, directionStyle]}>
          {OperationKind[item.kind]}
        </Text>
        <Text style={s.txAmount}>
          {sign}
          {msatToSat(item.amount ?? 0n)} sats
        </Text>
        <Text style={s.txDate}>
          {new Date(Number(item.time)).toLocaleString()}
        </Text>
        <Text style={s.txDate}>{ActivityStatus[item.status]}</Text>
      </View>
    )
  }

  return (
    <View style={s.container}>
      <FlatList
        data={items}
        keyExtractor={(item) => item.operationId}
        renderItem={renderItem}
        contentContainerStyle={[
          s.contentContainer,
          items.length === 0 && { flex: 1 },
        ]}
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={fetchActivity}
            tintColor="#60a5fa"
            colors={['#60a5fa']}
          />
        }
        ListHeaderComponent={
          <SectionCard>
            <SectionTitle>Transaction History</SectionTitle>
            <Text style={s.label}>Pull down to refresh</Text>
            {!!error && <Text style={s.errorText}>{error}</Text>}
          </SectionCard>
        }
        ListEmptyComponent={
          <Text style={s.emptyText}>
            No transactions yet. Join a federation and make some payments!
          </Text>
        }
      />
    </View>
  )
}
