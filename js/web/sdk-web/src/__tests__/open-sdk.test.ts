import { describe, expect, it } from 'vitest'
import { SdkError } from '../errors'
import { openSdk } from '../index'
import { REF_KEY } from '../protocol'
import { FakeWorker } from './fake-worker'

/** Opens a session over `worker`, answering the one call `openSdk` makes to create the SDK. */
async function open(worker: FakeWorker) {
  const opening = openSdk({ storage: 'store', createWorker: () => worker })
  worker.reply({ kind: 'ok', id: 1, value: { [REF_KEY]: 1, type: 'Sdk' } })
  return opening
}

/** The ids of the shutdown requests `worker` has been sent, in order. */
function shutdowns(worker: FakeWorker): number[] {
  return worker.sent.flatMap((request) =>
    request.kind === 'call' && request.method === 'shutdown'
      ? [request.id]
      : [],
  )
}

describe('openSdk', () => {
  it('makes a later close wait for the first rather than resolving early', async () => {
    // A caller `close` has resolved for is told the session is gone, so the worker has to be
    // gone by then too, whichever call it made.
    const worker = new FakeWorker()
    const session = await open(worker)

    const first = session.close()
    const second = session.close()
    let terminatedWhenSecondResolved: boolean | undefined
    const watch = second.then(() => {
      terminatedWhenSecondResolved = worker.terminated
    })
    // Lets a close that does not wait resolve before the shutdown is answered.
    await new Promise((resolve) => setTimeout(resolve, 0))

    const [id] = shutdowns(worker)
    worker.reply({ kind: 'ok', id: id!, value: undefined })
    await Promise.all([first, second, watch])
    expect(terminatedWhenSecondResolved).toBe(true)
    expect(shutdowns(worker)).toHaveLength(1)
  })

  it('terminates the worker even when the shutdown fails, and reports it to every caller', async () => {
    const worker = new FakeWorker()
    const session = await open(worker)

    const first = session.close()
    const second = session.close()
    const [id] = shutdowns(worker)
    worker.reply({
      kind: 'err',
      id: id!,
      error: {
        sdk: true,
        code: 'Internal' as never,
        reason: 'the flush failed',
      },
    })

    await expect(first).rejects.toBeInstanceOf(SdkError)
    await expect(second).rejects.toBeInstanceOf(SdkError)
    expect(worker.terminated).toBe(true)
    expect(shutdowns(worker)).toHaveLength(1)
  })
})
