import React, { useState } from 'react'
import { View, Text, TextInput, ScrollView, Alert, Linking } from 'react-native'
import * as Clipboard from 'expo-clipboard'
import { wallet } from '../src/wallet'
import {
  SectionCard,
  SectionTitle,
  Btn,
  SuccessBox,
  ErrorBox,
} from '../src/components'
import s from '../src/styles'

const GenerateLightningInvoice = () => {
  const [amount, setAmount] = useState('')
  const [description, setDescription] = useState('')
  const [invoice, setInvoice] = useState('')
  const [error, setError] = useState('')
  const [generating, setGenerating] = useState(false)

  const handleSubmit = async () => {
    setInvoice('')
    setError('')
    setGenerating(true)
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const response = await wallet.lightning.createInvoice(
        Number(amount),
        description,
      )
      response && setInvoice(response.invoice)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setGenerating(false)
    }
  }

  const copyInvoice = async () => {
    await Clipboard.setStringAsync(invoice)
    Alert.alert('Copied', 'Invoice copied to clipboard')
  }

  return (
    <SectionCard>
      <SectionTitle>Generate Lightning Invoice</SectionTitle>
      <Text style={s.label}>Amount (msats):</Text>
      <TextInput
        style={s.input}
        placeholder="Enter amount in msats"
        placeholderTextColor="#888"
        keyboardType="numeric"
        value={amount}
        onChangeText={setAmount}
      />
      <Text style={s.label}>Description:</Text>
      <TextInput
        style={s.input}
        placeholder="Enter description"
        placeholderTextColor="#888"
        value={description}
        onChangeText={setDescription}
      />
      <Btn
        title={generating ? 'Generating...' : 'Generate Invoice'}
        onPress={handleSubmit}
        disabled={generating}
        primary
      />
      <Text
        style={s.link}
        onPress={() => Linking.openURL('https://faucet.mutinynet.com/')}
      >
        mutinynet faucet ↗
      </Text>

      {!!invoice && (
        <View style={s.invoiceBox}>
          <Text style={s.label}>Generated Invoice:</Text>
          <Text style={s.mono} selectable>
            {invoice}
          </Text>
          <Btn title="Copy" onPress={copyInvoice} small />
        </View>
      )}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const Deposit = () => {
  const [address, setAddress] = useState('')
  const [addressError, setAddressError] = useState('')
  const [loading, setLoading] = useState(false)

  const handleGenerate = async () => {
    setLoading(true)
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const result = await wallet.wallet.generateAddress()
      result && setAddress(result.deposit_address)
    } catch (e) {
      setAddressError(e instanceof Error ? e.message : String(e))
    } finally {
      setLoading(false)
    }
  }

  const copyAddress = async () => {
    await Clipboard.setStringAsync(address)
    Alert.alert('Copied', 'Address copied to clipboard')
  }

  return (
    <SectionCard>
      <SectionTitle>Generate Deposit Address</SectionTitle>
      <Btn
        title={loading ? 'Generating...' : 'Generate'}
        onPress={handleGenerate}
        disabled={loading}
        primary
      />
      {!!address && (
        <View style={s.resultBox}>
          <Text style={s.label}>Deposit Address:</Text>
          <Text style={s.mono} selectable>
            {address}
          </Text>
          <Btn title="Copy" onPress={copyAddress} small />
        </View>
      )}
      {!!addressError && <ErrorBox>{addressError}</ErrorBox>}
    </SectionCard>
  )
}

export default function ReceiveScreen() {
  return (
    <ScrollView
      style={s.container}
      contentContainerStyle={s.contentContainer}
      keyboardShouldPersistTaps="handled"
    >
      <GenerateLightningInvoice />
      <Deposit />
    </ScrollView>
  )
}
