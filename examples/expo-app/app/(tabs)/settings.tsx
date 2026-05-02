import React, { useState } from 'react'
import { View, Text, TextInput, ScrollView } from 'react-native'
import * as Clipboard from 'expo-clipboard'
import { director } from '../../src/wallet'
import { extractErrorMessage } from '../../src/hooks'
import {
  SectionCard,
  SectionTitle,
  Btn,
  Row,
  SuccessBox,
  ErrorBox,
} from '../../src/components'
import s from '../../src/styles'
import type { ParsedInviteCode, ParsedBolt11Invoice } from '@fedimint/core'

const MnemonicManager = () => {
  const [mnemonicState, setMnemonicState] = useState('')
  const [inputMnemonic, setInputMnemonic] = useState('')
  const [activeAction, setActiveAction] = useState<
    'get' | 'set' | 'generate' | null
  >(null)
  const [isLoading, setIsLoading] = useState(false)
  const [message, setMessage] = useState<{
    text: string
    type: 'success' | 'error'
  }>()
  const [showMnemonic, setShowMnemonic] = useState(false)

  const clearMessage = () => setMessage(undefined)

  const handleAction = async (action: 'get' | 'set' | 'generate') => {
    if (activeAction === action) {
      setActiveAction(null)
      return
    }
    setActiveAction(action)
    clearMessage()
    if (action === 'get') await handleGetMnemonic()
    else if (action === 'generate') await handleGenerateMnemonic()
  }

  const handleGenerateMnemonic = async () => {
    setIsLoading(true)
    try {
      const newMnemonic = await director.generateMnemonic()
      setMnemonicState(newMnemonic.join(' '))
      setMessage({ text: 'New mnemonic generated!', type: 'success' })
      setShowMnemonic(true)
    } catch (error) {
      setMessage({ text: extractErrorMessage(error), type: 'error' })
    } finally {
      setIsLoading(false)
    }
  }

  const handleGetMnemonic = async () => {
    setIsLoading(true)
    try {
      const mnemonic = await director.getMnemonic()
      if (mnemonic && mnemonic.length > 0) {
        setMnemonicState(mnemonic.join(' '))
        setMessage({ text: 'Mnemonic retrieved!', type: 'success' })
        setShowMnemonic(true)
      } else {
        setMessage({ text: 'No mnemonic found', type: 'error' })
      }
    } catch (error) {
      setMessage({ text: extractErrorMessage(error), type: 'error' })
    } finally {
      setIsLoading(false)
    }
  }

  const handleSetMnemonic = async () => {
    if (!inputMnemonic.trim()) return
    setIsLoading(true)
    try {
      const words = inputMnemonic.trim().split(/\s+/)
      await director.setMnemonic(words)
      setMessage({ text: 'Mnemonic set successfully!', type: 'success' })
      setInputMnemonic('')
      setMnemonicState(words.join(' '))
      setActiveAction(null)
    } catch (error) {
      setMessage({ text: extractErrorMessage(error), type: 'error' })
    } finally {
      setIsLoading(false)
    }
  }

  const copyToClipboard = async () => {
    try {
      await Clipboard.setStringAsync(mnemonicState)
      setMessage({ text: 'Copied to clipboard!', type: 'success' })
    } catch {
      setMessage({ text: 'Failed to copy', type: 'error' })
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Mnemonic Manager</SectionTitle>

      <Row>
        <Btn
          title="Get"
          onPress={() => handleAction('get')}
          disabled={isLoading}
          active={activeAction === 'get'}
        />
        <Btn
          title="Set"
          onPress={() => handleAction('set')}
          disabled={isLoading}
          active={activeAction === 'set'}
        />
        <Btn
          title="Generate"
          onPress={() => handleAction('generate')}
          disabled={isLoading}
          active={activeAction === 'generate'}
        />
      </Row>

      {activeAction === 'set' && (
        <View style={s.formGroup}>
          <TextInput
            style={s.textArea}
            placeholder="Enter 12 or 24 words separated by spaces"
            placeholderTextColor="#888"
            value={inputMnemonic}
            onChangeText={setInputMnemonic}
            multiline
            numberOfLines={2}
          />
          <Btn
            title={isLoading ? 'Setting...' : 'Set Mnemonic'}
            onPress={handleSetMnemonic}
            disabled={isLoading || !inputMnemonic.trim()}
            primary
          />
        </View>
      )}

      {!!mnemonicState && (
        <View style={s.mnemonicDisplay}>
          <Text style={showMnemonic ? s.mnemonicText : s.mnemonicBlurred}>
            {mnemonicState}
          </Text>
          <Row>
            <Btn
              title={showMnemonic ? 'Hide' : 'Show'}
              onPress={() => setShowMnemonic(!showMnemonic)}
              small
            />
            <Btn
              title="Copy"
              onPress={copyToClipboard}
              disabled={!showMnemonic}
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
  const [inviteCode, setInviteCode] = useState('')
  const [parseResult, setParseResult] = useState<ParsedInviteCode | null>(null)
  const [parseError, setParseError] = useState('')
  const [parsing, setParsing] = useState(false)

  const handleParse = async () => {
    setParseResult(null)
    setParseError('')
    setParsing(true)
    try {
      const result = await director.parseInviteCode(inviteCode)
      setParseResult(result)
    } catch (e) {
      setParseError(e instanceof Error ? e.message : String(e))
    } finally {
      setParsing(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Parse Invite Code</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter invite code..."
        placeholderTextColor="#888"
        value={inviteCode}
        onChangeText={setInviteCode}
      />
      <Btn
        title={parsing ? 'Parsing...' : 'Parse'}
        onPress={handleParse}
        disabled={parsing}
      />
      {parseResult && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Fed Id: <Text style={s.mono}>{parseResult.federation_id}</Text>
          </Text>
          <Text style={s.label}>
            Fed url: <Text style={s.mono}>{parseResult.url}</Text>
          </Text>
        </View>
      )}
      {!!parseError && <ErrorBox>{parseError}</ErrorBox>}
    </SectionCard>
  )
}

const ParseLightningInvoice = () => {
  const [invoiceStr, setInvoiceStr] = useState('')
  const [parseResult, setParseResult] = useState<ParsedBolt11Invoice | null>(
    null,
  )
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
      <Btn
        title={parsing ? 'Parsing...' : 'Parse'}
        onPress={handleParse}
        disabled={parsing}
      />
      {parseResult && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Amount: <Text style={s.value}>{parseResult.amount}</Text> sats
          </Text>
          <Text style={s.label}>
            Expiry: <Text style={s.value}>{parseResult.expiry}</Text>
          </Text>
          <Text style={s.label}>
            Memo: <Text style={s.value}>{parseResult.memo}</Text>
          </Text>
        </View>
      )}
      {!!parseError && <ErrorBox>{parseError}</ErrorBox>}
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
      <ParseLightningInvoice />
    </ScrollView>
  )
}
