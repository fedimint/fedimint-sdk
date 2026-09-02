import React, { useCallback, useEffect, useRef, useState } from 'react'
import { director, getWallet } from './wallet'
import type {
  FedimintWallet,
  ParsedInviteCode,
  ParsedBolt11Invoice,
  PreviewFederation,
} from '@fedimint/core'

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

type AppPhase = 'loading' | 'onboarding' | 'ready'

// ── Wallet Context ────────────────────────────────────────────────────
// Instead of importing a mutable module-level variable, components
// receive the initialized wallet via a hook that guarantees it is ready.

const WalletContext = React.createContext<FedimintWallet | null>(null)

function useWallet(): FedimintWallet | null {
  return React.useContext(WalletContext)
}

// ── Hooks ─────────────────────────────────────────────────────────────

const useIsOpen = (wallet: FedimintWallet | null) => {
  const [open, setIsOpen] = useState(false)

  const checkIsOpen = useCallback(() => {
    if (wallet) {
      setIsOpen(wallet.isOpen())
    }
  }, [wallet])

  // Re-check whenever wallet reference changes
  useEffect(() => {
    checkIsOpen()
  }, [checkIsOpen])

  return { open, checkIsOpen }
}

const useBalance = (wallet: FedimintWallet | null, checkIsOpen: () => void) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    if (!wallet) return

    const unsubscribe = wallet.balance.subscribeBalance((bal) => {
      checkIsOpen()
      setBalance(bal)
    })

    return () => {
      unsubscribe?.()
    }
  }, [wallet, checkIsOpen])

  return balance
}

// ── App ───────────────────────────────────────────────────────────────

const App = () => {
  const [phase, setPhase] = useState<AppPhase>('loading')
  const [wallet, setWallet] = useState<FedimintWallet | null>(null)

  useEffect(() => {
    let cancelled = false

    const checkOnboarding = async () => {
      try {
        const hasMnemonic = await director.hasMnemonicSet()
        if (cancelled) return

        if (hasMnemonic) {
          // Verify user actually backed up the phrase
          const backupConfirmed =
            localStorage.getItem('backupConfirmed') === 'true'
          if (!backupConfirmed) {
            if (!cancelled) setPhase('onboarding')
            return
          }

          // Mnemonic exists, try to open the wallet
          try {
            const w = await getWallet()
            if (cancelled) return
            await w.open()
            setWallet(w)
          } catch (e) {
            console.warn(
              'Wallet has mnemonic but could not open client (may need to join a federation)',
              e,
            )
            // Still set wallet so user can joinFederation
            const w = await getWallet()
            if (!cancelled) setWallet(w)
          }
          if (!cancelled) setPhase('ready')
        } else {
          if (!cancelled) setPhase('onboarding')
        }
      } catch (e) {
        console.error('Error checking onboarding state:', e)
        if (!cancelled) setPhase('onboarding')
      }
    }

    checkOnboarding()
    return () => {
      cancelled = true
    }
  }, [])

  const handleOnboardingComplete = useCallback(async () => {
    try {
      const w = await getWallet()
      await w.open()
      setWallet(w)
    } catch (e) {
      console.warn(
        'Wallet could not be opened after onboarding (may need to join a federation)',
        e,
      )
      const w = await getWallet()
      setWallet(w)
    }
    setPhase('ready')
  }, [])

  if (phase === 'loading') {
    return (
      <div className="loading-screen">
        <div className="loading-spinner" />
        <p>Initializing wallet...</p>
      </div>
    )
  }

  if (phase === 'onboarding') {
    return <OnboardingScreen onComplete={handleOnboardingComplete} />
  }

  return (
    <WalletContext.Provider value={wallet}>
      <AppContent />
    </WalletContext.Provider>
  )
}

// ── Main Content (rendered only when wallet is initialized) ───────────

