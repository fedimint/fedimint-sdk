import React, { useState } from 'react'
import {
  View,
  Text,
  TextInput,
  ScrollView,
  KeyboardAvoidingView,
  Platform,
} from 'react-native'
import { Ionicons } from '@expo/vector-icons'
import { director } from '../wallet'
import { Btn, ErrorBox, SuccessBox, SectionCard, SectionTitle, Row } from '../components/common'
import { extractErrorMessage } from '../hooks'
import s from '../styles'

type Step = 'welcome' | 'mnemonic'

export const OnboardingScreen = ({
  onComplete,
}: {
  onComplete: () => void
}) => {
  const [step, setStep] = useState<Step>('welcome')

  return (
    <KeyboardAvoidingView
      style={{ flex: 1 }}
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
    >
      <ScrollView
        style={s.container}
        contentContainerStyle={s.contentContainer}
        keyboardShouldPersistTaps="handled"
      >
        {step === 'welcome' && (
          <WelcomeStep onNext={() => setStep('mnemonic')} />
        )}
        {step === 'mnemonic' && <MnemonicStep onComplete={onComplete} />}
      </ScrollView>
    </KeyboardAvoidingView>
  )
}

function WelcomeStep({ onNext }: { onNext: () => void }) {
  return (
    <SectionCard>
      <View style={{ alignItems: 'center', marginBottom: 24 }}>
        <Ionicons name="wallet" size={64} color="#4fd1c5" />
        <SectionTitle>Welcome to Fedimint</SectionTitle>
        <Text style={[s.label, { textAlign: 'center', paddingHorizontal: 16 }]}>
          A self-custodial wallet powered by Fedimint. To get started, you'll need
          to set up a recovery mnemonic.
        </Text>
      </View>
      <Btn title="Get Started" onPress={onNext} primary />
    </SectionCard>
  )
}

function MnemonicStep({ onComplete }: { onComplete: () => void }) {
  const [mode, setMode] = useState<'choose' | 'generate' | 'import'>('choose')
  const [mnemonic, setMnemonic] = useState('')
  const [importInput, setImportInput] = useState('')
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState<{
    text: string
    type: 'success' | 'error'
  }>()

  const handleGenerate = async () => {
    setLoading(true)
    setMessage(undefined)
    try {
      const words = await director.generateMnemonic()
      setMnemonic(words.join(' '))
      setMode('generate')
    } catch (error) {
      setMessage({ text: extractErrorMessage(error), type: 'error' })
    } finally {
      setLoading(false)
    }
  }

  const handleSaveMnemonic = async () => {
    setLoading(true)
    setMessage(undefined)
    try {
      const words = mnemonic.trim().split(/\s+/)
      await director.setMnemonic(words)
      setMessage({ text: 'Mnemonic saved!', type: 'success' })
      setTimeout(onComplete, 600)
    } catch (error) {
      const msg = extractErrorMessage(error)
      if (msg.toLowerCase().includes('already exists')) {
        setMessage({ text: 'Mnemonic already set!', type: 'success' })
        setTimeout(onComplete, 600)
      } else {
        setMessage({ text: msg, type: 'error' })
      }
    } finally {
      setLoading(false)
    }
  }

  const handleImport = async () => {
    if (!importInput.trim()) return
    setLoading(true)
    setMessage(undefined)
    try {
      const words = importInput.trim().split(/\s+/)
      await director.setMnemonic(words)
      setMessage({ text: 'Mnemonic imported!', type: 'success' })
      setTimeout(onComplete, 600)
    } catch (error) {
      const msg = extractErrorMessage(error)
      if (msg.toLowerCase().includes('already exists')) {
        setMessage({ text: 'Mnemonic already set!', type: 'success' })
        setTimeout(onComplete, 600)
      } else {
        setMessage({ text: msg, type: 'error' })
      }
    } finally {
      setLoading(false)
    }
  }

  const renderMessage = () => {
    if (!message) return null
    return message.type === 'success' ? (
      <SuccessBox>{message.text}</SuccessBox>
    ) : (
      <ErrorBox>{message.text}</ErrorBox>
    )
  }

  if (mode === 'choose') {
    return (
      <SectionCard>
        <View style={{ alignItems: 'center', marginBottom: 24 }}>
          <Ionicons name="key-outline" size={48} color="#4fd1c5" />
          <SectionTitle>Set Up Mnemonic</SectionTitle>
          <Text style={[s.label, { textAlign: 'center' }]}>
            Your mnemonic phrase is used to recover your wallet. Generate a new one
            or import an existing phrase.
          </Text>
        </View>
        <View style={{ gap: 12 }}>
          <Btn
            title={loading ? 'Generating...' : 'Generate New'}
            onPress={handleGenerate}
            disabled={loading}
            primary
          />
          <Btn
            title="Import Existing"
            onPress={() => setMode('import')}
            disabled={loading}
          />
        </View>
        {renderMessage()}
      </SectionCard>
    )
  }

  if (mode === 'generate') {
    return (
      <SectionCard>
        <View style={{ alignItems: 'center', marginBottom: 16 }}>
          <Ionicons name="document-text-outline" size={48} color="#4fd1c5" />
          <SectionTitle>Recovery Phrase</SectionTitle>
          <Text style={[s.label, { textAlign: 'center', color: '#d97706' }]}>
            Write these words down and store them safely.
          </Text>
        </View>
        <View style={s.mnemonicDisplay}>
          <Text style={s.mnemonicText}>{mnemonic}</Text>
        </View>
        <View style={{ marginTop: 24 }}>
          <Btn
            title={loading ? 'Saving...' : "I've Saved It — Continue"}
            onPress={handleSaveMnemonic}
            disabled={loading}
            primary
          />
        </View>
        {renderMessage()}
      </SectionCard>
    )
  }

  // mode === 'import'
  return (
    <SectionCard>
      <View style={{ alignItems: 'center', marginBottom: 16 }}>
        <Ionicons name="download-outline" size={48} color="#4fd1c5" />
        <SectionTitle>Import Mnemonic</SectionTitle>
        <Text style={[s.label, { textAlign: 'center' }]}>
          Enter your 12 or 24 word recovery phrase.
        </Text>
      </View>
      <TextInput
        style={s.textArea}
        placeholder="Enter your mnemonic words..."
        placeholderTextColor="#a0aec0"
        value={importInput}
        onChangeText={setImportInput}
        multiline
        numberOfLines={3}
        autoCapitalize="none"
        autoCorrect={false}
      />
      <View style={{ gap: 12, marginTop: 12 }}>
        <Btn
          title={loading ? 'Importing...' : 'Import & Continue'}
          onPress={handleImport}
          disabled={loading || !importInput.trim()}
          primary
        />
        <Btn title="Back" onPress={() => setMode('choose')} disabled={loading} />
      </View>
      {renderMessage()}
    </SectionCard>
  )
}
