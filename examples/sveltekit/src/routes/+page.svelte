<script lang="ts">
  import { wallet, director } from '../wallet'
  import type {
    ParsedInviteCode,
    ParsedBolt11Invoice,
    PreviewFederation,
  } from '@fedimint/core'

  const TESTNET_FEDERATION_CODE =
    'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

  // --- Wallet Status ---
  let isOpen = $state(false)
  let balance = $state(0)

  function checkIsOpen() {
    if (wallet) {
      isOpen = wallet.isOpen()
    }
  }

  $effect(() => {
    checkIsOpen()
    const unsubscribe = wallet?.balance.subscribeBalance((bal) => {
      checkIsOpen()
      balance = bal
    })
    return () => {
      unsubscribe?.()
    }
  })

  // --- Mnemonic Manager ---
  let mnemonicState = $state('')
  let inputMnemonic = $state('')
  let activeAction = $state<'get' | 'set' | 'generate' | null>(null)
  let mnemonicLoading = $state(false)
  let mnemonicMessage = $state<{
    text: string
    type: 'success' | 'error'
  } | null>(null)
  let showMnemonic = $state(false)

  function extractErrorMessage(error: any): string {
    let errorMsg = 'Operation failed'
    if (error instanceof Error) {
      errorMsg = error.message
    } else if (typeof error === 'object' && error !== null) {
      const rpcError = error as any
      if (rpcError.error) {
        errorMsg = rpcError.error
      } else if (rpcError.message) {
        errorMsg = rpcError.message
      }
    }
    return errorMsg
  }

  async function handleMnemonicAction(action: 'get' | 'set' | 'generate') {
    if (activeAction === action) {
      activeAction = null
      return
    }
    activeAction = action
    mnemonicMessage = null

    if (action === 'get') {
      await handleGetMnemonic()
    } else if (action === 'generate') {
      await handleGenerateMnemonic()
    }
  }

  async function handleGenerateMnemonic() {
    mnemonicLoading = true
    try {
      const newMnemonic = await director.generateMnemonic()
      mnemonicState = newMnemonic.join(' ')
      mnemonicMessage = { text: 'New mnemonic generated!', type: 'success' }
      showMnemonic = true
    } catch (error) {
      console.error('Error generating mnemonic:', error)
      mnemonicMessage = { text: extractErrorMessage(error), type: 'error' }
    } finally {
      mnemonicLoading = false
    }
  }

  async function handleGetMnemonic() {
    mnemonicLoading = true
    try {
      const mnemonic = await director.getMnemonic()
      if (mnemonic && mnemonic.length > 0) {
        mnemonicState = mnemonic.join(' ')
        mnemonicMessage = { text: 'Mnemonic retrieved!', type: 'success' }
        showMnemonic = true
      } else {
        mnemonicMessage = { text: 'No mnemonic found', type: 'error' }
      }
    } catch (error) {
      console.error('Error getting mnemonic:', error)
      mnemonicMessage = { text: extractErrorMessage(error), type: 'error' }
    } finally {
      mnemonicLoading = false
    }
  }

  async function handleSetMnemonic() {
    if (!inputMnemonic.trim()) return
    mnemonicLoading = true
    try {
      const words = inputMnemonic.trim().split(/\s+/)
      await director.setMnemonic(words)
      mnemonicMessage = { text: 'Mnemonic set successfully!', type: 'success' }
      inputMnemonic = ''
      mnemonicState = words.join(' ')
      activeAction = null
    } catch (error) {
      console.error('Error setting mnemonic:', error)
      mnemonicMessage = { text: extractErrorMessage(error), type: 'error' }
    } finally {
      mnemonicLoading = false
    }
  }

  async function copyMnemonicToClipboard() {
    try {
      await navigator.clipboard.writeText(mnemonicState)
      mnemonicMessage = { text: 'Copied to clipboard!', type: 'success' }
    } catch {
      mnemonicMessage = { text: 'Failed to copy', type: 'error' }
    }
  }

  // --- Join Federation ---
  let inviteCode = $state(TESTNET_FEDERATION_CODE)
  let joinResult = $state('')
  let joinError = $state('')
  let joining = $state(false)
  let previewData = $state<PreviewFederation | null>(null)
  let previewing = $state(false)

  async function previewFederation() {
    if (!inviteCode.trim()) return
    previewing = true
    joinError = ''
    try {
      previewData = await director.previewFederation(inviteCode)
      console.log('Preview federation:', previewData)
    } catch (e) {
      console.error('Error previewing federation:', e)
      joinError = e instanceof Error ? e.message : String(e)
      previewData = null
    } finally {
      previewing = false
    }
  }

  async function joinFederation() {
    checkIsOpen()
    console.log('Joining federation:', inviteCode)
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      joining = true
      const res = await wallet.joinFederation(inviteCode)
      console.log('join federation res', res)
      joinResult = 'Joined!'
      joinError = ''
    } catch (e: any) {
      console.log('Error joining federation', e)
      joinError = typeof e === 'object' ? e.toString() : (e as string)
      joinResult = ''
    } finally {
      joining = false
    }
  }

  // --- Lightning Invoice ---
  let invoiceAmount = $state('')
  let invoiceDescription = $state('')
  let invoice = $state('')
  let invoiceError = $state('')
  let generating = $state(false)

  async function generateInvoice() {
    invoice = ''
    invoiceError = ''
    generating = true
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const response = await wallet.lightning.createInvoice(
        Number(invoiceAmount),
        invoiceDescription,
      )
      if (response) invoice = response.invoice
    } catch (e) {
      console.error('Error generating Lightning invoice', e)
      invoiceError = e instanceof Error ? e.message : String(e)
    } finally {
      generating = false
    }
  }

  // --- Redeem Ecash ---
  let ecashInput = $state('')
  let redeemResult = $state('')
  let redeemError = $state('')

  async function redeemEcash() {
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const res = await wallet.mint.redeemEcash(ecashInput)
      console.log('redeem ecash res', res)
      redeemResult = 'Redeemed!'
      redeemError = ''
    } catch (e) {
      console.log('Error redeeming ecash', e)
      redeemError = String(e)
      redeemResult = ''
    }
  }

  // --- Pay Lightning ---
  let lightningInput = $state('')
  let lightningResult = $state('')
  let lightningError = $state('')

  async function payLightning() {
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      await wallet.lightning.payInvoice(lightningInput)
      lightningResult = 'Paid!'
      lightningError = ''
    } catch (e) {
      console.log('Error paying lightning', e)
      lightningError = String(e)
      lightningResult = ''
    }
  }

  // --- Parse Invite Code ---
  let parseInviteInput = $state('')
  let parseInviteResult = $state<ParsedInviteCode | null>(null)
  let parseInviteError = $state('')
  let parsingInvite = $state(false)

  async function parseInviteCode() {
    parseInviteResult = null
    parseInviteError = ''
    parsingInvite = true
    try {
      const result = await director.parseInviteCode(parseInviteInput)
      parseInviteResult = result
    } catch (e) {
      console.error('Error parsing invite code', e)
      parseInviteError = e instanceof Error ? e.message : String(e)
    } finally {
      parsingInvite = false
    }
  }

  // --- Parse Lightning Invoice ---
  let parseInvoiceInput = $state('')
  let parseInvoiceResult = $state<ParsedBolt11Invoice | null>(null)
  let parseInvoiceError = $state('')
  let parsingInvoice = $state(false)

  async function parseLightningInvoice() {
    parseInvoiceResult = null
    parseInvoiceError = ''
    parsingInvoice = true
    try {
      const result = await director.parseBolt11Invoice(parseInvoiceInput)
      console.log('result ', result)
      parseInvoiceResult = result
    } catch (e) {
      console.error('Error parsing invoice', e)
      parseInvoiceError = e instanceof Error ? e.message : String(e)
    } finally {
      parsingInvoice = false
    }
  }

  // --- Deposit ---
  let depositAddress = $state('')
  let addressError = $state('')
  let addressLoading = $state(false)

  async function generateAddress() {
    addressLoading = true
    try {
      if (!wallet) throw new Error('Wallet unavailable')
      const result = await wallet.wallet.generateAddress()
      if (result) depositAddress = result.deposit_address
    } catch (e) {
      console.error('Error', e)
      addressError = e instanceof Error ? e.message : String(e)
    } finally {
      addressLoading = false
    }
  }

  // --- Send Onchain ---
  let onchainAddress = $state('')
  let onchainAmount = $state(0)
  let onchainResult = $state('')
  let onchainError = $state('')
  let onchainLoading = $state(false)

  async function sendOnchain() {
    try {
      onchainLoading = true
      if (!wallet) throw new Error('Wallet unavailable')
      const result = await wallet.wallet.sendOnchain(
        onchainAmount,
        onchainAddress,
      )
      if (result) onchainResult = result.operation_id
    } catch (e) {
      console.error('Error ', e)
      onchainError = e instanceof Error ? e.message : String(e)
    } finally {
      onchainLoading = false
    }
  }