const AppContent = () => {
  const wallet = useWallet()
  const { open, checkIsOpen } = useIsOpen(wallet)
  const balance = useBalance(wallet, checkIsOpen)

  return (
    <>
      <header>
        <h1>Fedimint Typescript Library Demo</h1>

        <div className="steps">
          <strong>Steps to get started:</strong>
          <ol>
            <li>Join a Federation (persists across sessions)</li>
            <li>Generate an Invoice</li>
            <li>
              Pay the Invoice using the{' '}
              <a href="https://faucet.mutinynet.com/" target="_blank">
                mutinynet faucet
              </a>
            </li>
            <li>
              Investigate the Browser Tools
              <ul>
                <li>Browser Console for logs</li>
                <li>Network Tab (websocket) for guardian requests</li>
                <li>Application Tab for state</li>
              </ul>
            </li>
          </ol>
        </div>
      </header>
      <main>
        <WalletStatus open={open} checkIsOpen={checkIsOpen} balance={balance} />
        <JoinFederation open={open} checkIsOpen={checkIsOpen} />
        <GenerateLightningInvoice />
        <RedeemEcash />
        <SendLightning />
        <InviteCodeParser />
        <ParseLightningInvoice />
        <Deposit />
        <SendOnchain />
      </main>
    </>
  )
}

