import React, { useCallback, useEffect, useRef, useState } from 'react'
import {
  View,
  Text,
  TextInput,
  TouchableOpacity,
  ScrollView,
  Linking,
  Alert,
  SafeAreaView,
  StatusBar,
} from 'react-native'
import Clipboard from '@react-native-clipboard/clipboard'
import {
  InviteCode,
  Mnemonic,
  Notes,
  LnSendState_Tags,
  LnReceiveState_Tags,
  OnchainSendState_Tags,
  Network,
  type FederationPreview,
  type LnQuoteLike,
  type OnchainQuoteLike,
} from '@fedimint/react-native'
import {
  current,
  errorMessage,
  federation,
  generateWords,
  msatToSat,
  onChange,
  open,
  reset,
  satToMsat,
  walletExists,
} from './sdk'
import s from './styles'

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

type WalletState = { exists: boolean | undefined; joined: boolean }

/**
 * Tracks whether a wallet has been created on this device and whether it has
 * joined a federation. On mount it opens an existing wallet, if any;
 * `refresh` re-checks after an action that may have changed either. It also
 * re-checks on every `onChange` notification (another screen opened, restored
 * or reset the session), so every screen using this hook stays in sync;
 * `version` counts those notifications for `useBalance` to key its subscription on.
 */
const useWallet = () => {
  const [state, setState] = useState<WalletState>({
    exists: undefined,
    joined: false,
  })
  const [version, setVersion] = useState(0)

  const refresh = useCallback(() => {
    setState({ exists: true, joined: federation() !== undefined })
  }, [])

  useEffect(() => {
    let cancelled = false
    const load = async () => {
      const exists = await walletExists()
      if (cancelled) return
      if (!exists) {
        setState({ exists: false, joined: false })
        return
      }
      try {
        await open()
        if (!cancelled) {
          setState({ exists: true, joined: federation() !== undefined })
        }
      } catch {
        if (!cancelled) setState({ exists: true, joined: false })
      }
    }
    load()
    return () => {
      cancelled = true
    }
  }, [])

  useEffect(
    () =>
      onChange(() => {
        setState({ exists: current() !== undefined, joined: federation() !== undefined })
        setVersion((v) => v + 1)
      }),
    [],
  )

  return { ...state, refresh, version }
}

/** The joined federation's balance in sats, kept live through `balanceUpdates`. */
const useBalance = (joined: boolean, version: number): number => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    if (!joined) {
      setBalance(0)
      return
    }
    const fed = federation()
    if (!fed) return

    const controller = new AbortController()
    let cancelled = false

    const subscribe = async () => {
      try {
        const initial = await fed.balance()
        if (cancelled) return
        setBalance(msatToSat(initial))
        const updates = fed.balanceUpdates()
        for (;;) {
          const next = await updates.next({ signal: controller.signal })
          if (cancelled) return
          setBalance(msatToSat(next))
        }
      } catch (error) {
        if (!controller.signal.aborted) {
          console.warn('balance updates stopped:', errorMessage(error))
        }
      }
    }
    subscribe()

    return () => {
      cancelled = true
      controller.abort()
    }
    // `version` restarts the loop against the new session on every open/reset, so a
    // closed session's iterator is never awaited past the session that owned it.
  }, [joined, version])

  return balance
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
  exists,
  joined,
  balance,
  refresh,
}: {
  exists: boolean | undefined
  joined: boolean
  balance: number
  refresh: () => void
}) => {
  const fed = joined ? federation() : undefined
  const status =
    exists !== true
      ? 'No wallet yet'
      : !fed
        ? 'Wallet open, no federation'
        : `${fed.name() ?? fed.id()} (${Network[fed.network()]})`

  return (
    <SectionCard>
      <SectionTitle>Wallet Status</SectionTitle>
      <Row>
        <Text style={s.label}>Status:</Text>
        <Text style={s.value}>{status}</Text>
        <Btn title="Check" onPress={refresh} small />
      </Row>
      <Row>
        <Text style={s.label}>Balance:</Text>
        <Text style={s.balance}>{balance}</Text>
        <Text style={s.value}> sats</Text>
      </Row>
    </SectionCard>
  )
}

