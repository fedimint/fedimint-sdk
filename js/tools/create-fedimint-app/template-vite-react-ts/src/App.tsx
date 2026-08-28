import { useCallback, useEffect, useState } from 'react'
import { wallet } from './wallet'

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

// Expose the wallet to the global window object for testing
// @ts-ignore
globalThis.wallet = wallet

const useIsOpen = () => {
  const [open, setIsOpen] = useState(false)

  const checkIsOpen = useCallback(() => {
    if (open !== wallet.isOpen()) {
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
    const unsubscribe = wallet.balance.subscribeBalance((balance) => {
      // checks if the wallet is open when the first
      // subscription event fires.
      // TODO: make a subscription to the wallet open status
      checkIsOpen()
      setBalance(balance)
    })

    return () => {
      unsubscribe()
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
        <JoinFederation open={open} checkIsOpen={checkIsOpen} />
        <GenerateLightningInvoice />
        <RedeemEcash />
        <SendLightning />
        <InviteCodeParser />
        <ParseLightningInvoice />
      </main>
    </>
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

const InviteCodeParser = () => {
  const [inviteCode, setInviteCode] = useState('')
  const [parseResult, setParseResult] = useState<any>(null)
  const [parseError, setParseError] = useState('')
  const [parsingStatus, setParsingStatus] = useState(false)

  const handleParse = async (e: React.FormEvent) => {
    e.preventDefault()
    setParseResult(null)
    setParseError('')
    setParsingStatus(true)

    try {
      const result = await wallet.parseInviteCode(inviteCode)
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
  const [parseResult, setParseResult] = useState<any>(null)
  const [parseError, setParseError] = useState('')
  const [parsingStatus, setParsingStatus] = useState(false)

  const handleParse = async (e: React.FormEvent) => {
    e.preventDefault()
    setParseResult(null)
    setParseError('')
    setParsingStatus(true)

    try {
      const result = await wallet.parseBolt11Invoice(invoiceStr)
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

export default App
