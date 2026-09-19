import React, { useState } from 'react'
import { View, Text, TextInput, ScrollView, Alert, Linking } from 'react-native'
import * as Clipboard from 'expo-clipboard'
import { LnReceiveState_Tags } from '@fedimint/react-native'
import { errorMessage, federation, msatToSat, satToMsat } from '../src/sdk'
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
  const [status, setStatus] = useState('')
  const [error, setError] = useState('')
  const [generating, setGenerating] = useState(false)

  const handleGenerate = async () => {
    setInvoice('')
    setStatus('')
    setError('')
    setGenerating(true)
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const lightning = fed.lightning()
      if (!lightning) {
        throw new Error('Lightning is not supported by this federation')
      }
      const handle = await lightning.receive(satToMsat(amount), description)
      setInvoice(handle.invoice)
      setStatus('Waiting for payment')
      handle.operation
        .awaitFinal()
        .then((state) => setStatus(LnReceiveState_Tags[state.tag]))
        .catch((finalError) => setStatus(errorMessage(finalError)))
    } catch (error) {
      setError(errorMessage(error))
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
      <Text style={s.label}>Amount (sats):</Text>
      <TextInput
        style={s.input}
        placeholder="Enter amount in sats"
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
        onPress={handleGenerate}
        disabled={generating || !amount.trim()}
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
          {!!status && <Text style={s.value}>{status}</Text>}
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
    setAddressError('')
    setLoading(true)
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const onchain = fed.onchain()
      if (!onchain) {
        throw new Error('On-chain is not supported by this federation')
      }
      const handle = await onchain.receive()
      setAddress(handle.address)
    } catch (error) {
      setAddressError(errorMessage(error))
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
