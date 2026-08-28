import type { JSONValue } from '@fedimint/types'

type Alias<T> = T & {}
type Resolve<T> = T & unknown

type Seconds = Alias<number>
type Nanos = Alias<number>

type Duration = {
  nanos: Nanos
  secs: Seconds
}

type MSats = Alias<number>
type Sats = Alias<number>

type JSONObject = Record<string, JSONValue>

type Result<T, U = string> =
  | { success: true; data?: T }
  | { success: false; error: U }

export { Alias, Resolve, Duration, MSats, Sats, JSONValue, JSONObject, Result }