const OnboardingScreen = ({
  onComplete,
}: {
  onComplete: () => Promise<void>
}) => {
  const [step, setStep] = useState<'welcome' | 'generate' | 'restore'>(
    'welcome',
  )
  const [generatedMnemonic, setGeneratedMnemonic] = useState('')
  const [inputMnemonic, setInputMnemonic] = useState('')
  const [showMnemonic, setShowMnemonic] = useState(false)
  const [backedUp, setBackedUp] = useState(false)
  const [isLoading, setIsLoading] = useState(false)
  const [error, setError] = useState('')
  const [copyFeedback, setCopyFeedback] = useState('')

  const extractErrorMessage = (error: any): string => {
    if (error instanceof Error) return error.message
    if (typeof error === 'object' && error !== null) {
      return error.error || error.message || String(error)
    }
    return String(error)
  }

  // Recover unconfirmed mnemonic on boot
  useEffect(() => {
    let cancelled = false
    const recoverMnemonic = async () => {
      try {
        if (step === 'welcome') {
          const hasMnemonic = await director.hasMnemonicSet()
          if (
            hasMnemonic &&
            localStorage.getItem('backupConfirmed') !== 'true'
          ) {
            const existing = await director.getMnemonic()
            if (cancelled) return
            setGeneratedMnemonic(existing.join(' '))
            setShowMnemonic(true)
            setStep('generate')
          }
        }
      } catch (e) {
        console.error('Failed to check unconfirmed backup', e)
      }
    }
    recoverMnemonic()
    return () => {
      cancelled = true
    }
  }, [step])

  const handleGenerate = async () => {
    setIsLoading(true)
    setError('')
    try {
      const words = await director.generateMnemonic()
      setGeneratedMnemonic(words.join(' '))
      setShowMnemonic(true)
      setStep('generate')
    } catch (err) {
      console.error('Error generating mnemonic:', err)
      setError(extractErrorMessage(err))
    } finally {
      setIsLoading(false)
    }
  }

  const handleConfirmGenerated = async () => {
    if (!backedUp) return
    setIsLoading(true)
    setError('')
    try {
      const words = generatedMnemonic.split(' ')
      await director.setMnemonic(words)
      localStorage.setItem('backupConfirmed', 'true')
      await onComplete()
    } catch (err) {
      const msg = extractErrorMessage(err)
      if (
        msg.includes('mnemonic already exists') ||
        msg.includes('already set')
      ) {
        // The Rust backend's generateMnemonic saves the mnemonic to the DB immediately.
        // So setMnemonic will fail with "already exists". But we MUST verify the
        // stored mnemonic actually matches what the user wrote down, otherwise
        // they could end up with a wallet backed by a different key than they saved.
        try {
          const existing = await director.getMnemonic()
          if (existing.join(' ') === generatedMnemonic) {
            localStorage.setItem('backupConfirmed', 'true')
            await onComplete()
          } else {
            // CRITICAL: DB has a DIFFERENT mnemonic than what was displayed.
            // The user wrote down the wrong key. Force a wipe.
            setError(
              'CRITICAL: The stored mnemonic does not match the one displayed. ' +
                'This means stale data exists. You must wipe and start fresh.',
            )
            setIsLoading(false)
          }
        } catch (verifyErr) {
          console.error('Failed to verify existing mnemonic:', verifyErr)
          setError(
            'Could not verify the stored mnemonic. Please wipe and try again.',
          )
          setIsLoading(false)
        }
      } else {
        console.error('Error setting mnemonic:', err)
        setError(msg)
        setIsLoading(false)
      }
    }
  }

  const handleRestore = async (e: React.FormEvent) => {
    e.preventDefault()
    const trimmed = inputMnemonic.trim()
    if (!trimmed) return

    const words = trimmed.split(/\s+/)
    if (words.length !== 12 && words.length !== 24) {
      setError('Mnemonic must be exactly 12 or 24 words')
      return
    }

    setIsLoading(true)
    setError('')
    try {
      await director.setMnemonic(words)
      localStorage.setItem('backupConfirmed', 'true')
      await onComplete()
    } catch (err) {
      const msg = extractErrorMessage(err)
      if (msg.includes('Wallet mnemonic already exists')) {
        try {
          const existing = await director.getMnemonic()
          if (existing.join(' ') === words.join(' ')) {
            // The mnemonic in the DB already matches what they are trying to restore.
            // This happens if they generated it, went back without wiping, and pasted it here.
            localStorage.setItem('backupConfirmed', 'true')
            await onComplete()
            return
          }
        } catch (e) {
          console.error('Failed to get existing mnemonic to compare:', e)
        }
      }
      console.error('Error restoring mnemonic:', err)
      setError(msg)
    } finally {
      setIsLoading(false)
    }
  }

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(generatedMnemonic)
      setCopyFeedback('Copied!')
    } catch {
      setCopyFeedback('Failed to copy')
    } finally {
      setTimeout(() => setCopyFeedback(''), 2000)
    }
  }

  const handleWipe = (skipConfirm = false) => {
    if (
      !skipConfirm &&
      !window.confirm(
        'Are you sure you want to wipe all wallet data? This cannot be undone.',
      )
    )
      return
    localStorage.setItem('pendingWipe', 'true')
    window.location.reload()
  }

  if (step === 'welcome') {
    return (
      <div className="onboarding">
        <div className="onboarding-card">
          <h1>Welcome to Fedimint</h1>
          <p className="onboarding-subtitle">
            Set up your wallet to get started. You can create a new wallet or
            restore an existing one from a mnemonic phrase.
          </p>
          <div className="onboarding-actions">
            <button
              className="btn btn-primary btn-large"
              onClick={handleGenerate}
              disabled={isLoading}
            >
              {isLoading ? 'Generating...' : 'Create New Wallet'}
            </button>
            <button
              className="btn btn-secondary btn-large"
              onClick={() => setStep('restore')}
              disabled={isLoading}
            >
              Restore from Mnemonic
            </button>
          </div>
          {error && (
            <div className="onboarding-error">
              {error}
              <div style={{ marginTop: '0.75rem', textAlign: 'center' }}>
                <button
                  className="btn btn-small"
                  onClick={() => handleWipe()}
                  style={{
                    backgroundColor: 'transparent',
                    color: '#ff4444',
                    border: '1px solid #ff4444',
                  }}
                >
                  Wipe Data & Reset
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    )
  }

  if (step === 'generate') {
    return (
      <div className="onboarding">
        <div className="onboarding-card">
          <h2>Your Recovery Phrase</h2>
          <p className="onboarding-subtitle">
            Write down these words in order and store them somewhere safe. This
            is the only way to recover your wallet.
          </p>

          <div className="mnemonic-display-grid">
            <div className={`mnemonic-words ${showMnemonic ? '' : 'blurred'}`}>
              {generatedMnemonic.split(' ').map((word, i) => (
                <div key={i} className="mnemonic-word">
                  <span className="word-index">{i + 1}.</span>
                  <span>{word}</span>
                </div>
              ))}
            </div>
            <div className="mnemonic-controls">
              <button
                className="btn btn-small"
                onClick={() => setShowMnemonic(!showMnemonic)}
              >
                {showMnemonic ? '🙈 Hide' : '👁️ Reveal'}
              </button>
              <button
                className="btn btn-small"
                onClick={copyToClipboard}
                disabled={!showMnemonic || !!copyFeedback}
              >
                📋 Copy
              </button>
              {copyFeedback && (
                <span className="copy-feedback">{copyFeedback}</span>
              )}
            </div>
          </div>

          <label className="backup-checkbox">
            <input
              type="checkbox"
              checked={backedUp}
              onChange={(e) => setBackedUp(e.target.checked)}
            />
            I have written down my recovery phrase and stored it safely
          </label>

          <div className="onboarding-actions">
            <button
              className="btn btn-primary btn-large"
              onClick={handleConfirmGenerated}
              disabled={!backedUp || isLoading}
            >
              {isLoading ? 'Setting up...' : 'Continue'}
            </button>
            <button
              className="btn btn-secondary"
              onClick={() => {
                // generateMnemonic saves to the DB immediately. Going back requires
                // a full reload to wipe the saved key, otherwise the user is stuck.
                if (
                  window.confirm(
                    'Going back requires a full app reload to clear the generated key. Proceed?',
                  )
                ) {
                  handleWipe(true)
                }
              }}
            >
              ← Back
            </button>
          </div>
          {error && (
            <div className="onboarding-error">
              {error}
              <div style={{ marginTop: '0.75rem', textAlign: 'center' }}>
                <button
                  className="btn btn-small"
                  onClick={() => handleWipe()}
                  style={{
                    backgroundColor: 'transparent',
                    color: '#ff4444',
                    border: '1px solid #ff4444',
                  }}
                >
                  Wipe Data & Reset
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    )
  }

  // step === 'restore'
  return (
    <div className="onboarding">
      <div className="onboarding-card">
        <h2>Restore Wallet</h2>
        <p className="onboarding-subtitle">
          Enter your 12 or 24 word recovery phrase to restore your wallet.
        </p>

        <form onSubmit={handleRestore}>
          <textarea
            className="mnemonic-textarea"
            placeholder="Enter your recovery phrase (12 or 24 words separated by spaces)"
            value={inputMnemonic}
            onChange={(e) => {
              setInputMnemonic(e.target.value)
              setError('')
            }}
            rows={3}
          />
          <div className="onboarding-actions">
            <button
              className="btn btn-primary btn-large"
              type="submit"
              disabled={isLoading || !inputMnemonic.trim()}
            >
              {isLoading ? 'Restoring...' : 'Restore Wallet'}
            </button>
            <button
              className="btn btn-secondary"
              type="button"
              onClick={() => {
                setStep('welcome')
                setInputMnemonic('')
                setError('')
              }}
            >
              ← Back
            </button>
          </div>
          {error && (
            <div className="onboarding-error">
              {error}
              <div style={{ marginTop: '0.75rem', textAlign: 'center' }}>
                <button
                  className="btn btn-small"
                  type="button"
                  onClick={() => handleWipe()}
                  style={{
                    backgroundColor: 'transparent',
                    color: '#ff4444',
                    border: '1px solid #ff4444',
                  }}
                >
                  Wipe Data & Reset
                </button>
              </div>
            </div>
          )}
        </form>
      </div>
    </div>
  )
}

const WalletStatus = ({
  open,
  checkIsOpen,
  balance,
}: {
  open: boolean
  checkIsOpen: () => void
  balance: number
}) => {
  return (
    <div className="section">
      <h3>Wallet Status</h3>
      <div className="row">
        <strong>Is Wallet Open?</strong>
        <div>{open ? 'Yes' : 'No'}</div>
        <button onClick={() => checkIsOpen()}>Check</button>
      </div>
      <div className="row">
        <strong>Balance:</strong>
        <div className="balance">{balance}</div>
        sats
      </div>
    </div>
  )
}

const JoinFederation = ({
  open,
  checkIsOpen,
}: {
  open: boolean
  checkIsOpen: () => void
}) => {
  const wallet = useWallet()
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
      console.log('Preview federation:', data)
    } catch (error) {
      console.error('Error previewing federation:', error)
      setJoinError(error instanceof Error ? error.message : String(error))
      setPreviewData(null)
    } finally {
      setPreviewing(false)
    }
  }

  const joinFederation = async (e: React.FormEvent) => {
    e.preventDefault()
    checkIsOpen()

    console.log('Joining federation:', inviteCode)
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      setJoining(true)
      const res = await wallet.joinFederation(inviteCode)
      console.log('join federation res', res)
      setJoinResult('Joined!')
      setJoinError('')
    } catch (e: any) {
      console.log('Error joining federation', e)
      setJoinError(typeof e === 'object' ? e.toString() : (e as string))
      setJoinResult('')
    } finally {
      setJoining(false)
    }
  }

  return (
    <div className="section">
      <h3>Join Federation</h3>
      <form onSubmit={joinFederation} className="row">
        <input
          className="ecash-input"
          placeholder="Invite Code..."
          required
          value={inviteCode}
          onChange={(e) => {
            setInviteCode(e.target.value)
            setPreviewData(null)
          }}
          disabled={open}
        />
        <button
          type="button"
          onClick={previewFederationHandler}
          disabled={previewing || !inviteCode.trim() || open}
        >
          {previewing ? 'Previewing...' : 'Preview'}
        </button>
        <button type="submit" disabled={open || joining}>
          {joining ? 'Joining...' : 'Join'}
        </button>
      </form>

      {previewData && (
        <div className="preview-result">
          <h4>Federation Preview</h4>
          <div className="preview-info">
            <div className="preview-row">
              <strong>Federation ID:</strong>
              <code className="id">{previewData.federation_id}</code>
            </div>
            <div className="preview-row">
              <strong>Name:</strong>
              <span>
                {previewData.config.global.meta?.federation_name || 'Unnamed'}
              </span>
            </div>
            <div className="preview-row">
              <strong>Consensus Version:</strong>
              <span>
                {previewData.config.global.consensus_version.major}.
                {previewData.config.global.consensus_version.minor}
              </span>
            </div>
            <div className="preview-row">
              <strong>Guardians:</strong>
              <span>
                {Object.keys(previewData.config.global.api_endpoints).length}
              </span>
            </div>

            <details className="preview-details">
              <summary>Guardian Endpoints</summary>
              <div className="guardian-list">
                {Object.entries(
                  previewData.config.global.api_endpoints as Record<
                    string,
                    any
                  >,
                ).map(([id, peer]) => (
                  <div key={id} className="guardian-item">
                    <div>
                      <strong>{peer.name}</strong>
                    </div>
                    <div className="url">{peer.url}</div>
                  </div>
                ))}
              </div>
            </details>

            <details className="preview-details">
              <summary>Module Configuration</summary>
              <div className="module-list">
                {Object.entries(
                  previewData.config.modules as Record<string, any>,
                ).map(([id, module]) => (
                  <div key={id} className="module-item">
                    <strong>{module.kind}</strong>
                  </div>
                ))}
              </div>
            </details>

            <details className="preview-details">
              <summary>Full JSON</summary>
              <pre>{JSON.stringify(previewData, null, 2)}</pre>
            </details>
          </div>
        </div>
      )}

      {!joinResult && open && <i>(You've already joined a federation)</i>}
      {joinResult && <div className="success">{joinResult}</div>}
      {joinError && <div className="error">{joinError}</div>}
    </div>
  )
}

const RedeemEcash = () => {
  const wallet = useWallet()
  const [ecashInput, setEcashInput] = useState('')
  const [redeemResult, setRedeemResult] = useState('')
  const [redeemError, setRedeemError] = useState('')

  const handleRedeem = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const res = await wallet.mint.redeemEcash(ecashInput)
      console.log('redeem ecash res', res)
      setRedeemResult('Redeemed!')
      setRedeemError('')
    } catch (e) {
      console.log('Error redeeming ecash', e)
      setRedeemError(e as string)
      setRedeemResult('')
    }
  }

  return (
    <div className="section">
      <h3>Redeem Ecash</h3>
      <form onSubmit={handleRedeem} className="row">
        <input
          placeholder="Long ecash string..."
          required
          value={ecashInput}
          onChange={(e) => setEcashInput(e.target.value)}
        />
        <button type="submit">redeem</button>
      </form>
      {redeemResult && <div className="success">{redeemResult}</div>}
      {redeemError && <div className="error">{redeemError}</div>}
    </div>
  )
}

