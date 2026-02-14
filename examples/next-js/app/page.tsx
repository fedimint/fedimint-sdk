'use client'

import { SetStateAction, useCallback, useEffect, useState } from 'react'
import {
  director,
  getWallet,
  getWalletSync,
  initializeWallet,
} from '@/utils/wallet'

const TESTNET_FEDERATION_CODE =
  'fed11qgqzc2nhwden5te0vejkg6tdd9h8gepwvejkg6tdd9h8garhduhx6at5d9h8jmn9wshxxmmd9uqqzgxg6s3evnr6m9zdxr6hxkdkukexpcs3mn7mj3g5pc5dfh63l4tj6g9zk4er'

const useIsOpen = (walletReady: boolean) => {
  const [open, setIsOpen] = useState(false)

  const checkIsOpen = useCallback(() => {
    if (!walletReady) return
    const currentWallet = getWalletSync()
    if (!currentWallet) return
    if (open !== currentWallet.isOpen()) {
      setIsOpen(currentWallet.isOpen())
    }
  }, [open, walletReady])

  useEffect(() => {
    if (!walletReady) return
    checkIsOpen()
  }, [checkIsOpen, walletReady])

  return { open, checkIsOpen }
}

const useBalance = (checkIsOpen: () => void, walletReady: boolean) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    if (!walletReady) return
    const currentWallet = getWalletSync()
    if (!currentWallet) return
    const unsubscribe = currentWallet.balance.subscribeBalance(
      (balance: SetStateAction<number>) => {
        checkIsOpen()
        setBalance(balance)
      },
    )

    return () => {
      unsubscribe()
    }
  }, [checkIsOpen, walletReady])

  return balance
}

const App = () => {
  const [mnemonicStatus, setMnemonicStatus] = useState<
    'checking' | 'missing' | 'set'
  >('checking')
  const [walletReady, setWalletReady] = useState(false)

  const initialize = useCallback(async () => {
    await initializeWallet()
    const currentWallet = getWalletSync()
    if (currentWallet) {
      // @ts-ignore
      globalThis.wallet = currentWallet
    }
    setWalletReady(true)
  }, [initializeWallet, getWalletSync])

  useEffect(() => {
    if (!director) return
    let active = true
    director
      .hasMnemonicSet()
      .then((hasMnemonic) => {
        if (!active) return
        setMnemonicStatus(hasMnemonic ? 'set' : 'missing')
      })
      .catch((error) => {
        console.warn('Failed to check mnemonic status', error)
        if (!active) return
        setMnemonicStatus('missing')
      })

    return () => {
      active = false
    }
  }, [])

  useEffect(() => {
    if (mnemonicStatus === 'set') {
      initialize()
    }
  }, [initialize, mnemonicStatus])

  const { open, checkIsOpen } = useIsOpen(walletReady)
  const balance = useBalance(checkIsOpen, walletReady)

  if (!director || mnemonicStatus === 'checking') {
    return <LoadingScreen message="Checking wallet status..." />
  }

  if (mnemonicStatus === 'missing') {
    return (
      <Onboarding
        onComplete={() => {
          setMnemonicStatus('set')
        }}
      />
    )
  }

  if (!walletReady) {
    return <LoadingScreen message="Preparing your wallet..." />
  }

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
      </main>
    </>
  )
}

const LoadingScreen = ({ message }: { message: string }) => {
  return (
    <main className="onboarding-screen">
      <div className="section onboarding-card">
        <h3>{message}</h3>
        <p>Please wait a moment.</p>
      </div>
    </main>
  )
}

