import React, { useCallback, useEffect, useState } from 'react'
import { wallet, director } from './wallet'
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
    if (wallet && open !== wallet?.isOpen()) {
      setIsOpen(wallet.isOpen())
    }
  }, [open])

  useEffect(() => {
    checkIsOpen()
  }, [checkIsOpen])

  return { open, checkIsOpen }
}

const useBalance = (checkIsOpen: () => void) => {
  const [balance, setBalance] = useState(0)

  useEffect(() => {
    const unsubscribe = wallet?.balance.subscribeBalance((balance) => {
      // checks if the wallet is open when the first
      // subscription event fires.
      // TODO: make a subscription to the wallet open status
      checkIsOpen()
      setBalance(balance)
    })

    return () => {
      unsubscribe?.()
    }
  }, [checkIsOpen])

  return balance
}

const App = () => {
  const { open, checkIsOpen } = useIsOpen()
  const balance = useBalance(checkIsOpen)

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
        <MnemonicManager />
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

const MnemonicManager = () => {
  const [mnemonicState, setMnemonicState] = useState<string>('')
  const [inputMnemonic, setInputMnemonic] = useState<string>('')
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

  // Helper function to extract user-friendly error messages
  const extractErrorMessage = (error: any): string => {
    let errorMsg = 'Operation failed'

    if (error instanceof Error) {
      errorMsg = error.message
    } else if (typeof error === 'object' && error !== null) {
      // Handle RPC error objects
      const rpcError = error as any
      if (rpcError.error) {
        errorMsg = rpcError.error
      } else if (rpcError.message) {
        errorMsg = rpcError.message
      }
    }

    return errorMsg
  }

  const handleAction = async (action: 'get' | 'set' | 'generate') => {
    if (activeAction === action) {
      setActiveAction(null)
      return
    }
    setActiveAction(action)
    clearMessage()

    if (action === 'get') {
      await handleGetMnemonic()
    } else if (action === 'generate') {
      await handleGenerateMnemonic()
    }
  }

  const handleGenerateMnemonic = async () => {
    setIsLoading(true)
    try {
      const newMnemonic = await director.generateMnemonic()
      setMnemonicState(newMnemonic.join(' '))
      setMessage({ text: 'New mnemonic generated!', type: 'success' })
      setShowMnemonic(true)
    } catch (error) {
      console.error('Error generating mnemonic:', error)
      const errorMsg = extractErrorMessage(error)
      setMessage({ text: errorMsg, type: 'error' })
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
      console.error('Error getting mnemonic:', error)
      const errorMsg = extractErrorMessage(error)
      setMessage({ text: errorMsg, type: 'error' })
    } finally {
      setIsLoading(false)
    }
  }

  const handleSetMnemonic = async (e: React.FormEvent) => {
    e.preventDefault()
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
      console.error('Error setting mnemonic:', error)
      const errorMsg = extractErrorMessage(error)
      setMessage({ text: errorMsg, type: 'error' })
    } finally {
      setIsLoading(false)
    }
  }

  const copyToClipboard = async () => {
    try {
      await navigator.clipboard.writeText(mnemonicState)
      setMessage({ text: 'Copied to clipboard!', type: 'success' })
    } catch (error) {
      setMessage({ text: 'Failed to copy', type: 'error' })
    }
  }

  return (
    <div className="section mnemonic-section">
      <h3>🔑 Mnemonic Manager</h3>

      <div className="mnemonic-buttons">
        <button
          onClick={() => handleAction('get')}
          disabled={isLoading}
          className={`btn ${activeAction === 'get' ? 'active' : ''}`}
        >
          Get
        </button>
        <button
          onClick={() => handleAction('set')}
          disabled={isLoading}
          className={`btn ${activeAction === 'set' ? 'active' : ''}`}
        >
          Set
        </button>
        <button
          onClick={() => handleAction('generate')}
          disabled={isLoading}
          className={`btn ${activeAction === 'generate' ? 'active' : ''}`}
        >
          Generate
        </button>
      </div>

      {activeAction === 'set' && (
        <form onSubmit={handleSetMnemonic} className="mnemonic-form">
          <textarea
            placeholder="Enter 12 or 24 words separated by spaces"
            value={inputMnemonic}
            onChange={(e) => setInputMnemonic(e.target.value)}
            rows={2}
            className="mnemonic-input"
          />
          <button
            type="submit"
            disabled={isLoading || !inputMnemonic.trim()}
            className="btn btn-primary"
          >
            {isLoading ? 'Setting...' : 'Set Mnemonic'}
          </button>
        </form>
      )}

      {mnemonicState && (
        <div className="mnemonic-display">
          <div className="mnemonic-output">
            <span className={showMnemonic ? '' : 'blurred'}>
              {mnemonicState}
            </span>
            <div className="mnemonic-actions">
              <button
                onClick={() => setShowMnemonic(!showMnemonic)}
                className="btn btn-small"
                title={showMnemonic ? 'Hide mnemonic' : 'Show mnemonic'}
              >
                {showMnemonic ? '👁️' : '👁️‍🗨️'}
              </button>
              <button
                onClick={copyToClipboard}
                className="btn btn-small"
                disabled={!showMnemonic}
                title="Copy to clipboard"
              >
                📋
              </button>
            </div>
          </div>
        </div>
      )}

      {message && (
        <div className={`message ${message.type}`}>{message.text}</div>
      )}
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
                {Object.entries(previewData.config.global.api_endpoints).map(
                  ([id, peer]) => (
                    <div key={id} className="guardian-item">
                      <div>
                        <strong>{peer.name}</strong>
                      </div>
                      <div className="url">{peer.url}</div>
                    </div>
                  ),
                )}
              </div>
            </details>

            <details className="preview-details">
              <summary>Module Configuration</summary>
              <div className="module-list">
                {Object.entries(previewData.config.modules).map(
                  ([id, module]) => (
                    <div key={id} className="module-item">
                      <strong>{module.kind}</strong>
                    </div>
                  ),
                )}
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
