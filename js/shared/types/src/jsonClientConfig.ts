import { JSONValue } from './utils'

// TODO: add types for the various module configs
export type JsonClientConfig = {
  global: GlobalClientConfig
  modules: Record<string, ModuleConfig>
}

export type GlobalClientConfig = {
  api_endpoints: Record<string, Peer>
  broadcast_public_keys: Record<string, string> | null
  consensus_version: CoreConsensusVersion
  meta: Record<string, string> // Additional config, includes "federation_name" if set
}

export type Peer = {
  url: string // SafeUrl, e.g. "wss://..."
  name: string // The peer's name
}

export type CoreConsensusVersion = {
  major: number // u32
  minor: number // u32
}

export type ModuleConfig = {
  kind: string // ModuleKind, e.g. "ln", "mint", "wallet"
  [key: string]: JSONValue // Module-specific configuration fields
}
