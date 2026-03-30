import React, { useState } from 'react'
import { ScrollView, Text, TextInput, View } from 'react-native'
import s from '../styles'
import { wallet, director } from '../wallet'
import { SectionCard, SectionTitle, Row, Btn, SuccessBox, ErrorBox } from '../components/common'
import type { ParsedBolt11Invoice } from '@fedimint/core'

const SendLightning = () => {
  const [lightningInput, setLightningInput] = useState('')
  const [lightningResult, setLightningResult] = useState('')
  const [lightningError, setLightningError] = useState('')

  const handleSubmit = async () => {
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      await wallet.lightning.payInvoice(lightningInput)
      setLightningResult('Paid!')
      setLightningError('')
    } catch (e) {
      setLightningError(String(e))
      setLightningResult('')
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Pay Lightning</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="lnbc..."
        placeholderTextColor="#888"
        value={lightningInput}
        onChangeText={setLightningInput}
      />
      <Btn title="Pay" onPress={handleSubmit} primary />
      {!!lightningResult && <SuccessBox>{lightningResult}</SuccessBox>}
      {!!lightningError && <ErrorBox>{lightningError}</ErrorBox>}
    </SectionCard>
  )
}

const SendOnchain = () => {
  const [address, setAddress] = useState('')
  const [amount, setAmount] = useState('')
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  const [sending, setSending] = useState(false)

  const handleWithdraw = async () => {
    try {
      setSending(true)
      if (!wallet) throw new Error('Wallet unavailable')
      const res = await wallet.wallet.sendOnchain(Number(amount), address)
      res && setResult(res.operation_id)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setSending(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Send Onchain</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter amount"
        placeholderTextColor="#888"
        keyboardType="numeric"
        value={amount}
        onChangeText={setAmount}
      />
      <TextInput
        style={s.input}
        placeholder="Enter onchain address"
        placeholderTextColor="#888"
        value={address}
        onChangeText={setAddress}
      />
      <Btn title={sending ? 'Sending...' : 'Send'} onPress={handleWithdraw} disabled={sending} primary />
      {!!result && <SuccessBox>Onchain Send Successful</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const ParseLightningInvoice = () => {
  const [invoiceStr, setInvoiceStr] = useState('')
  const [parseResult, setParseResult] = useState<ParsedBolt11Invoice | null>(null)
  const [parseError, setParseError] = useState('')
  const [parsing, setParsing] = useState(false)

  const handleParse = async () => {
    setParseResult(null)
    setParseError('')
    setParsing(true)
    try {
      const result = await director.parseBolt11Invoice(invoiceStr)
      setParseResult(result)
    } catch (e) {
      setParseError(e instanceof Error ? e.message : String(e))
    } finally {
      setParsing(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Parse Lightning Invoice</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter invoice..."
        placeholderTextColor="#888"
        value={invoiceStr}
        onChangeText={setInvoiceStr}
      />
      <Btn title={parsing ? 'Parsing...' : 'Parse'} onPress={handleParse} disabled={parsing} />
      {parseResult && (
        <View style={s.resultBox}>
          <Text style={s.label}>Amount: <Text style={s.value}>{parseResult.amount}</Text> sats</Text>
          <Text style={s.label}>Expiry: <Text style={s.value}>{parseResult.expiry}</Text></Text>
          <Text style={s.label}>Memo: <Text style={s.value}>{parseResult.memo}</Text></Text>
        </View>
      )}
      {!!parseError && <ErrorBox>{parseError}</ErrorBox>}
    </SectionCard>
  )
}

export const SendScreen = () => (
  <ScrollView style={s.container} contentContainerStyle={s.contentContainer} keyboardShouldPersistTaps="handled">
    <Text style={s.header}>Send</Text>
    <SendLightning />
    <SendOnchain />
    <ParseLightningInvoice />
  </ScrollView>
)
