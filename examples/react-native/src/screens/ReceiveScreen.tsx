import React, { useState } from 'react'
import { ScrollView, Text, TextInput, TouchableOpacity, View, Linking, Alert } from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import s from '../styles'
import { wallet } from '../wallet'
import { SectionCard, SectionTitle, Row, Btn, SuccessBox, ErrorBox } from '../components/common'

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
      const response = await wallet.lightning.createInvoice(Number(amount), description)
      response && setInvoice(response.invoice)
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setGenerating(false)
    }
  }

  const copyInvoice = () => {
    Clipboard.setString(invoice)
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
      <TouchableOpacity onPress={() => Linking.openURL('https://faucet.mutinynet.com/')}>
        <Text style={s.link}>mutinynet faucet ↗</Text>
      </TouchableOpacity>

      {!!invoice && (
        <View style={s.invoiceBox}>
          <Text style={s.label}>Generated Invoice:</Text>
          <Text style={s.mono} selectable>{invoice}</Text>
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

  return (
    <SectionCard>
      <SectionTitle>Generate Deposit Address</SectionTitle>
      <Btn title={loading ? 'Generating...' : 'Generate'} onPress={handleGenerate} disabled={loading} primary />
      {!!address && <SuccessBox>{address}</SuccessBox>}
      {!!addressError && <ErrorBox>{addressError}</ErrorBox>}
    </SectionCard>
  )
}

const RedeemEcash = () => {
  const [ecashInput, setEcashInput] = useState('')
  const [redeemResult, setRedeemResult] = useState('')
  const [redeemError, setRedeemError] = useState('')

  const handleRedeem = async () => {
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      await wallet.mint.redeemEcash(ecashInput)
      setRedeemResult('Redeemed!')
      setRedeemError('')
    } catch (e) {
      setRedeemError(String(e))
      setRedeemResult('')
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Redeem Ecash</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Long ecash string..."
        placeholderTextColor="#888"
        value={ecashInput}
        onChangeText={setEcashInput}
      />
      <Btn title="Redeem" onPress={handleRedeem} />
      {!!redeemResult && <SuccessBox>{redeemResult}</SuccessBox>}
      {!!redeemError && <ErrorBox>{redeemError}</ErrorBox>}
    </SectionCard>
  )
}

export const ReceiveScreen = () => (
  <ScrollView style={s.container} contentContainerStyle={s.contentContainer} keyboardShouldPersistTaps="handled">
    <Text style={s.header}>Receive</Text>
    <GenerateLightningInvoice />
    <Deposit />
    <RedeemEcash />
  </ScrollView>
)