const Onboarding = ({ onComplete }: { onComplete: () => void }) => {
  const [inputMnemonic, setInputMnemonic] = useState('')
  const [generatedMnemonic, setGeneratedMnemonic] = useState<string | null>(
    null,
  )
  const [isLoading, setIsLoading] = useState(false)
  const [message, setMessage] = useState<{
    text: string
    type: 'success' | 'error'
  } | null>(null)

  const handleGenerate = async () => {
    if (!director) return
    setIsLoading(true)
    setMessage(null)
    try {
      const words = await director.generateMnemonic()
      setGeneratedMnemonic(words.join(' '))
      setMessage({
        text: 'Mnemonic generated. Please back it up before continuing.',
        type: 'success',
      })
    } catch (error) {
      console.error('Error generating mnemonic:', error)
      setMessage({
        text: error instanceof Error ? error.message : 'Failed to generate',
        type: 'error',
      })
    } finally {
      setIsLoading(false)
    }
  }

  const handleSet = async (e: React.FormEvent) => {
    e.preventDefault()
    if (!director || !inputMnemonic.trim()) return

    setIsLoading(true)
    setMessage(null)
    try {
      const words = inputMnemonic.trim().split(/\s+/)
      await director.setMnemonic(words)
      setMessage({ text: 'Mnemonic set successfully!', type: 'success' })
      onComplete()
    } catch (error) {
      console.error('Error setting mnemonic:', error)
      setMessage({
        text: error instanceof Error ? error.message : 'Failed to set mnemonic',
        type: 'error',
      })
    } finally {
      setIsLoading(false)
    }
  }

  const handleCopy = async () => {
    if (!generatedMnemonic) return
    try {
      await navigator.clipboard.writeText(generatedMnemonic)
      setMessage({ text: 'Copied to clipboard!', type: 'success' })
    } catch (error) {
      setMessage({ text: 'Failed to copy', type: 'error' })
    }
  }

  return (
    <main className="onboarding-screen">
      <div className="section onboarding-card">
        <h3>Welcome to Fedimint</h3>
        <p>
          To continue, set an existing mnemonic or generate a new one for this
          wallet.
        </p>

        <div className="onboarding-actions">
          <button onClick={handleGenerate} disabled={isLoading}>
            {isLoading ? 'Generating...' : 'Generate Mnemonic'}
          </button>
          {generatedMnemonic && (
            <button onClick={onComplete} disabled={isLoading}>
              Continue
            </button>
          )}
        </div>

        {generatedMnemonic && (
          <div className="mnemonic-output">
            <div className="mnemonic-text">{generatedMnemonic}</div>
            <div className="button-group">
              <button onClick={handleCopy} disabled={isLoading}>
                Copy
              </button>
            </div>
          </div>
        )}

        <form onSubmit={handleSet} className="mnemonic-form">
          <textarea
            className="mnemonic-input"
            placeholder="Enter 12 or 24 words separated by spaces"
            value={inputMnemonic}
            onChange={(e) => setInputMnemonic(e.target.value)}
            rows={3}
          />
          <button type="submit" disabled={isLoading || !inputMnemonic.trim()}>
            {isLoading ? 'Setting...' : 'Set Mnemonic'}
          </button>
        </form>

        {message && (
          <div className={message.type === 'error' ? 'error' : 'success'}>
            {message.text}
          </div>
        )}
      </div>
    </main>
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
  const [inviteCode, setInviteCode] = useState(TESTNET_FEDERATION_CODE)
  const [joinResult, setJoinResult] = useState<string | null>(null)
  const [joinError, setJoinError] = useState('')
  const [joining, setJoining] = useState(false)

  const joinFederation = async (e: React.FormEvent) => {
    e.preventDefault()
    checkIsOpen()

    console.log('Joining federation:', inviteCode)
    try {
      const wallet = await getWallet()
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
          onChange={(e) => setInviteCode(e.target.value)}
          disabled={open}
        />
        <button type="submit" disabled={open || joining}>
          Join
        </button>
      </form>
      {!joinResult && open && <i>(You've already joined a federation)</i>}
      {joinResult && <div className="success">{joinResult}</div>}
      {joinError && <div className="error">{joinError}</div>}
    </div>
  )
}

const RedeemEcash = () => {
  const [ecashInput, setEcashInput] = useState('')
  const [redeemResult, setRedeemResult] = useState('')
  const [redeemError, setRedeemError] = useState('')

  const handleRedeem = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      const wallet = await getWallet()
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
  const [lightningInput, setLightningInput] = useState('')
  const [lightningResult, setLightningResult] = useState('')
  const [lightningError, setLightningError] = useState('')

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      const wallet = await getWallet()
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
      const wallet = await getWallet()
      const response = await wallet.lightning.createInvoice(
        Number(amount),
        description,
      )
      setInvoice(response.invoice)
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
          <label htmlFor="amount">Amount (sats):</label>
          <input
            id="amount"
            type="number"
            placeholder="Enter amount"
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

export default App