const SendLightning = () => {
  const wallet = useWallet()
  const [lightningInput, setLightningInput] = useState('')
  const [lightningResult, setLightningResult] = useState('')
  const [lightningError, setLightningError] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      await wallet.lightning.payInvoice(lightningInput)
      setLightningResult('Paid!')
      setLightningError('')
    } catch (e) {
      console.log('Error paying lightning', e)
      setLightningError(e as string)
      setLightningResult('')
    }
  }

  return (
    <div className="section">
      <h3>Pay Lightning</h3>
      <form onSubmit={handleSubmit} className="row">
        <input
          placeholder="lnbc..."
          required
          value={lightningInput}
          onChange={(e) => setLightningInput(e.target.value)}
        />
        <button type="submit">pay</button>
      </form>
      {lightningResult && <div className="success">{lightningResult}</div>}
      {lightningError && <div className="error">{lightningError}</div>}
    </div>
  )
}

const GenerateLightningInvoice = () => {
  const wallet = useWallet()
  const [amount, setAmount] = useState('')
  const [description, setDescription] = useState('')
  const [invoice, setInvoice] = useState('')
  const [error, setError] = useState('')
  const [generating, setGenerating] = useState(false)

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
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
      console.error('Error generating Lightning invoice', e)
      setError(e instanceof Error ? e.message : String(e))
    } finally {
      setGenerating(false)
    }
  }

  return (
    <div className="section">
      <h3>Generate Lightning Invoice</h3>
      <form onSubmit={handleSubmit}>
        <div className="input-group">
          <label htmlFor="amount">Amount (msats):</label>
          <input
            id="amount"
            type="number"
            placeholder="Enter amount in msats"
            required
            value={amount}
            onChange={(e) => setAmount(e.target.value)}
          />
        </div>
        <div className="input-group">
          <label htmlFor="description">Description:</label>
          <input
            id="description"
            placeholder="Enter description"
            required
            value={description}
            onChange={(e) => setDescription(e.target.value)}
          />
        </div>
        <button type="submit" disabled={generating}>
          {generating ? 'Generating...' : 'Generate Invoice'}
        </button>
      </form>
      <div>
        mutinynet faucet:{' '}
        <a href="https://faucet.mutinynet.com/" target="_blank">
          https://faucet.mutinynet.com/
        </a>
      </div>
      {invoice && (
        <div className="success">
          <strong>Generated Invoice:</strong>
          <pre className="invoice-wrap">{invoice}</pre>
          <button onClick={() => navigator.clipboard.writeText(invoice)}>
            Copy
          </button>
        </div>
      )}
      {error && <div className="error">{error}</div>}
    </div>
  )
}

