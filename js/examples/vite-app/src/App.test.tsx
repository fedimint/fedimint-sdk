import {
  ActivityStatus,
  Network,
  OperationKind,
  openSdk,
  type SdkSession,
} from '@fedimint/web-sdk'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, beforeEach, describe, expect, test, vi } from 'vitest'
import App from './App'

// The page runs against fakes of the SDK's objects, which reply when the test lets them: what
// matters here is what a reply that comes back late does to the page.
vi.mock('@fedimint/web-sdk', async (importOriginal) => ({
  ...(await importOriginal<typeof import('@fedimint/web-sdk')>()),
  openSdk: vi.fn(),
}))

Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true })

/** A reply held until the test sends it. */
function later<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((r) => (resolve = r))
  return { promise, resolve }
}

const never = () => new Promise<never>(() => {})

const operation = {
  state: async () => ({ tag: 'Created' }),
  updates: async () => ({ next: never }),
}

/** A quote of any of the three kinds, labelled with the input it was made for. */
const quote = (label: string) => ({
  label,
  amount: async () => 50_000n,
  invoiceAmount: async () => 50_000_000n,
  requestedAmount: async () => 10_000n,
  notesValue: async () => 10_000n,
  fee: async () => 1_000n,
  total: async () => 51_000n,
  route: async () => ({ tag: 'Direct' }),
  expiresAt: async () => 0n,
})

type Quote = ReturnType<typeof quote>

function fakeFederation(name: string) {
  const id = `fed-${name.toLowerCase()}`
  const onchain = {
    receive: vi.fn(async (): Promise<string> => `address of ${name}`),
    deposits: vi.fn(async () => ({ next: never })),
    quote: vi.fn(async (input: unknown) => quote(String(input))),
    send: vi.fn(async (_quote: Quote) => operation),
  }
  const lightning = {
    receive: vi.fn(async () => ({ invoice: `invoice of ${name}`, operation })),
    quote: vi.fn(async (input: unknown) => quote(String(input))),
    send: vi.fn(async (_quote: Quote) => operation),
  }
  const ecash = {
    quote: vi.fn(async (input: unknown) => quote(String(input))),
    send: vi.fn(async (_quote: Quote) => ({
      notes: { display: async () => 'notes' },
      operation,
    })),
  }
  return {
    id: async () => id,
    name: vi.fn(async (): Promise<string | undefined> => name),
    network: async () => Network.Regtest,
    capabilities: async () => ({ ecash: true, lightning: true, onchain: true }),
    inviteCode: async () => ({ display: async () => `invite of ${name}` }),
    balance: async () => 0n,
    balanceUpdates: async () => ({ next: never }),
    activity: vi.fn(async () => ({
      items: [] as unknown[],
      next: undefined as string | undefined,
    })),
    onchain: async () => onchain,
    lightning: async () => lightning,
    ecash: async () => ecash,
    fake: { id, name, onchain, lightning, ecash },
  }
}

type FakeFederation = ReturnType<typeof fakeFederation>

function fakeSession(federations: FakeFederation[]) {
  const byId = (id: string) => federations.find((f) => f.fake.id === id)
  return {
    sdk: {
      federations: async () => federations.slice(0, 1),
      storedFederations: async () =>
        federations.map(({ fake }) => ({
          id: fake.id,
          name: fake.name,
          network: Network.Regtest,
          status: { tag: 'Running' },
        })),
      federationStatusUpdates: async () => ({ next: never }),
      federation: async (id: string) => byId(id),
      recoveryStatus: async () => undefined,
      closeFederation: vi.fn(async (_id: string) => {}),
      forgetFederation: vi.fn(async (_id: string) => {}),
    },
    close: async () => {},
  }
}

let container: HTMLElement
let root: Root

beforeEach(() => {
  container = document.body.appendChild(document.createElement('div'))
  root = createRoot(container)
})

afterEach(async () => {
  await act(async () => root.unmount())
  container.remove()
  vi.unstubAllGlobals()
})

/** Lets every reply that is due arrive, and the page render what it set. */
const settle = () => act(() => new Promise((r) => setTimeout(r, 0)))

function section(title: string): HTMLElement {
  const heading = [...container.querySelectorAll('.section > h3')].find(
    (h) => h.textContent === title,
  )
  if (!heading) throw new Error(`no section "${title}"`)
  return heading.parentElement!
}

function button(within: HTMLElement, text: string): HTMLButtonElement {
  const found = [...within.querySelectorAll('button')].find(
    (b) => b.textContent === text,
  )
  if (!found) throw new Error(`no button "${text}"`)
  return found
}

async function click(target: HTMLElement) {
  await act(async () => target.click())
  await settle()
}

