import { WasmWorkerTransport } from './WasmWorkerTransport'

export { WasmWorkerTransport }

export const createWasmWorker = () =>
  new Worker(new URL('./worker.js', import.meta.url), { type: 'module' })

export const createWasmWorkerTransport = () => new WasmWorkerTransport()