const InviteCodeParser = () => {
  const [inviteCode, setInviteCode] = useState('')
  const [parseResult, setParseResult] = useState<ParsedInviteCode | null>(null)
  const [parseError, setParseError] = useState('')
  const [parsingStatus, setParsingStatus] = useState(false)

  const handleParse = async (e: React.FormEvent) => {
    e.preventDefault()
    setParseResult(null)
    setParseError('')
    setParsingStatus(true)

    try {
      const result = await director.parseInviteCode(inviteCode)
      setParseResult(result)
    } catch (e) {
      console.error('Error parsing invite code', e)
      setParseError(e instanceof Error ? e.message : String(e))
    } finally {
      setParsingStatus(false)
    }
  }

  return (
    <div className="section">
      <h3>Parse Invite Code</h3>
      <form onSubmit={handleParse} className="row">
        <input
          placeholder="Enter invite code..."
          value={inviteCode}
          onChange={(e) => setInviteCode(e.target.value)}
          required
        />
        <button type="submit" disabled={parsingStatus}>
          {parsingStatus ? 'Parsing...' : 'Parse'}
        </button>
      </form>
      {parseResult && (
        <div className="success">
          <div className="row">
            <strong>Fed Id:</strong>
            <div className="id">{parseResult.federation_id}</div>
          </div>
          <div className="row">
            <strong>Fed url:</strong>
            <div className="url">{parseResult.url}</div>
          </div>
        </div>
      )}
      {parseError && <div className="error">{parseError}</div>}
    </div>
  )
}

