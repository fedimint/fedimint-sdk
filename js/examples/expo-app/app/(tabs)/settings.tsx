import React, { useState } from 'react'
import { View, Text, TextInput, ScrollView, Alert } from 'react-native'
import * as Clipboard from 'expo-clipboard'
import { InviteCode, type LnQuoteLike } from '@fedimint/react-native'
import {
  current,
  errorMessage,
  federation,
  msatToSat,
  open,
  reset,
} from '../../src/sdk'
import {
  SectionCard,
  SectionTitle,
  Btn,
  Row,
  SuccessBox,
  ErrorBox,
} from '../../src/components'
import s from '../../src/styles'

const MnemonicManager = () => {
  const [words, setWords] = useState('')
  const [visible, setVisible] = useState(false)
  const [restoreInput, setRestoreInput] = useState('')
  const [showRestore, setShowRestore] = useState(false)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{
    text: string
    type: 'success' | 'error'
  }>()

  const handleExport = async () => {
    setBusy(true)
    setMessage(undefined)
    try {
      const session = current()
      if (!session) throw new Error('Wallet not ready')
      const list = session.sdk.exportMnemonic().words()
      setWords(list.join(' '))
      setVisible(true)
      setMessage({ text: 'Mnemonic retrieved!', type: 'success' })
    } catch (error) {
      setMessage({ text: errorMessage(error), type: 'error' })
    } finally {
      setBusy(false)
    }
  }

  const restoreWallet = async () => {
    const restoreWords = restoreInput.trim().split(/\s+/).filter(Boolean)
    if (restoreWords.length === 0) return
    setBusy(true)
    setMessage(undefined)
    try {
      await reset()
      await open(restoreWords)
      setRestoreInput('')
      setShowRestore(false)
      setMessage({ text: 'Wallet restored!', type: 'success' })
    } catch (error) {
      setMessage({ text: errorMessage(error), type: 'error' })
    } finally {
      setBusy(false)
    }
  }

  const handleRestore = () => {
    const restoreWords = restoreInput.trim().split(/\s+/).filter(Boolean)
    if (restoreWords.length === 0) return
    Alert.alert(
      'Restore wallet?',
      'This deletes the wallet data on this device and replaces it with the seed you entered.',
      [
        { text: 'Cancel', style: 'cancel' },
        { text: 'Restore', style: 'destructive', onPress: restoreWallet },
      ],
    )
  }

  const copyToClipboard = async () => {
    await Clipboard.setStringAsync(words)
    setMessage({ text: 'Copied to clipboard!', type: 'success' })
  }

  return (
    <SectionCard>
      <SectionTitle>Mnemonic Manager</SectionTitle>

      <Row>
        <Btn
          title={busy ? 'Working...' : 'Export'}
          onPress={handleExport}
          disabled={busy}
        />
        <Btn
          title="Restore"
          onPress={() => setShowRestore(!showRestore)}
          disabled={busy}
          active={showRestore}
        />
      </Row>

      {showRestore && (
        <View style={s.formGroup}>
          <TextInput
            style={s.textArea}
            placeholder="Enter 12 or 24 words separated by spaces"
            placeholderTextColor="#888"
            value={restoreInput}
            onChangeText={setRestoreInput}
            multiline
            numberOfLines={2}
          />
          <Btn
            title={busy ? 'Restoring...' : 'Restore Wallet'}
            onPress={handleRestore}
            disabled={busy || !restoreInput.trim()}
            primary
          />
        </View>
      )}

      {!!words && (
        <View style={s.mnemonicDisplay}>
          <Text style={visible ? s.mnemonicText : s.mnemonicBlurred}>
            {words}
          </Text>
          <Row>
            <Btn
              title={visible ? 'Hide' : 'Show'}
              onPress={() => setVisible(!visible)}
              small
            />
            <Btn
              title="Copy"
              onPress={copyToClipboard}
              disabled={!visible}
              small
            />
          </Row>
        </View>
      )}

      {message &&
        (message.type === 'success' ? (
          <SuccessBox>{message.text}</SuccessBox>
        ) : (
          <ErrorBox>{message.text}</ErrorBox>
        ))}
    </SectionCard>
  )
}

const InviteCodeParser = () => {
  const [code, setCode] = useState('')
  const [result, setResult] = useState<{
    federationId: string
    display: string
  }>()
  const [error, setError] = useState('')

  const handleParse = () => {
    setResult(undefined)
    setError('')
    try {
      const invite = InviteCode.parse(code.trim())
      setResult({
        federationId: invite.federationId(),
        display: invite.display(),
      })
    } catch (error) {
      setError(errorMessage(error))
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Parse Invite Code</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter invite code..."
        placeholderTextColor="#888"
        value={code}
        onChangeText={setCode}
      />
      <Btn title="Parse" onPress={handleParse} disabled={!code.trim()} />
      {result && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Fed Id: <Text style={s.mono}>{result.federationId}</Text>
          </Text>
          <Text style={s.label}>
            Display: <Text style={s.mono}>{result.display}</Text>
          </Text>
        </View>
      )}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const QuoteLightningInvoice = () => {
  const [invoice, setInvoice] = useState('')
  const [quote, setQuote] = useState<LnQuoteLike>()
  const [error, setError] = useState('')
  const [quoting, setQuoting] = useState(false)

  const handleQuote = async () => {
    setQuote(undefined)
    setError('')
    setQuoting(true)
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const lightning = fed.lightning()
      if (!lightning) {
        throw new Error('Lightning is not supported by this federation')
      }
      setQuote(await lightning.quote(invoice.trim()))
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setQuoting(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Quote Lightning Invoice</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter invoice..."
        placeholderTextColor="#888"
        value={invoice}
        onChangeText={setInvoice}
      />
      <Btn
        title={quoting ? 'Quoting...' : 'Quote'}
        onPress={handleQuote}
        disabled={quoting || !invoice.trim()}
      />
      {quote && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Amount:{' '}
            <Text style={s.value}>{msatToSat(quote.invoiceAmount())} sats</Text>
          </Text>
          <Text style={s.label}>
            Fee: <Text style={s.value}>{msatToSat(quote.fee())} sats</Text>
          </Text>
          <Text style={s.label}>
            Total: <Text style={s.value}>{msatToSat(quote.total())} sats</Text>
          </Text>
          <Text style={s.label}>
            Expires:{' '}
            <Text style={s.value}>
              {new Date(Number(quote.expiresAt())).toLocaleString()}
            </Text>
          </Text>
        </View>
      )}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

export default function SettingsScreen() {
  return (
    <ScrollView
      style={s.container}
      contentContainerStyle={s.contentContainer}
      keyboardShouldPersistTaps="handled"
    >
      <MnemonicManager />
      <InviteCodeParser />
      <QuoteLightningInvoice />
    </ScrollView>
  )
}