</script>

<header>
  <h1>Fedimint Typescript Library Demo</h1>

  <div class="steps">
    <strong>Steps to get started:</strong>
    <ol>
      <li>Join a Federation (persists across sessions)</li>
      <li>Generate an Invoice</li>
      <li>
        Pay the Invoice using the
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
  <!-- Wallet Status -->
  <div class="section">
    <h3>Wallet Status</h3>
    <div class="row">
      <strong>Is Wallet Open?</strong>
      <div>{isOpen ? 'Yes' : 'No'}</div>
      <button onclick={checkIsOpen}>Check</button>
    </div>
    <div class="row">
      <strong>Balance:</strong>
      <div class="balance">{balance}</div>
      sats
    </div>
  </div>

  <!-- Mnemonic Manager -->
  <div class="section mnemonic-section">
    <h3>🔑 Mnemonic Manager</h3>

    <div class="mnemonic-buttons">
      <button
        onclick={() => handleMnemonicAction('get')}
        disabled={mnemonicLoading}
        class="btn {activeAction === 'get' ? 'active' : ''}"
      >
        Get
      </button>
      <button
        onclick={() => handleMnemonicAction('set')}
        disabled={mnemonicLoading}
        class="btn {activeAction === 'set' ? 'active' : ''}"
      >
        Set
      </button>
      <button
        onclick={() => handleMnemonicAction('generate')}
        disabled={mnemonicLoading}
        class="btn {activeAction === 'generate' ? 'active' : ''}"
      >
        Generate
      </button>
    </div>

    {#if activeAction === 'set'}
      <form
        onsubmit={(e) => {
          e.preventDefault()
          handleSetMnemonic()
        }}
        class="mnemonic-form"
      >
        <textarea
          placeholder="Enter 12 or 24 words separated by spaces"
          bind:value={inputMnemonic}
          rows={2}
          class="mnemonic-input"
        ></textarea>
        <button
          type="submit"
          disabled={mnemonicLoading || !inputMnemonic.trim()}
          class="btn btn-primary"
        >
          {mnemonicLoading ? 'Setting...' : 'Set Mnemonic'}
        </button>
      </form>
    {/if}

    {#if mnemonicState}
      <div class="mnemonic-display">
        <div class="mnemonic-output">
          <span class={showMnemonic ? '' : 'blurred'}>
            {mnemonicState}
          </span>
          <div class="mnemonic-actions">
            <button
              onclick={() => (showMnemonic = !showMnemonic)}
              class="btn btn-small"
              title={showMnemonic ? 'Hide mnemonic' : 'Show mnemonic'}
            >
              {showMnemonic ? '👁️' : '👁️‍🗨️'}
            </button>
            <button
              onclick={copyMnemonicToClipboard}
              class="btn btn-small"
              disabled={!showMnemonic}
              title="Copy to clipboard"
            >
              📋
            </button>
          </div>
        </div>
      </div>
    {/if}

    {#if mnemonicMessage}
      <div class="message {mnemonicMessage.type}">{mnemonicMessage.text}</div>
    {/if}
  </div>

  <!-- Join Federation -->
  <div class="section">
    <h3>Join Federation</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        joinFederation()
      }}
      class="row"
    >
      <input
        class="ecash-input"
        placeholder="Invite Code..."
        required
        bind:value={inviteCode}
        disabled={isOpen}
      />
      <button
        type="button"
        onclick={previewFederation}
        disabled={previewing || !inviteCode.trim() || isOpen}
      >
        {previewing ? 'Previewing...' : 'Preview'}
      </button>
      <button type="submit" disabled={isOpen || joining}>
        {joining ? 'Joining...' : 'Join'}
      </button>
    </form>

    {#if previewData}
      <div class="preview-result">
        <h4>Federation Preview</h4>
        <div class="preview-info">
          <div>
            <strong>Federation ID:</strong>
            <code class="id">{previewData.federation_id}</code>
          </div>
          <div>
            <strong>Name:</strong>
            <span>
              {previewData.config.global.meta?.federation_name || 'Unnamed'}
            </span>
          </div>
          <div>
            <strong>Consensus Version:</strong>
            <span>
              {previewData.config.global.consensus_version.major}.{previewData
                .config.global.consensus_version.minor}
            </span>
          </div>
          <div>
            <strong>Guardians:</strong>
            <span>
              {Object.keys(previewData.config.global.api_endpoints).length}
            </span>
          </div>

          <details class="preview-details">
            <summary>Guardian Endpoints</summary>
            <div class="guardian-list">
              {#each Object.entries(previewData.config.global.api_endpoints) as [id, peer]}
                <div class="guardian-item">
                  <div><strong>{peer.name}</strong></div>
                  <div class="url">{peer.url}</div>
                </div>
              {/each}
            </div>
          </details>

          <details class="preview-details">
            <summary>Module Configuration</summary>
            <div class="module-list">
              {#each Object.entries(previewData.config.modules) as [id, mod]}
                <div class="module-item">
                  <strong>{mod.kind}</strong>
                </div>
              {/each}
            </div>
          </details>

          <details class="preview-details">
            <summary>Full JSON</summary>
            <pre>{JSON.stringify(previewData, null, 2)}</pre>
          </details>
        </div>
      </div>
    {/if}

    {#if !joinResult && isOpen}
      <i>(You've already joined a federation)</i>
    {/if}
    {#if joinResult}
      <div class="success">{joinResult}</div>
    {/if}
    {#if joinError}
      <div class="error">{joinError}</div>
    {/if}
  </div>

  <!-- Generate Lightning Invoice -->
  <div class="section">
    <h3>Generate Lightning Invoice</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        generateInvoice()
      }}
    >
      <div class="input-group">
        <label for="amount">Amount (msats):</label>
        <input
          id="amount"
          type="number"
          placeholder="Enter amount in msats"
          required
          bind:value={invoiceAmount}
        />
      </div>
      <div class="input-group">
        <label for="description">Description:</label>
        <input
          id="description"
          placeholder="Enter description"
          required
          bind:value={invoiceDescription}
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
    {#if invoice}
      <div class="success">
        <strong>Generated Invoice:</strong>
        <pre class="invoice-wrap">{invoice}</pre>
        <button onclick={() => navigator.clipboard.writeText(invoice)}>
          Copy
        </button>
      </div>
    {/if}
    {#if invoiceError}
      <div class="error">{invoiceError}</div>
    {/if}
  </div>

  <!-- Redeem Ecash -->
  <div class="section">
    <h3>Redeem Ecash</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        redeemEcash()
      }}
      class="row"
    >
      <input
        placeholder="Long ecash string..."
        required
        bind:value={ecashInput}
      />
      <button type="submit">redeem</button>
    </form>
    {#if redeemResult}
      <div class="success">{redeemResult}</div>
    {/if}
    {#if redeemError}
      <div class="error">{redeemError}</div>
    {/if}
  </div>

  <!-- Pay Lightning -->
  <div class="section">
    <h3>Pay Lightning</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        payLightning()
      }}
      class="row"
    >
      <input placeholder="lnbc..." required bind:value={lightningInput} />
      <button type="submit">pay</button>
    </form>
    {#if lightningResult}
      <div class="success">{lightningResult}</div>
    {/if}
    {#if lightningError}
      <div class="error">{lightningError}</div>
    {/if}
  </div>

  <!-- Parse Invite Code -->
  <div class="section">
    <h3>Parse Invite Code</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        parseInviteCode()
      }}
      class="row"
    >
      <input
        placeholder="Enter invite code..."
        bind:value={parseInviteInput}
        required
      />
      <button type="submit" disabled={parsingInvite}>
        {parsingInvite ? 'Parsing...' : 'Parse'}
      </button>
    </form>
    {#if parseInviteResult}
      <div class="success">
        <div class="row">
          <strong>Fed Id:</strong>
          <div class="id">{parseInviteResult.federation_id}</div>
        </div>
        <div class="row">
          <strong>Fed url:</strong>
          <div class="url">{parseInviteResult.url}</div>
        </div>
      </div>
    {/if}
    {#if parseInviteError}
      <div class="error">{parseInviteError}</div>
    {/if}
  </div>

  <!-- Parse Lightning Invoice -->
  <div class="section">
    <h3>Parse Lightning Invoice</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        parseLightningInvoice()
      }}
      class="row"
    >
      <input
        placeholder="Enter invoice..."
        bind:value={parseInvoiceInput}
        required
      />
      <button type="submit" disabled={parsingInvoice}>
        {parsingInvoice ? 'Parsing...' : 'Parse'}
      </button>
    </form>
    {#if parseInvoiceResult}
      <div class="success">
        <div class="row">
          <strong>Amount :</strong>
          <div class="id">{parseInvoiceResult.amount}</div>
          sats
        </div>
        <div class="row">
          <strong>Expiry :</strong>
          <div class="url">{parseInvoiceResult.expiry}</div>
        </div>
        <div class="row">
          <strong>Memo :</strong>
          <div class="url">{parseInvoiceResult.memo}</div>
        </div>
      </div>
    {/if}
    {#if parseInvoiceError}
      <div class="error">{parseInvoiceError}</div>
    {/if}
  </div>

  <!-- Generate Deposit Address -->
  <div class="section">
    <h3>Generate Deposit Address</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        generateAddress()
      }}
      class="row"
    >
      <button type="submit" disabled={addressLoading}>
        {addressLoading ? 'Generating...' : 'Generate'}
      </button>
    </form>
    {#if depositAddress}
      <div class="success">
        <p>{depositAddress}</p>
      </div>
    {/if}
    {#if addressError}
      <div class="error">{addressError}</div>
    {/if}
  </div>

  <!-- Send Onchain -->
  <div class="section">
    <h3>Send Onchain</h3>
    <form
      onsubmit={(e) => {
        e.preventDefault()
        sendOnchain()
      }}
      class="row"
    >
      <input
        placeholder="Enter amount"
        type="number"
        bind:value={onchainAmount}
        required
      />
      <input
        placeholder="Enter onchain address"
        bind:value={onchainAddress}
        required
      />
      <button type="submit" disabled={onchainLoading}>
        {onchainLoading ? 'Sending' : 'Send'}
      </button>
    </form>
    {#if onchainResult}
      <div class="success">
        <p>Onchain Send Successful</p>
      </div>
    {/if}
    {#if onchainError}
      <div class="error">{onchainError}</div>
    {/if}
  </div>
</main>
