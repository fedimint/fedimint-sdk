import { wallet } from './wallet.js'
import './style.css' // Load CSS

const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'

const checkIsOpen = () => {
  let walletResult = document.getElementById('walletResult')
  if (wallet.isOpen() == true) {
    walletResult.innerHTML = 'Yes'
    getBalance()
  } else {
    console.log('No')
    walletResult.innerHTML = 'No'
  }
}

// fetching balance
const getBalance = () => {
  let bal = document.getElementById('bal')
  console.log('bal', bal)
  wallet.balance.subscribeBalance((balance) => {
    console.log('the balance is ', balance)
    bal.innerText = balance
  })
}

let joinInput = document.getElementById('join-input')
joinInput.value = TESTNET_FEDERATION_CODE

// join federation
const joinFederation = async (event) => {
  event.preventDefault()
  console.log('Joining federation:', TESTNET_FEDERATION_CODE)
  let joinResult = document.getElementById('joinResult')
  try {
    const res = await wallet.joinFederation(TESTNET_FEDERATION_CODE)
    console.log('join federation res', res)
    joinResult.innerHTML = 'Joined!'
    joinResult.style.color = 'green'
  } catch (e) {
    console.log('Error joining federation', e)
    joinResult.innerHTML = `Error in joining federation ${e}`
    joinResult.style.color = 'red'
  }
}

const RedeemECash = async () => {
  let redeemInput = document.getElementById('redeemInput').value
  let redeemResult = document.getElementById('redeemResult')
  console.log('redeem input is ', redeemInput)
  try {
    const res = await wallet.mint.redeemEcash(redeemInput)
    console.log('redeem ecash res', res)
    redeemResult.innerHTML = 'Redeemed!'
    redeemResult.style.color = 'green'
  } catch (e) {
    console.log('Error redeeming ecash', e)
    redeemResult.innerHTML = `An error occured ${e}`
    redeemResult.style.color = 'red'
  }
}

const sendLightning = async () => {
  let payInput = document.getElementById('payInput').value
  let payResult = document.getElementById('payResult')
  try {
    await wallet.lightning.payInvoice(payInput)
    console.log('Paid!')
    payResult.innerHTML = 'Paid!'
    payResult.style.color = 'green'
  } catch (e) {
    console.log('Error paying lightning', e)
    payResult.innerHTML = `Error paying lightning ${e}`
    payResult.style.color = 'red'
  }
}

// generate lightning invoice
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
    console.log('response invoice ', response.invoice)
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
    console.error('Error generating Lightning invoice', e)
    InvoiceBtn.textContent = 'Generate Invoice'
    error.innerHTML = `An error occured ${e}`
    InvoiceBtn.disabled = false
  }
}

document.addEventListener('DOMContentLoaded', async () => {
  await checkIsOpen()

  document.querySelector('.JoinFederation').addEventListener('submit', (e) => {
    e.preventDefault() // Prevent form submission
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
})

window.checkIsOpen = checkIsOpen