async function type(
  field: HTMLInputElement | HTMLTextAreaElement,
  value: string,
) {
  // React tracks the value it last rendered; the prototype's setter goes around that, so the
  // `input` event reads as an edit.
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(field),
    'value',
  )!.set!
  await act(async () => {
    setter.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

async function openWallet(session: ReturnType<typeof fakeSession>) {
  vi.mocked(openSdk).mockResolvedValue(session as unknown as SdkSession)
  await act(async () => root.render(<App />))
  await click(button(section('Wallet'), 'Open Wallet'))
}

/** Selects a federation in the list, by name. */
async function pick(name: string) {
  const list = container.querySelector<HTMLElement>('.section .wallet-grid')!
  const entry = [...list.querySelectorAll('button')].find((b) =>
    b.textContent?.includes(name),
  )
  await click(entry!)
}

describe('quotes', () => {
  test.each([
    {
      title: 'On-chain',
      field: '#send-to',
      send: 'Send',
      module: (f: FakeFederation) => f.fake.onchain,
    },
    {
      title: 'Pay Lightning',
      field: 'textarea',
      send: 'Pay',
      module: (f: FakeFederation) => f.fake.lightning,
    },
    {
      title: 'Send Ecash',
      field: '#ecash-amount',
      send: 'Send',
      module: (f: FakeFederation) => f.fake.ecash,
    },
  ])(
    '$title: a quote that comes back after an edit cannot be sent',
    async ({ title, field, send, module }) => {
      const alpha = fakeFederation('Alpha')
      await openWallet(fakeSession([alpha]))
      const form = section(title)
      const input = form.querySelector<HTMLInputElement>(field)!
      const { quote: quoteFor, send: sendFn } = module(alpha)

      const first = later<Quote>()
      quoteFor.mockReturnValueOnce(first.promise)
      await type(input, '1111')
      await click(button(form, 'Quote'))
      await type(input, '2222')
      first.resolve(quote('1111'))
      await settle()

      expect(button(form, send).disabled).toBe(true)
      expect(form.querySelector('.preview-result')).toBeNull()

      // A quote for what the form shows is the one that is sent.
      await click(button(form, 'Quote'))
      expect(button(form, send).disabled).toBe(false)
      await click(button(form, send))
      expect(sendFn).toHaveBeenCalledOnce()
      expect(sendFn).toHaveBeenCalledWith(
        expect.objectContaining({ label: '2222' }),
      )
    },
  )
})

describe('selecting another federation', () => {
  test('Forget names the federation it erases while its details load', async () => {
    const alpha = fakeFederation('Alpha')
    const beta = fakeFederation('Beta')
    const session = fakeSession([alpha, beta])
    await openWallet(session)
    expect(section('Wallet Status').textContent).toContain('Alpha')

    beta.name.mockReturnValueOnce(never())
    await pick('Beta')
    expect(section('Wallet Status').textContent).not.toContain('Alpha')

    const confirm = vi.fn((_message: string) => true)
    vi.stubGlobal('confirm', confirm)
    await click(button(section('Wallet Status'), 'Forget Federation'))
    expect(confirm).toHaveBeenCalledOnce()
    expect(confirm.mock.calls[0]![0]).toContain('fed-beta')
    expect(confirm.mock.calls[0]![0]).not.toContain('Alpha')
    expect(session.sdk.forgetFederation).toHaveBeenCalledWith('fed-beta')
  })

  test("the last federation's late replies stay out of the next one's sections", async () => {
    const alpha = fakeFederation('Alpha')
    const beta = fakeFederation('Beta')
    await openWallet(fakeSession([alpha, beta]))

    const address = later<string>()
    alpha.fake.onchain.receive.mockReturnValueOnce(address.promise)
    const invoice = later<{ invoice: string; operation: typeof operation }>()
    alpha.fake.lightning.receive.mockReturnValueOnce(invoice.promise)
    const page = later<{ items: unknown[]; next: string | undefined }>()
    alpha.activity.mockReturnValueOnce(page.promise)
    await click(button(section('On-chain'), 'New Deposit Address'))
    await click(
      button(section('Generate Lightning Invoice'), 'Generate Invoice'),
    )
    await click(button(section('Activity'), 'Load'))

    await pick('Beta')
    address.resolve('address of Alpha')
    invoice.resolve({ invoice: 'invoice of Alpha', operation })
    page.resolve({
      items: [
        {
          operationId: 'op',
          kind: OperationKind.LnReceive,
          time: 0n,
          status: ActivityStatus.Pending,
          isFinal: false,
        },
      ],
      next: undefined,
    })
    await settle()

    expect(section('Wallet Status').textContent).toContain('fed-beta')
    expect(container.textContent).not.toContain('of Alpha')
    expect(section('Activity').querySelector('table')).toBeNull()
    // Nothing watches Beta for deposits to Alpha's address.
    expect(beta.fake.onchain.deposits).not.toHaveBeenCalled()
  })

  test('a Forget that ends after another federation was picked leaves that one in use', async () => {
    const alpha = fakeFederation('Alpha')
    const beta = fakeFederation('Beta')
    const session = fakeSession([alpha, beta])
    await openWallet(session)

    const forgotten = later<void>()
    session.sdk.forgetFederation.mockReturnValueOnce(forgotten.promise)
    vi.stubGlobal('confirm', () => true)
    await click(button(section('Wallet Status'), 'Forget Federation'))
    await pick('Beta')
    forgotten.resolve()
    await settle()

    expect(session.sdk.forgetFederation).toHaveBeenCalledWith('fed-alpha')
    expect(section('Wallet Status').textContent).toContain('fed-beta')
  })
})
