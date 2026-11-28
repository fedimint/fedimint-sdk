// devimint's faucet: it hands out its federation's invite code, and pays and issues invoices
// through its gateway, so the page can move money without a second wallet.
const DEFAULT_URL = 'http://localhost:15243'

export const initialFaucetUrl = () =>
  new URLSearchParams(location.search).get('faucet') ?? DEFAULT_URL

// Only a faucet on this machine is used, so a link to the page cannot point it at another
// service that hands out invoices to pay.
function onThisMachine(base: string) {
  const { protocol, hostname, origin } = new URL(base)
  const loopback =
    hostname === 'localhost' ||
    hostname === '[::1]' ||
    /^127(\.\d{1,3}){3}$/.test(hostname)
  if (!loopback || (protocol !== 'http:' && protocol !== 'https:'))
    throw new Error(`the faucet must be on this machine, not ${origin}`)
}

async function call(
  base: string,
  path: string,
  body?: string,
): Promise<string> {
  onThisMachine(base)
  const response = await fetch(
    `${base.replace(/\/+$/, '')}${path}`,
    body === undefined ? undefined : { method: 'POST', body },
  )
  const text = await response.text()
  if (!response.ok)
    throw new Error(`faucet ${path} answered ${response.status}: ${text}`)
  return text
}

export const faucet = {
  inviteCode: (base: string) => call(base, '/connect-string'),
  pay: (base: string, invoice: string) => call(base, '/pay', invoice),
  invoice: (base: string, amount: bigint) =>
    call(base, '/invoice', amount.toString()),
}
