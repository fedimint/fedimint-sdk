import React, { useCallback, useEffect, useState } from 'react'
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  ScrollView,
  Linking,
  Alert,
  StyleSheet,
  SafeAreaView,
  StatusBar,
} from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import { wallet, director } from './wallet'
import s from './styles'
import type {
  ParsedInviteCode,
  ParsedBolt11Invoice,
  PreviewFederation,
} from '@fedimint/core'

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

const useIsOpen = () => {
  const [open, setIsOpen] = useState(false)

  const checkIsOpen = useCallback(() => {
    if (wallet && open !== wallet.isOpen()) {
      setIsOpen(wallet.isOpen())
    }
  }, [open])

  useEffect(() => {
    const tryOpen = async () => {
      try {
        if (wallet && !wallet.isOpen()) {
          console.log('Attempting to open wallet on startup...')
          await wallet.open()
          checkIsOpen()
        }
      } catch (e) {
        console.log('Wallet could not be opened on startup (might not be joined): ', e)
      }
    }
    tryOpen()
    checkIsOpen()
  }, [checkIsOpen])

  return { open, checkIsOpen }
}

const useBalance = (checkIsOpen: () => void) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    const unsubscribe = wallet?.balance.subscribeBalance((bal) => {
      checkIsOpen()
      setBalance(bal)
    })

    return () => {
      unsubscribe?.()
    }
  }, [checkIsOpen])

  return balance
}

const extractErrorMessage = (error: any): string => {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error !== null) {
    if (error.error) return error.error
    if (error.message) return error.message
  }
  return 'Operation failed'
}

const SectionCard: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => <View style={s.section}>{children}</View>

const SectionTitle: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => <Text style={s.sectionTitle}>{String(children)}</Text>

const Btn: React.FC<{
  title: string
  onPress: () => void
  disabled?: boolean
  active?: boolean
  small?: boolean
  primary?: boolean
}> = ({ title, onPress, disabled, active, small, primary }) => (
  <TouchableOpacity
    onPress={onPress}
    disabled={disabled}
    style={[
      s.btn,
      active && s.btnActive,
      small && s.btnSmall,
      primary && s.btnPrimary,
      disabled && s.btnDisabled,
    ]}
  >
    <Text
      style={[
        s.btnText,
        small && s.btnTextSmall,
        disabled && s.btnTextDisabled,
      ]}
    >
      {title}
    </Text>
  </TouchableOpacity>
)

const SuccessBox: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <View style={s.success}>
    <Text style={s.successText}>{children}</Text>
  </View>
)

const ErrorBox: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <View style={s.error}>
    <Text style={s.errorText}>{children}</Text>
  </View>
)

const Row: React.FC<{ children: React.ReactNode }> = ({ children }) => (
  <View style={s.row}>{children}</View>
)

const WalletStatus = ({
  open,
  checkIsOpen,
  balance,
}: {
  open: boolean
  checkIsOpen: () => void
  balance: number
}) => (
  <SectionCard>
    <SectionTitle>Wallet Status</SectionTitle>
    <Row>
      <Text style={s.label}>Is Wallet Open?</Text>
      <Text style={s.value}>{open ? 'Yes' : 'No'}</Text>
      <Btn title="Check" onPress={checkIsOpen} small />
    </Row>
    <Row>
      <Text style={s.label}>Balance:</Text>
      <Text style={s.balance}>{balance}</Text>
      <Text style={s.value}> sats</Text>
    </Row>
  </SectionCard>
)

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
              title={showMnemonic ? '👁️' : '👁️‍🗨️'}
              onPress={() => setShowMnemonic(!showMnemonic)}
              small
            />
            <Btn
              title="📋"
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

