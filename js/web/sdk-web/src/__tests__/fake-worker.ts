import type { WorkerLike } from '../client'
import type { Request, Response } from '../protocol'

/** A worker that records what it is sent and replies only when a test tells it to. */
export class FakeWorker implements WorkerLike {
  sent: Request[] = []
  private listeners = new Map<string, ((event: never) => void)[]>()
  terminated = false
  postMessage(message: unknown) {
    this.sent.push(message as Request)
  }
  addEventListener(type: string, listener: (event: never) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), listener])
  }
  terminate() {
    this.terminated = true
  }
  reply(message: Response) {
    for (const l of this.listeners.get('message') ?? [])
      (l as (e: { data: Response }) => void)({ data: message })
  }
  crash(message: string) {
    for (const l of this.listeners.get('error') ?? [])
      (l as (e: { message: string }) => void)({ message })
  }
}
