// The web SDK harness the cross-platform suite in js/e2e drives over
// js/e2e/PROTOCOL.md: the console wired to a `Harness` over `openSdk`.
// `?storage=<name>` picks the origin-private store; a fresh browser context is
// a fresh install.
import { openSdk } from '@fedimint/web-sdk'

import { Harness } from './dispatch'

const element = <T extends HTMLElement>(id: string) =>
  document.getElementById(id) as T

const request = element<HTMLTextAreaElement>('request')
const response = element<HTMLPreElement>('response')
const storage = new URLSearchParams(location.search).get('storage') ?? 'harness'

const harness = new Harness(openSdk, storage)
harness.onResponse = (text) => {
  response.textContent = text
}
response.textContent = harness.response

element<HTMLButtonElement>('run').addEventListener('click', () => {
  const raw = request.value
  // An empty field is a repeated click on a request already taken.
  if (raw.trim() === '') return
  request.value = ''
  harness.submit(raw)
})
