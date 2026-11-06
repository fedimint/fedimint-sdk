import { METHODS } from '../protocol/methods'

import { TestDef } from './types'

/** Every character class the wire has to carry intact: quotes, backslashes,
 * braces, a newline and non-ASCII (sent escaped), at a realistic length. */
const ECHO_PAYLOAD =
  `"quoted" back\\slash {braces} [brackets] line\nbreak café ₿ `
    .repeat(24)
    .slice(0, 1500)

/**
 * The harness speaks PROTOCOL.md: it names this platform, serves every method
 * (or the target says why not), carries text both ways intact, and answers a
 * bad call with the harness codes rather than crashing. Runs first: every
 * other test depends on this channel.
 */
export const consoleTest: TestDef = {
  name: 'console',
  prerequisites: [],
  produces: [],
  requires: ['harness.describe', 'harness.echo', 'harness.state'],

  async run({ h, target, log }) {
    const described = await h.call('harness.describe', {})
    if (described.platform !== target.name) {
      throw new Error(
        `The harness says it is "${described.platform}", but the target is "${target.name}"`,
      )
    }
    const missing = METHODS.filter(
      (m) => !described.methods.includes(m) && !(m in target.knownUnsupported),
    )
    if (missing.length > 0) {
      throw new Error(`The harness does not serve: ${missing.join(', ')}`)
    }
    const extra = described.methods.filter(
      (m) => !(METHODS as readonly string[]).includes(m),
    )
    if (extra.length > 0) log(`The harness also serves: ${extra.join(', ')}`)
    log(
      `${described.platform} harness, SDK ${described.sdkVersion ?? 'unknown'}`,
    )

    const { payload } = await h.call('harness.echo', { payload: ECHO_PAYLOAD })
    if (payload !== ECHO_PAYLOAD) {
      throw new Error(
        `The echo came back altered: sent ${JSON.stringify(ECHO_PAYLOAD.slice(0, 80))}…, ` +
          `got ${JSON.stringify(payload.slice(0, 80))}…`,
      )
    }

    const unknown = await h.tryUnchecked('no.such.method', {})
    if (unknown.ok || unknown.error.code !== 'HARNESS_UNKNOWN_METHOD') {
      throw new Error(
        `An unknown method should answer HARNESS_UNKNOWN_METHOD: ${JSON.stringify(unknown)}`,
      )
    }
    const badParams = await h.tryUnchecked('harness.echo', { payload: 5 })
    if (badParams.ok || badParams.error.code !== 'HARNESS_BAD_PARAMS') {
      throw new Error(
        `A mistyped param should answer HARNESS_BAD_PARAMS: ${JSON.stringify(badParams)}`,
      )
    }

    const state = await h.call('harness.state', {})
    if (state.sdkOpen) {
      throw new Error('A fresh install should not have the SDK open yet')
    }
  },
}