const ParseLightningInvoice = () => {
  const [invoiceStr, setInvoiceStr] = useState('')
  const [parseResult, setParseResult] = useState<ParsedBolt11Invoice | null>(
    null,
  )
  const [parseError, setParseError] = useState('')
  const [parsingStatus, setParsingStatus] = useState(false)

  const handleParse = async (e: React.FormEvent) => {
    e.preventDefault()
    setParseResult(null)
    setParseError('')
    setParsingStatus(true)

    try {
      const result = await director.parseBolt11Invoice(invoiceStr)
      console.log('result ', result)
      setParseResult(result)
    } catch (e) {
      console.error('Error parsing invite code', e)
      setParseError(e instanceof Error ? e.message : String(e))
    } finally {
      setParsingStatus(false)
    }
  }

  return (
    <div className="section">
      <h3>Parse Lightning Invoice</h3>
      <form onSubmit={handleParse} className="row">
        <input
          placeholder="Enter invoice..."
          value={invoiceStr}
          onChange={(e) => setInvoiceStr(e.target.value)}
          required
        />
        <button type="submit" disabled={parsingStatus}>
          {parsingStatus ? 'Parsing...' : 'Parse'}
        </button>
      </form>
      {parseResult && (
        <div className="success">
          <div className="row">
            <strong>Amount :</strong>
            <div className="id">{parseResult.amount}</div>
            sats
          </div>
          <div className="row">
            <strong>Expiry :</strong>
            <div className="url">{parseResult.expiry}</div>
          </div>
          <div className="row">
            <strong>Memo :</strong>
            <div className="url">{parseResult.memo}</div>
          </div>
        </div>
      )}
      {parseError && <div className="error">{parseError}</div>}
    </div>
  )
}

