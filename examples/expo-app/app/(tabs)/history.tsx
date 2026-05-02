import React, { useCallback, useState } from 'react'
import { View, Text, FlatList, RefreshControl } from 'react-native'
import { wallet, director } from '../../src/wallet'
import { SectionCard, SectionTitle } from '../../src/components'
import s from '../../src/styles'
import type {
  Transactions,
  LightningTransaction,
  EcashTransaction,
  WalletTransaction,
} from '@fedimint/core'

type DisplayTx = {
  operationId: string
  label: string
  amountMsats: number
  incoming: boolean
  timestamp: number
}

async function lnAmountMsats(invoice: string): Promise<number> {
  try {
    const parsed = await director.parseBolt11Invoice(invoice)
    // parseBolt11Invoice returns amount in sats, convert to msats
    return (parsed?.amount ?? 0) * 1000
  } catch {
    return 0
  }
}

function txToDisplay(tx: Transactions): Omit<DisplayTx, 'amountMsats'> & {
  amountMsatsOrPromise: number | Promise<number>
} {
  const base = { operationId: tx.operationId, timestamp: tx.timestamp }
  if (tx.kind === 'ln') {
    const lnTx = tx as LightningTransaction
    const incoming = lnTx.type === 'receive'
    return {
      ...base,
      label: incoming ? 'ln_receive' : 'ln_pay',
      incoming,
      amountMsatsOrPromise: lnAmountMsats(lnTx.invoice),
    }
  }
  if (tx.kind === 'mint') {
    const mintTx = tx as EcashTransaction
    const incoming = mintTx.type === 'reissue'
    return {
      ...base,
      label: incoming ? 'mint_reissue' : 'mint_spend',
      incoming,
      amountMsatsOrPromise: mintTx.amountMsats,
    }
  }
  const walletTx = tx as WalletTransaction
  const incoming = walletTx.type === 'deposit'
  return {
    ...base,
    label: incoming ? 'wallet_deposit' : 'wallet_withdraw',
    incoming,
    amountMsatsOrPromise: walletTx.amountMsats,
  }
}

export default function HistoryScreen() {
  const [transactions, setTransactions] = useState<DisplayTx[]>([])
  const [refreshing, setRefreshing] = useState(false)
  const [error, setError] = useState('')

  const fetchTransactions = useCallback(async () => {
    setRefreshing(true)
    setError('')
    try {
      if (!wallet || !wallet.isOpen()) {
        setTransactions([])
        return
      }
      const txs = (await wallet.federation.listTransactions()) ?? []
      const mapped = txs.map(txToDisplay)
      const resolved: DisplayTx[] = await Promise.all(
        mapped.map(async ({ amountMsatsOrPromise, ...rest }) => ({
          ...rest,
          amountMsats: await amountMsatsOrPromise,
        })),
      )
      setTransactions(resolved)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setRefreshing(false)
    }
  }, [])

  const renderItem = ({ item }: { item: DisplayTx }) => {
    const sign = item.incoming ? '+' : '-'

    return (
      <View style={s.txItem}>
        <Text style={[s.txType, item.incoming ? s.txIncoming : s.txOutgoing]}>
          {item.label}
        </Text>
        <Text style={s.txAmount}>
          {sign}
          {item.amountMsats} msats
        </Text>
        {item.timestamp > 0 && (
          <Text style={s.txDate}>
            {new Date(item.timestamp).toLocaleString()}
          </Text>
        )}
      </View>
    )
  }

  return (
    <View style={s.container}>
      <FlatList
        data={transactions}
        keyExtractor={(item) => item.operationId}
        renderItem={renderItem}
        contentContainerStyle={[
          s.contentContainer,
          transactions.length === 0 && { flex: 1 },
        ]}
        refreshControl={
          <RefreshControl
            refreshing={refreshing}
            onRefresh={fetchTransactions}
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
