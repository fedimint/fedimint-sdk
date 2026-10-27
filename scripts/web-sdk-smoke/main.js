/**
 * The module the smoke test's page runs: what an application does first with the SDK. It opens a
 * session over a storage name nobody else uses, reads the seed the session created and checks
 * that no federation is joined. None of that needs a federation or the network.
 *
 * The outcome is left in `window.__fedimintSmoke`, where scripts/web-sdk-smoke/driver.mjs reads
 * it:
 *
 *   { status: 'ok', words: 12 }
 *   { status: 'fail', error: '<text>' }
 *
 * Before the check ends, the property is absent.
 */
import { openSdk } from '@fedimint/web-sdk'

async function check() {
  const storage = `web-sdk-smoke-${Date.now().toString(36)}-${Math.random()
    .toString(36)
    .slice(2, 8)}`
  const session = await openSdk({ storage })
  try {
    const words = await (await session.sdk.exportMnemonic()).words()
    if (words.length !== 12) {
      throw new Error(`the mnemonic has ${words.length} words, expected 12`)
    }
    const federations = await session.sdk.federations()
    if (federations.length !== 0) {
      throw new Error(`${federations.length} federations on a fresh storage`)
    }
    return { status: 'ok', words: words.length }
  } finally {
    await session.close()
  }
}

check().then(
  (result) => {
    window.__fedimintSmoke = result
  },
  (error) => {
    window.__fedimintSmoke = {
      status: 'fail',
      error:
        error instanceof Error
          ? `${error.name}: ${error.message}`
          : String(error),
    }
  },
)
