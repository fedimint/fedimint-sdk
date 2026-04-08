import React, { useState } from 'react'
import { ScrollView, Text, TextInput, View } from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import s from '../styles'
import { wallet, director } from '../wallet'
import { SectionCard, SectionTitle, Row, Btn, SuccessBox, ErrorBox } from '../components/common'
import { extractErrorMessage, TESTNET_FEDERATION_CODE } from '../utils'
import { useIsOpen } from '../hooks'
import type { ParsedInviteCode, PreviewFederation } from '@fedimint/core'

const MnemonicManager = () => {
  const [mnemonicState, setMnemonicState] = useState('')
  const [inputMnemonic, setInputMnemonic] = useState('')
  const [activeAction, setActiveAction] = useState<'get' | 'set' | 'generate' | null>(null)
  const [isLoading, setIsLoading] = useState(false)
  const [message, setMessage] = useState<{ text: string; type: 'success' | 'error' }>()
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

  const copyToClipboard = () => {
    try {
      Clipboard.setString(mnemonicState)
      setMessage({ text: 'Copied to clipboard!', type: 'success' })
    } catch {
      setMessage({ text: 'Failed to copy', type: 'error' })
    }
  }

  return (
    <SectionCard>
      <SectionTitle>🔑 Mnemonic Manager</SectionTitle>

      <Row>
        <Btn title="Get" onPress={() => handleAction('get')} disabled={isLoading} active={activeAction === 'get'} />
        <Btn title="Set" onPress={() => handleAction('set')} disabled={isLoading} active={activeAction === 'set'} />
        <Btn title="Generate" onPress={() => handleAction('generate')} disabled={isLoading} active={activeAction === 'generate'} />
      </Row>

      {activeAction === 'set' && (
        <View style={s.formGroup}>
          <TextInput
            style={s.textArea}
            placeholder="Enter 12 or 24 words separated by spaces"
            placeholderTextColor="#a0aec0"
            value={inputMnemonic}
            onChangeText={setInputMnemonic}
            multiline
            numberOfLines={2}
          />
          <Btn title={isLoading ? 'Setting...' : 'Set Mnemonic'} onPress={handleSetMnemonic} disabled={isLoading || !inputMnemonic.trim()} primary />
        </View>
      )}

      {!!mnemonicState && (
        <View style={s.mnemonicDisplay}>
          <Text style={showMnemonic ? s.mnemonicText : s.mnemonicBlurred}>{mnemonicState}</Text>
          <Row>
            <Btn title={showMnemonic ? '👁️' : '👁️‍🗨️'} onPress={() => setShowMnemonic(!showMnemonic)} small />
            <Btn title="📋" onPress={copyToClipboard} disabled={!showMnemonic} small />
          </Row>
        </View>
      )}

      {message && (message.type === 'success' ? <SuccessBox>{message.text}</SuccessBox> : <ErrorBox>{message.text}</ErrorBox>)}
    </SectionCard>
  )
}

const JoinFederation = ({ open, checkIsOpen }: { open: boolean; checkIsOpen: () => void }) => {
  const [inviteCode, setInviteCode] = useState(TESTNET_FEDERATION_CODE)
  const [previewData, setPreviewData] = useState<PreviewFederation | null>(null)
  const [previewing, setPreviewing] = useState(false)
  const [joinResult, setJoinResult] = useState<string | null>(null)
  const [joinError, setJoinError] = useState('')
  const [joining, setJoining] = useState(false)

  const previewFederationHandler = async () => {
    if (!inviteCode.trim()) return
    setPreviewing(true)
    setJoinError('')
    try {
      const data = await director.previewFederation(inviteCode)
      setPreviewData(data)
    } catch (error) {
      setJoinError(error instanceof Error ? error.message : String(error))
      setPreviewData(null)
    } finally {
      setPreviewing(false)
    }
  }

  const joinFederation = async () => {
    checkIsOpen()
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      setJoining(true)
      await wallet.joinFederation(inviteCode)
      await wallet.open()
      setJoinResult('Joined!')
      setJoinError('')
    } catch (e: any) {
      setJoinError(typeof e === 'object' ? e.toString() : (e as string))
      setJoinResult('')
    } finally {
      setJoining(false)
      checkIsOpen()
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Join Federation</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Invite Code..."
        placeholderTextColor="#a0aec0"
        value={inviteCode}
        onChangeText={(text) => {
          setInviteCode(text)
          setPreviewData(null)
        }}
        editable={!open}
      />
      <Row>
        <Btn title={previewing ? 'Previewing...' : 'Preview'} onPress={previewFederationHandler} disabled={previewing || !inviteCode.trim() || open} />
        <Btn title={joining ? 'Joining...' : 'Join'} onPress={joinFederation} disabled={open || joining} primary />
      </Row>

      {previewData && (
        <View style={s.previewCard}>
          <Text style={s.previewTitle}>Federation Preview</Text>
          <Text style={s.label}>Federation ID: <Text style={s.mono}>{previewData.federation_id}</Text></Text>
          <Text style={s.label}>Name: <Text style={s.value}>{previewData.config.global.meta?.federation_name || 'Unnamed'}</Text></Text>
          <Text style={s.label}>Consensus Version: <Text style={s.value}>{previewData.config.global.consensus_version.major}.{previewData.config.global.consensus_version.minor}</Text></Text>
          <Text style={s.label}>Guardians: <Text style={s.value}>{Object.keys(previewData.config.global.api_endpoints).length}</Text></Text>
          <Text style={[s.label, { marginTop: 8 }]}>Guardian Endpoints:</Text>
          {Object.entries(previewData.config.global.api_endpoints).map(([id, peer]) => (
            <View key={id} style={s.guardianItem}>
              <Text style={s.guardianName}>{peer.name}</Text>
              <Text style={s.guardianUrl}>{peer.url}</Text>
            </View>
          ))}
          <Text style={[s.label, { marginTop: 8 }]}>Modules:</Text>
          {Object.entries(previewData.config.modules).map(([id, module]) => (
            <Text key={id} style={s.value}>• {module.kind}</Text>
          ))}
        </View>
      )}

      {!joinResult && open && <Text style={s.italic}>(You've already joined a federation)</Text>}
      {!!joinResult && <SuccessBox>{joinResult}</SuccessBox>}
      {!!joinError && <ErrorBox>{joinError}</ErrorBox>}
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
        placeholderTextColor="#a0aec0"
        value={inviteCode}
        onChangeText={setInviteCode}
      />
      <Btn title={parsing ? 'Parsing...' : 'Parse'} onPress={handleParse} disabled={parsing} />
      {parseResult && (
        <View style={s.resultBox}>
          <Text style={s.label}>Fed Id: <Text style={s.mono}>{parseResult.federation_id}</Text></Text>
          <Text style={s.label}>Fed url: <Text style={s.mono}>{parseResult.url}</Text></Text>
        </View>
      )}
      {!!parseError && <ErrorBox>{parseError}</ErrorBox>}
    </SectionCard>
  )
}

export const SettingsScreen = () => {
  const { open, checkIsOpen } = useIsOpen()

  return (
    <ScrollView style={s.container} contentContainerStyle={s.contentContainer} keyboardShouldPersistTaps="handled">
      <Text style={s.header}>Settings</Text>
      <JoinFederation open={open} checkIsOpen={checkIsOpen} />
      <MnemonicManager />
      <InviteCodeParser />
    </ScrollView>
  )
}
