import './style.css' // Load CSS

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

let wallet
let director
let walletReady

// --- Screen Management ---
const showScreen = (screenId) => {
  document.getElementById('loading-screen').style.display = 'none'
  document.getElementById('onboarding-screen').style.display = 'none'
  document.getElementById('wallet-ui').style.display = 'none'
  document.getElementById(screenId).style.display =
    screenId === 'wallet-ui' ? 'block' : 'flex'
}

const showOnboardingStep = (stepId) => {
  document.getElementById('onboarding-welcome').style.display = 'none'
  document.getElementById('onboarding-generate').style.display = 'none'
  document.getElementById('onboarding-restore').style.display = 'none'
  document.getElementById(stepId).style.display = 'block'
}

const showError = (msg) => {
  const el = document.getElementById('onboarding-error')
  el.textContent = msg
  el.style.display = msg ? 'block' : 'none'
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

// --- Wallet UI Functions ---
const checkIsOpen = () => {
  let walletResult = document.getElementById('walletResult')
  if (wallet && wallet.isOpen() == true) {
    walletResult.innerHTML = 'Yes'
    getBalance()
  } else {
    walletResult.innerHTML = 'No'
  }
}

const getBalance = () => {
  let bal = document.getElementById('bal')
  wallet.balance.subscribeBalance((balance) => {
    bal.innerText = balance
  })
}

const joinFederation = async (event) => {
  event.preventDefault()
  let joinInput = document.getElementById('join-input')
  let joinResult = document.getElementById('joinResult')
  try {
    await wallet.joinFederation(joinInput.value || TESTNET_FEDERATION_CODE)
    joinResult.innerHTML = 'Joined!'
    joinResult.style.color = 'green'
  } catch (e) {
    joinResult.innerHTML = `Error: ${e}`
    joinResult.style.color = 'red'
  }
}

const RedeemECash = async () => {
  let redeemInput = document.getElementById('redeemInput').value
  let redeemResult = document.getElementById('redeemResult')
  try {
    await wallet.mint.redeemEcash(redeemInput)
    redeemResult.innerHTML = 'Redeemed!'
    redeemResult.style.color = 'green'
  } catch (e) {
    redeemResult.innerHTML = `Error: ${e}`
    redeemResult.style.color = 'red'
  }
}

const sendLightning = async () => {
  let payInput = document.getElementById('payInput').value
  let payResult = document.getElementById('payResult')
  try {
    await wallet.lightning.payInvoice(payInput)
    payResult.innerHTML = 'Paid!'
    payResult.style.color = 'green'
  } catch (e) {
    payResult.innerHTML = `Error: ${e}`
    payResult.style.color = 'red'
  }
}

const GenerateLightningInvoice = async () => {
  let Invoiceamount = document.getElementById('Invoiceamount').value
  let description = document.getElementById('description').value
  let InvoiceBtn = document.getElementById('InvoiceBtn')
  let success = document.querySelector('.success')
  let error = document.querySelector('.error')
  InvoiceBtn.disabled = true
  InvoiceBtn.textContent = 'Generating'
  try {
    const response = await wallet.lightning.createInvoice(
      Number(Invoiceamount),
      description,
    )
    success.innerHTML = `
    <strong>Generated Invoice:</strong>
    <pre class="invoice-wrap">${response.invoice}</pre>
    <button onclick="navigator.clipboard.writeText('${response.invoice}')">
        Copy
    </button>
`
    InvoiceBtn.textContent = 'Generate Invoice'
    InvoiceBtn.disabled = false
  } catch (e) {
    InvoiceBtn.textContent = 'Generate Invoice'
    error.innerHTML = `Error: ${e}`
    InvoiceBtn.disabled = false
  }
}

// --- Onboarding Logic ---
const completeOnboarding = async () => {
  await wallet.open() // FIX 2: Await wallet.open()
  showScreen('wallet-ui')
  checkIsOpen()

  let joinInput = document.getElementById('join-input')
  joinInput.value = TESTNET_FEDERATION_CODE

  // Set up wallet UI event listeners
  document.querySelector('.JoinFederation').addEventListener('submit', (e) => {
    e.preventDefault()
    joinFederation(e)
  })
  document.querySelector('.RedeemForm').addEventListener('submit', (e) => {
    e.preventDefault()
    RedeemECash()
  })
  document.querySelector('.PayForm').addEventListener('submit', (e) => {
    e.preventDefault()
    sendLightning()
  })
  document.querySelector('.InvoiceForm').addEventListener('submit', (e) => {
    e.preventDefault()
    GenerateLightningInvoice()
  })
}

let generatedWords = []

const init = async () => {
  // Wipe DB logic to prevent stale mnemonic lockout
  if (localStorage.getItem('pendingWipe') === 'true') {
    localStorage.removeItem('pendingWipe')
    const { clearClientStorage } = await import('@fedimint/core')
    await clearClientStorage()
  }

  // Dynamically import wallet after DB wipe to prevent WASM locks
  const walletModule = await import('./wallet.js')
  director = walletModule.director
  walletReady = walletModule.walletReady

  try {
    wallet = await walletReady

    const hasMnemonic = await director.hasMnemonicSet()
    if (hasMnemonic) {
      await completeOnboarding()
    } else {
      showScreen('onboarding-screen')
      showOnboardingStep('onboarding-welcome')
    }
  } catch {
    showScreen('onboarding-screen')
    showOnboardingStep('onboarding-welcome')
  }

  // Generate button
  document
    .getElementById('btn-generate')
    .addEventListener('click', async () => {
      showError('')
      try {
        const words = await director.generateMnemonic()
        generatedWords = words
        const container = document.getElementById('mnemonic-words')
        container.innerHTML = words
          .map(
            (word, i) =>
              `<div class="mnemonic-word"><span class="word-index">${i + 1}.</span><span>${word}</span></div>`,
          )
          .join('')
        showOnboardingStep('onboarding-generate')
      } catch (e) {
        showError(
          e instanceof Error ? e.message : 'Failed to generate mnemonic',
        )
      }
    })

  // Backup confirm checkbox
  document.getElementById('backup-confirm').addEventListener('change', (e) => {
    document.getElementById('btn-confirm').disabled = !e.target.checked
  })

  // Confirm generated mnemonic
  document.getElementById('btn-confirm').addEventListener('click', async () => {
    showError('')
    try {
      await director.setMnemonic(generatedWords)
      await completeOnboarding()
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e)
      if (
        msg.includes('mnemonic already exists') ||
        msg.includes('already set')
      ) {
        try {
          const existing = await director.getMnemonic()
          if (existing.join(' ') === generatedWords.join(' ')) {
            await completeOnboarding()
          } else {
            showError(
              'CRITICAL: Stored mnemonic mismatch. Please clear site data and restart.',
            )
          }
        } catch (verifyErr) {
          showError('Could not verify stored mnemonic.')
        }
      } else {
        showError(msg)
      }
    }
  })

  // Show restore
  document.getElementById('btn-restore-show').addEventListener('click', () => {
    showOnboardingStep('onboarding-restore')
  })

  // Restore form
  document
    .getElementById('restore-form')
    .addEventListener('submit', async (e) => {
      e.preventDefault()
      showError('')
      const input = document.getElementById('restore-input').value.trim()
      if (!input) return

      const words = input.split(/\s+/)
      if (words.length !== 12 && words.length !== 24) {
        showError('Mnemonic must be exactly 12 or 24 words')
        return
      }

      try {
        await director.setMnemonic(words)
        await completeOnboarding()
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e)
        if (
          msg.includes('mnemonic already exists') ||
          msg.includes('already set')
        ) {
          try {
            const existing = await director.getMnemonic()
            if (existing.join(' ') === words.join(' ')) {
              await completeOnboarding()
              return
            }
          } catch (verifyErr) {
            console.error('Failed to verify existing mnemonic', verifyErr)
          }
        }
        showError(msg || 'Invalid mnemonic phrase')
      }
    })

  // Back buttons (FIX 1: Wipe DB if generated)
  document.getElementById('btn-back-generate').addEventListener('click', () => {
    if (
      window.confirm(
        'Going back requires a full app reload to clear the generated key. Proceed?',
      )
    ) {
      handleWipe(true)
    }
  })

  document.getElementById('btn-back-restore').addEventListener('click', () => {
    showOnboardingStep('onboarding-welcome')
  })
}

// Expose to window for inline onclicks in index.html (like checkIsOpen)
window.checkIsOpen = checkIsOpen

// Run initialization
init()