const JoinFederation = ({
  open,
  checkIsOpen,
}: {
  open: boolean
  checkIsOpen: () => void
}) => {
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
        placeholderTextColor="#888"
        value={inviteCode}
        onChangeText={(text) => {
          setInviteCode(text)
          setPreviewData(null)
        }}
        editable={!open}
      />
      <Row>
        <Btn
          title={previewing ? 'Previewing...' : 'Preview'}
          onPress={previewFederationHandler}
          disabled={previewing || !inviteCode.trim() || open}
        />
        <Btn
          title={joining ? 'Joining...' : 'Join'}
          onPress={joinFederation}
          disabled={open || joining}
          primary
        />
      </Row>

      {previewData && (
        <View style={s.previewCard}>
          <Text style={s.previewTitle}>Federation Preview</Text>
          <Text style={s.label}>
            Federation ID:{' '}
            <Text style={s.mono}>{previewData.federation_id}</Text>
          </Text>
          <Text style={s.label}>
            Name:{' '}
            <Text style={s.value}>
              {previewData.config.global.meta?.federation_name || 'Unnamed'}
            </Text>
          </Text>
          <Text style={s.label}>
            Consensus Version:{' '}
            <Text style={s.value}>
              {previewData.config.global.consensus_version.major}.
              {previewData.config.global.consensus_version.minor}
            </Text>
          </Text>
          <Text style={s.label}>
            Guardians:{' '}
            <Text style={s.value}>
              {Object.keys(previewData.config.global.api_endpoints).length}
            </Text>
          </Text>

          <Text style={[s.label, { marginTop: 8 }]}>Guardian Endpoints:</Text>
          {Object.entries(previewData.config.global.api_endpoints).map(
            ([id, peer]) => (
              <View key={id} style={s.guardianItem}>
                <Text style={s.guardianName}>{peer.name}</Text>
                <Text style={s.guardianUrl}>{peer.url}</Text>
              </View>
            ),
          )}

          <Text style={[s.label, { marginTop: 8 }]}>Modules:</Text>
          {Object.entries(previewData.config.modules).map(([id, module]) => (
            <Text key={id} style={s.value}>
              • {module.kind}
            </Text>
          ))}
        </View>
      )}

      {!joinResult && open && (
        <Text style={s.italic}>(You've already joined a federation)</Text>
      )}
      {!!joinResult && <SuccessBox>{joinResult}</SuccessBox>}
      {!!joinError && <ErrorBox>{joinError}</ErrorBox>}
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
      <Btn title="Pay" onPress={handleSubmit} />
      {!!lightningResult && <SuccessBox>{lightningResult}</SuccessBox>}
      {!!lightningError && <ErrorBox>{lightningError}</ErrorBox>}
    </SectionCard>
  )
}

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
      <TouchableOpacity
        onPress={() => Linking.openURL('https://faucet.mutinynet.com/')}
      >
        <Text style={s.link}>mutinynet faucet ↗</Text>
      </TouchableOpacity>

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
      <Btn
        title={loading ? 'Generating...' : 'Generate'}
        onPress={handleGenerate}
        disabled={loading}
        primary
      />
      {!!address && <SuccessBox>{address}</SuccessBox>}
      {!!addressError && <ErrorBox>{addressError}</ErrorBox>}
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

const App = () => {
  const { open, checkIsOpen } = useIsOpen()
  const balance = useBalance(checkIsOpen)

  return (
    <SafeAreaView style={s.safeArea}>
      <StatusBar barStyle="light-content" backgroundColor="#1a1a2e" />
      <ScrollView
        style={s.container}
        contentContainerStyle={s.contentContainer}
        keyboardShouldPersistTaps="handled"
      >
        <Text style={s.header}>Fedimint Typescript Library Demo</Text>

        <View style={s.stepsCard}>
          <Text style={s.stepsTitle}>Steps to get started:</Text>
          <Text style={s.stepItem}>
            1. Join a Federation (persists across sessions)
          </Text>
          <Text style={s.stepItem}>2. Generate an Invoice</Text>
          <Text style={s.stepItem}>
            3. Pay the Invoice using the mutinynet faucet
          </Text>
          <TouchableOpacity
            onPress={() => Linking.openURL('https://faucet.mutinynet.com/')}
          >
            <Text style={s.link}>
              {'   '}https://faucet.mutinynet.com/ ↗
            </Text>
          </TouchableOpacity>
        </View>

        <WalletStatus
          open={open}
          checkIsOpen={checkIsOpen}
          balance={balance}
        />
        <MnemonicManager />
        <JoinFederation open={open} checkIsOpen={checkIsOpen} />
        <GenerateLightningInvoice />
        <RedeemEcash />
        <SendLightning />
        <InviteCodeParser />
        <ParseLightningInvoice />
        <Deposit />
        <SendOnchain />
      </ScrollView>
    </SafeAreaView>
  )
}

export default App