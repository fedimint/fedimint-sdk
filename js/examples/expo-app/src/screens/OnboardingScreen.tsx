import React, { useState } from 'react'
import {
  View,
  Text,
  TextInput,
  StyleSheet,
  ScrollView,
  KeyboardAvoidingView,
  Platform,
} from 'react-native'
import { Ionicons } from '@expo/vector-icons'
import { errorMessage, generateWords, open } from '../sdk'
import { Btn, ErrorBox, SuccessBox } from '../components'

type Step = 'welcome' | 'mnemonic'

export default function OnboardingScreen({
  onComplete,
}: {
  onComplete: () => void
}) {
  const [step, setStep] = useState<Step>('welcome')

  return (
    <KeyboardAvoidingView
      style={styles.flex}
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
    >
      <ScrollView
        style={styles.container}
        contentContainerStyle={styles.content}
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
    <View style={styles.center}>
      <Ionicons name="wallet" size={64} color="#60a5fa" />
      <Text style={styles.title}>Welcome to Fedimint Wallet</Text>
      <Text style={styles.description}>
        A self-custodial wallet powered by Fedimint. To get started, you'll need
        to set up a recovery mnemonic.
      </Text>
      <View style={styles.btnContainer}>
        <Btn title="Get Started" onPress={onNext} primary />
      </View>
    </View>
  )
}

function MnemonicStep({ onComplete }: { onComplete: () => void }) {
  const [mode, setMode] = useState<'choose' | 'generate' | 'import'>('choose')
  const [words, setWords] = useState<string[]>([])
  const [importInput, setImportInput] = useState('')
  const [loading, setLoading] = useState(false)
  const [message, setMessage] = useState<{
    text: string
    type: 'success' | 'error'
  }>()

  const handleGenerate = () => {
    setMessage(undefined)
    setWords(generateWords())
    setMode('generate')
  }

  const handleContinue = async () => {
    setLoading(true)
    setMessage(undefined)
    try {
      await open(words)
      setMessage({ text: 'Wallet created!', type: 'success' })
      setTimeout(onComplete, 600)
    } catch (error) {
      setMessage({ text: errorMessage(error), type: 'error' })
    } finally {
      setLoading(false)
    }
  }

  const handleImport = async () => {
    const importWords = importInput.trim().split(/\s+/).filter(Boolean)
    if (importWords.length === 0) return
    setLoading(true)
    setMessage(undefined)
    try {
      await open(importWords)
      setMessage({ text: 'Wallet imported!', type: 'success' })
      setTimeout(onComplete, 600)
    } catch (error) {
      setMessage({ text: errorMessage(error), type: 'error' })
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
      <View style={styles.center}>
        <Ionicons name="key-outline" size={48} color="#60a5fa" />
        <Text style={styles.title}>Set Up Your Mnemonic</Text>
        <Text style={styles.description}>
          Your mnemonic phrase is used to recover your wallet. You can generate
          a new one or import an existing phrase.
        </Text>
        <View style={styles.optionRow}>
          <View style={styles.optionBtn}>
            <Btn title="Generate New" onPress={handleGenerate} disabled={loading} primary />
          </View>
          <View style={styles.optionBtn}>
            <Btn
              title="Import Existing"
              onPress={() => setMode('import')}
              disabled={loading}
            />
          </View>
        </View>
        {renderMessage()}
      </View>
    )
  }

  if (mode === 'generate') {
    return (
      <View style={styles.center}>
        <Ionicons name="document-text-outline" size={48} color="#4ade80" />
        <Text style={styles.title}>Your Recovery Phrase</Text>
        <Text style={styles.warningText}>
          Write these words down and store them safely. You will need them to
          recover your wallet.
        </Text>
        <View style={styles.mnemonicBox}>
          <Text style={styles.mnemonicText}>{words.join(' ')}</Text>
        </View>
        <View style={styles.btnContainer}>
          <Btn
            title={loading ? 'Creating...' : "I've Saved My Phrase — Continue"}
            onPress={handleContinue}
            disabled={loading}
            primary
          />
        </View>
        {renderMessage()}
      </View>
    )
  }

  // mode === 'import'
  return (
    <View style={styles.center}>
      <Ionicons name="download-outline" size={48} color="#60a5fa" />
      <Text style={styles.title}>Import Mnemonic</Text>
      <Text style={styles.description}>
        Enter your 12 or 24 word recovery phrase, separated by spaces.
      </Text>
      <TextInput
        style={styles.textArea}
        placeholder="Enter your mnemonic words..."
        placeholderTextColor="#888"
        value={importInput}
        onChangeText={setImportInput}
        multiline
        numberOfLines={3}
        autoCapitalize="none"
        autoCorrect={false}
      />
      <View style={styles.importActions}>
        <Btn title="Back" onPress={() => setMode('choose')} />
        <Btn
          title={loading ? 'Importing...' : 'Import & Continue'}
          onPress={handleImport}
          disabled={loading || !importInput.trim()}
          primary
        />
      </View>
      {renderMessage()}
    </View>
  )
}

const styles = StyleSheet.create({
  flex: {
    flex: 1,
  },
  container: {
    flex: 1,
    backgroundColor: '#1a1a2e',
  },
  content: {
    flexGrow: 1,
    justifyContent: 'center',
    padding: 24,
  },
  center: {
    alignItems: 'center',
  },
  title: {
    color: '#ffffff',
    fontSize: 22,
    fontWeight: '700',
    textAlign: 'center',
    marginTop: 16,
    marginBottom: 8,
  },
  description: {
    color: '#b0b0b0',
    fontSize: 15,
    textAlign: 'center',
    lineHeight: 22,
    marginBottom: 24,
    paddingHorizontal: 8,
  },
  warningText: {
    color: '#fbbf24',
    fontSize: 14,
    textAlign: 'center',
    lineHeight: 20,
    marginBottom: 16,
    paddingHorizontal: 8,
  },
  btnContainer: {
    width: '100%',
    marginTop: 8,
  },
  optionRow: {
    flexDirection: 'row',
    gap: 12,
    width: '100%',
  },
  optionBtn: {
    flex: 1,
  },
  mnemonicBox: {
    backgroundColor: '#0f3460',
    borderRadius: 12,
    padding: 16,
    width: '100%',
    marginBottom: 16,
  },
  mnemonicText: {
    color: '#e0e0e0',
    fontFamily: 'monospace',
    fontSize: 15,
    lineHeight: 24,
    textAlign: 'center',
  },
  textArea: {
    backgroundColor: '#0f3460',
    color: '#ffffff',
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 14,
    marginBottom: 12,
    borderWidth: 1,
    borderColor: '#1a4a7a',
    minHeight: 80,
    textAlignVertical: 'top',
    width: '100%',
  },
  importActions: {
    flexDirection: 'row',
    gap: 12,
    width: '100%',
  },
})
