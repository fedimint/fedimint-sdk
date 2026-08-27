/**
 * Test stand-in for the ubrn-generated `@fedimint/react-native-bindings`
 * module, injected via a Vitest alias (see the react-native project in
 * vitest.config.ts) so the transport can be exercised without the generated
 * bindings or a compiled native library.
 */
export type RpcCallback = { onResponse: (responseJson: string) => void }

export class RpcHandler {
  static instances: RpcHandler[] = []

  requests: { request: Record<string, unknown>; callback: RpcCallback }[] = []
  destroyed = false
  throwOnRpc: Error | null = null

  constructor(public dbPath: string) {
    RpcHandler.instances.push(this)
  }

  rpc(requestJson: string, callback: RpcCallback): void {
    if (this.throwOnRpc) {
      throw this.throwOnRpc
    }
    this.requests.push({ request: JSON.parse(requestJson), callback })
  }

  uniffiDestroy(): void {
    this.destroyed = true
  }

  /** Delivers `response` to the callback of the request at `index`. */
  respond(index: number, response: unknown): void {
    this.requests[index]!.callback.onResponse(JSON.stringify(response))
  }
}
