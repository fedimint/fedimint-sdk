import React, { useState } from 'react'
import { Text, TextInput, ScrollView } from 'react-native'
import { wallet } from '../src/wallet'
import {
  SectionCard,
  SectionTitle,
  Btn,
  SuccessBox,
  ErrorBox,
} from '../src/components'
import s from '../src/styles'

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
      <Text style={s.label}>Amount (sats):</Text>
      <TextInput
        style={s.input}
        placeholder="Enter amount"
        placeholderTextColor="#888"
        keyboardType="numeric"
        value={amount}
        onChangeText={setAmount}
      />
      <Text style={s.label}>Address:</Text>
      <TextInput
        style={s.input}
        placeholder="Enter onchain address"
        placeholderTextColor="#888"
        value={address}
        onChangeText={setAddress}
      />
      <Btn
        title={sending ? 'Sending...' : 'Send'}
        onPress={handleWithdraw}
        disabled={sending}
        primary
      />
      {!!result && <SuccessBox>Onchain Send Successful</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
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

export default function SendScreen() {
  return (
    <ScrollView
      style={s.container}
      contentContainerStyle={s.contentContainer}
      keyboardShouldPersistTaps="handled"
    >
      <SendLightning />
      <SendOnchain />
      <RedeemEcash />
    </ScrollView>
  )
}
