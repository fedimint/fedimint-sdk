import { WasmWorkerTransport } from './WasmWorkerTransport'
import { clearClientStorage } from './storage'

export { WasmWorkerTransport, clearClientStorage }

export const createWasmWorker = () =>
  new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })

export const createWasmWorkerTransport = () => new WasmWorkerTransport()
