import React, { useEffect, useState, useCallback } from 'react'
import { View, Text, ScrollView, RefreshControl } from 'react-native'
import { useFocusEffect } from '@react-navigation/native'
import s from '../styles'
import { SectionCard, SectionTitle, Row } from '../components/common'
import { wallet } from '../wallet'

export const HistoryScreen = () => {
  const [history, setHistory] = useState<any[]>([])
  const [loading, setLoading] = useState(true)

  const loadHistory = async () => {
    try {
      setLoading(true)
      if (wallet && wallet.isOpen()) {
        const txs = await wallet.federation.listTransactions()
        // Sort descending (newest first)
        const sorted = (txs || []).sort((a: any, b: any) => b.timestamp - a.timestamp)
        setHistory(sorted)
      }
    } catch (e) {
      console.error('Failed to load history', e)
    } finally {
      setLoading(false)
    }
  }

  // Refresh history whenever the tab comes into focus
  useFocusEffect(
    useCallback(() => {
      loadHistory()
    }, [])
  )

  const renderTxType = (type: string) => {
    switch(type) {
      case 'send': return 'Sent Lightning'
      case 'receive': return 'Received Lightning'
      case 'deposit': return 'On-chain Deposit'
      case 'withdraw': return 'On-chain Withdrawal'
      case 'spend_oob': return 'Ecash Spent'
      case 'reissue': return 'Ecash Reissued'
      default: return type
    }
  }

  return (
    <ScrollView 
      style={s.container} 
      contentContainerStyle={s.contentContainer}
      refreshControl={<RefreshControl refreshing={loading} onRefresh={loadHistory} tintColor="#4fd1c5" />}
    >
      <Text style={s.header}>History</Text>
      
      {history.length === 0 && !loading && (
        <SectionCard>
          <Text style={s.italic}>No transactions found.</Text>
        </SectionCard>
      )}

      {history.map((tx: any, idx: number) => (
        <SectionCard key={tx.operationId || idx}>
          <Row>
            <Text style={s.sectionTitle}>{renderTxType(tx.type)}</Text>
          </Row>
          {!!tx.amountMsats && (
            <Row>
              <Text style={s.label}>Amount:</Text>
              <Text style={s.value}>{Math.round(tx.amountMsats / 1000)} sats</Text>
            </Row>
          )}
          {tx.fee !== undefined && (
            <Row>
              <Text style={s.label}>Fee:</Text>
              <Text style={s.value}>{tx.fee} sats</Text>
            </Row>
          )}
          <Row>
            <Text style={s.label}>Status:</Text>
            <Text style={s.value}>{tx.outcome || 'Pending'}</Text>
          </Row>
          {tx.txId && (
            <Row>
              <Text style={s.label}>TxID:</Text>
              <Text style={s.mono}>{String(tx.txId).substring(0, 16)}...</Text>
            </Row>
          )}
          <View style={{marginTop: 8}}>
            <Text style={s.italic}>{new Date(tx.timestamp).toLocaleString()}</Text>
          </View>
        </SectionCard>
      ))}
    </ScrollView>
  )
}