const Deposit = () => {
  const wallet = useWallet()
  const [address, setAddress] = useState<string>('')
  const [addressError, setAddressError] = useState('')
  const [addressStatus, setAddressStatus] = useState(false)

  const handleGenerateAddress = async (e: React.FormEvent) => {
    e.preventDefault()
    setAddressStatus(true)
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const result = await wallet.wallet.generateAddress()
      result && setAddress(result.deposit_address)
    } catch (e) {
      console.error('Error', e)
      setAddressError(e instanceof Error ? e.message : String(e))
    } finally {
      setAddressStatus(false)
    }
  }
  return (
    <div className="section">
      <h3>Generate Deposit Address</h3>
      <form onSubmit={handleGenerateAddress} className="row">
        <button type="submit" disabled={addressStatus}>
          {addressStatus ? 'Generating...' : 'Generate'}
        </button>
      </form>
      {address && (
        <div className="success">
          <p>{address}</p>
        </div>
      )}
      {addressError && <div className="error">{addressError}</div>}
    </div>
  )
}

const SendOnchain = () => {
  const wallet = useWallet()
  const [address, setAddress] = useState('')
  const [amount, setAmount] = useState(0)
  const [withdrawalResult, setWithdrawalResult] = useState('')
  const [withdrawalError, setWithdrawalError] = useState('')
  const [withdrawalStatus, setWithdrawalStatus] = useState(false)

  const handleWithdraw = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      setWithdrawalStatus(true)
      if (!wallet) throw new Error('Wallet unavailable')
      const result = await wallet.wallet.sendOnchain(amount, address)
      result && setWithdrawalResult(result.operation_id)
    } catch (e) {
      console.error('Error ', e)
      setWithdrawalError(e instanceof Error ? e.message : String(e))
    } finally {
      setWithdrawalStatus(false)
    }
  }
  return (
    <div className="section">
      <h3>Send Onchain</h3>
      <form onSubmit={handleWithdraw} className="row">
        <input
          placeholder="Enter amount"
          type="number"
          value={amount}
          onChange={(e) => setAmount(Number(e.target.value))}
          required
        />
        <input
          placeholder="Enter onchain address"
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          required
        />
        <button type="submit" disabled={withdrawalStatus}>
          {withdrawalStatus ? 'Sending' : 'Send'}
        </button>
      </form>
      {withdrawalResult && (
        <div className="success">
          <p>Onchain Send Successful</p>
        </div>
      )}
      {withdrawalError && <div className="error">{withdrawalError}</div>}
    </div>
  )
}

export default App