const MnemonicManager = ({
  exists,
  refresh,
}: {
  exists: boolean | undefined
  refresh: () => void
}) => {
  const [words, setWords] = useState('')
  const [visible, setVisible] = useState(false)
  const [restoreInput, setRestoreInput] = useState('')
  const [showRestore, setShowRestore] = useState(false)
  const [busy, setBusy] = useState(false)
  const [message, setMessage] = useState<{
    text: string
    type: 'success' | 'error'
  }>()

  const handleGenerateOrShow = async () => {
    setBusy(true)
    setMessage(undefined)
    try {
      if (exists) {
        const session = current()
        if (!session) throw new Error('wallet is not open')
        const list = session.sdk.exportMnemonic().words()
        setWords(list.join(' '))
        setVisible(true)
        setMessage({ text: 'Mnemonic retrieved!', type: 'success' })
      } else {
        // Seed the wallet with the exact words shown, so what the user backs up is
        // what actually protects the wallet, not a seed generated and discarded later.
        const list = generateWords()
        await open(list)
        setWords(list.join(' '))
        setVisible(true)
        setMessage({ text: 'Wallet created from a new mnemonic', type: 'success' })
      }
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
      // Parse the seed before resetting, so a bad entry is caught while the current wallet
      // is still there instead of after it has already been deleted.
      Mnemonic.fromWords(restoreWords)
      await reset()
      await open(restoreWords)
      refresh()
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

  const copyToClipboard = () => {
    Clipboard.setString(words)
    setMessage({ text: 'Copied to clipboard!', type: 'success' })
  }

  return (
    <SectionCard>
      <SectionTitle>🔑 Mnemonic Manager</SectionTitle>

      <Row>
        <Btn
          title={busy ? 'Working...' : exists ? 'Show' : 'Generate'}
          onPress={handleGenerateOrShow}
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
              title={visible ? '👁️' : '👁️‍🗨️'}
              onPress={() => setVisible(!visible)}
              small
            />
            <Btn
              title="📋"
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

const JoinFederation = ({
  joined,
  refresh,
}: {
  joined: boolean
  refresh: () => void
}) => {
  const [code, setCode] = useState(TESTNET_FEDERATION_CODE)
  const [preview, setPreview] = useState<FederationPreview>()
  const [previewing, setPreviewing] = useState(false)
  const [joinResult, setJoinResult] = useState('')
  const [joinError, setJoinError] = useState('')
  const [joining, setJoining] = useState(false)

  const previewFederation = async () => {
    if (!code.trim()) return
    setPreviewing(true)
    setJoinError('')
    try {
      const session = await open()
      const invite = InviteCode.parse(code.trim())
      setPreview(await session.sdk.preview(invite))
    } catch (error) {
      setJoinError(errorMessage(error))
      setPreview(undefined)
    } finally {
      setPreviewing(false)
    }
  }

  const joinFederation = async () => {
    setJoining(true)
    setJoinError('')
    try {
      const session = await open()
      const invite = InviteCode.parse(code.trim())
      await session.sdk.join(invite)
      setJoinResult('Joined!')
      refresh()
    } catch (error) {
      setJoinError(errorMessage(error))
      setJoinResult('')
    } finally {
      setJoining(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Join Federation</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Invite Code..."
        placeholderTextColor="#888"
        value={code}
        onChangeText={(text) => {
          setCode(text)
          setPreview(undefined)
        }}
        editable={!joined}
      />
      <Row>
        <Btn
          title={previewing ? 'Previewing...' : 'Preview'}
          onPress={previewFederation}
          disabled={previewing || !code.trim() || joined}
        />
        <Btn
          title={joining ? 'Joining...' : 'Join'}
          onPress={joinFederation}
          disabled={joined || joining}
          primary
        />
      </Row>

      {preview && (
        <View style={s.previewCard}>
          <Text style={s.previewTitle}>Federation Preview</Text>
          <Text style={s.label}>
            Name: <Text style={s.value}>{preview.name ?? 'Unnamed'}</Text>
          </Text>
          <Text style={s.label}>
            Network:{' '}
            <Text style={s.value}>{Network[preview.network]}</Text>
          </Text>
          <Text style={s.label}>
            Guardians: <Text style={s.value}>{preview.guardians}</Text>
          </Text>
          <Text style={s.label}>
            Modules: <Text style={s.value}>{preview.modules.join(', ')}</Text>
          </Text>
        </View>
      )}

      {!joinResult && joined && (
        <Text style={s.italic}>(You've already joined a federation)</Text>
      )}
      {!!joinResult && <SuccessBox>{joinResult}</SuccessBox>}
      {!!joinError && <ErrorBox>{joinError}</ErrorBox>}
    </SectionCard>
  )
}

const RedeemEcash = () => {
  const [input, setInput] = useState('')
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  const handleRedeem = async () => {
    setBusy(true)
    setResult('')
    setError('')
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const ecash = fed.ecash()
      if (!ecash) throw new Error('Ecash is not supported by this federation')
      const notes = (() => {
        try {
          return Notes.parse(input.trim())
        } catch (parseError) {
          throw new Error(`Invalid ecash notes: ${errorMessage(parseError)}`)
        }
      })()
      const operation = await ecash.receive(notes)
      await operation.awaitFinal()
      setResult(`Redeemed ${msatToSat(notes.value())} sats`)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setBusy(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Redeem Ecash</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Long ecash string..."
        placeholderTextColor="#888"
        value={input}
        onChangeText={setInput}
      />
      <Btn
        title={busy ? 'Redeeming...' : 'Redeem'}
        onPress={handleRedeem}
        disabled={busy || !input.trim()}
      />
      {!!result && <SuccessBox>{result}</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const SendLightning = () => {
  const [invoice, setInvoice] = useState('')
  const [quote, setQuote] = useState<LnQuoteLike>()
  const [quoting, setQuoting] = useState(false)
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  // A quote belongs to the input it was requested for: this counts each request so a quote
  // that resolves after the invoice has since changed can tell it is stale and drop itself.
  const quoteGen = useRef(0)

  const handleQuote = async () => {
    const gen = ++quoteGen.current
    setQuoting(true)
    setError('')
    setResult('')
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const lightning = fed.lightning()
      if (!lightning) {
        throw new Error('Lightning is not supported by this federation')
      }
      const next = await lightning.quote(invoice.trim())
      if (gen !== quoteGen.current) return
      setQuote(next)
    } catch (error) {
      setError(errorMessage(error))
      setQuote(undefined)
    } finally {
      setQuoting(false)
    }
  }

  const handlePay = async () => {
    if (!quote) return
    setSending(true)
    setError('')
    try {
      const lightning = federation()?.lightning()
      if (!lightning) {
        throw new Error('Lightning is not supported by this federation')
      }
      const operation = await lightning.send(quote)
      const state = await operation.awaitFinal()
      setResult(LnSendState_Tags[state.tag])
      setQuote(undefined)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setSending(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Pay Lightning</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="lnbc..."
        placeholderTextColor="#888"
        value={invoice}
        onChangeText={(text) => {
          setInvoice(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        editable={!quoting}
      />
      <Btn
        title={quoting ? 'Quoting...' : 'Quote'}
        onPress={handleQuote}
        disabled={quoting || !invoice.trim()}
      />

      {quote && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Fee: <Text style={s.value}>{msatToSat(quote.fee())} sats</Text>
          </Text>
          <Text style={s.label}>
            Total: <Text style={s.value}>{msatToSat(quote.total())} sats</Text>
          </Text>
          <Btn
            title={sending ? 'Paying...' : `Pay ${msatToSat(quote.total())} sats`}
            onPress={handlePay}
            disabled={sending}
            primary
          />
        </View>
      )}
      {!!result && <SuccessBox>{result}</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

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

  const copyInvoice = () => {
    Clipboard.setString(invoice)
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
          {!!status && <Text style={s.value}>{status}</Text>}
          <Btn title="Copy" onPress={copyInvoice} small />
        </View>
      )}
      {!!error && <ErrorBox>{error}</ErrorBox>}
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

const Deposit = () => {
  const [address, setAddress] = useState('')
  const [error, setError] = useState('')
  const [loading, setLoading] = useState(false)

  const handleGenerate = async () => {
    setError('')
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
      setError(errorMessage(error))
    } finally {
      setLoading(false)
    }
  }

  const copyAddress = () => {
    Clipboard.setString(address)
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
        <View style={s.invoiceBox}>
          <Text style={s.label}>Deposit address:</Text>
          <Text style={s.mono} selectable>
            {address}
          </Text>
          <Btn title="Copy" onPress={copyAddress} small />
        </View>
      )}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const SendOnchain = () => {
  const [address, setAddress] = useState('')
  const [amount, setAmount] = useState('')
  const [quote, setQuote] = useState<OnchainQuoteLike>()
  const [quoting, setQuoting] = useState(false)
  const [sending, setSending] = useState(false)
  const [result, setResult] = useState('')
  const [error, setError] = useState('')
  // A quote belongs to the input it was requested for: this counts each request so a quote
  // that resolves after the amount or address has since changed can tell it is stale and drop
  // itself.
  const quoteGen = useRef(0)

  const handleQuote = async () => {
    const gen = ++quoteGen.current
    setQuote(undefined)
    setResult('')
    setError('')
    setQuoting(true)
    try {
      const fed = federation()
      if (!fed) throw new Error('Join a federation first')
      const onchain = fed.onchain()
      if (!onchain) {
        throw new Error('On-chain is not supported by this federation')
      }
      const next = await onchain.quote(address.trim(), BigInt(amount.trim()))
      if (gen !== quoteGen.current) return
      setQuote(next)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setQuoting(false)
    }
  }

  const handleSend = async () => {
    if (!quote) return
    setSending(true)
    setError('')
    try {
      const onchain = federation()?.onchain()
      if (!onchain) {
        throw new Error('On-chain is not supported by this federation')
      }
      const operation = await onchain.send(quote)
      const state = await operation.awaitFinal()
      setResult(OnchainSendState_Tags[state.tag])
      setQuote(undefined)
    } catch (error) {
      setError(errorMessage(error))
    } finally {
      setSending(false)
    }
  }

  return (
    <SectionCard>
      <SectionTitle>Send Onchain</SectionTitle>
      <TextInput
        style={s.input}
        placeholder="Enter amount in sats"
        placeholderTextColor="#888"
        keyboardType="numeric"
        value={amount}
        onChangeText={(text) => {
          setAmount(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        editable={!quoting}
      />
      <TextInput
        style={s.input}
        placeholder="Enter onchain address"
        placeholderTextColor="#888"
        value={address}
        onChangeText={(text) => {
          setAddress(text)
          setQuote(undefined)
          quoteGen.current += 1
        }}
        editable={!quoting}
      />
      <Btn
        title={quoting ? 'Quoting...' : 'Quote'}
        onPress={handleQuote}
        disabled={quoting || !amount.trim() || !address.trim()}
      />

      {quote && (
        <View style={s.resultBox}>
          <Text style={s.label}>
            Fee: <Text style={s.value}>{msatToSat(quote.fee())} sats</Text>
          </Text>
          <Text style={s.label}>
            Total: <Text style={s.value}>{msatToSat(quote.total())} sats</Text>
          </Text>
          <Btn
            title={sending ? 'Sending...' : `Send ${msatToSat(quote.total())} sats`}
            onPress={handleSend}
            disabled={sending}
            primary
          />
        </View>
      )}
      {!!result && <SuccessBox>{result}</SuccessBox>}
      {!!error && <ErrorBox>{error}</ErrorBox>}
    </SectionCard>
  )
}

const App = () => {
  const { exists, joined, refresh, version } = useWallet()
  const balance = useBalance(joined, version)

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
          exists={exists}
          joined={joined}
          balance={balance}
          refresh={refresh}
        />
        <MnemonicManager exists={exists} refresh={refresh} />
        <JoinFederation joined={joined} refresh={refresh} />
        <GenerateLightningInvoice />
        <RedeemEcash />
        <SendLightning />
        <InviteCodeParser />
        <QuoteLightningInvoice />
        <Deposit />
        <SendOnchain />
      </ScrollView>
    </SafeAreaView>
  )
}

export default App
