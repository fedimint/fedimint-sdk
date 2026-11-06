/**
 * Drives one harness console: the `request` field, the `run` button and the
 * `response` view every platform's harness has (PROTOCOL.md). One per
 * automation technology; a target picks the backend and how it finds the
 * three elements.
 */
export interface ConsoleBackend {
  /** Resolves once the console is on screen and answering. */
  waitReady(timeoutMs: number): Promise<void>
  /** Writes into the request field, which the harness leaves empty. */
  writeRequest(json: string): Promise<void>
  /** Reads the request field back, for backends whose typing can drop or
   * alter characters (`verifyInput`). */
  readRequest(): Promise<string>
  run(): Promise<void>
  readResponse(): Promise<string>
  /** Saves a screenshot and the UI tree as `<label>.png` / `<label>.xml`. */
  captureUi(dir: string, label: string): Promise<void>
  readonly verifyInput: boolean
}
