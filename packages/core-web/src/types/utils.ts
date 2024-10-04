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

type JSONValue =
  | string
  | number
  | boolean
  | null
  | { [key: string]: JSONValue }
  | JSONValue[]

type JSONObject = Record<string, JSONValue>

export { Alias, Resolve, Duration, MSats, Sats, JSONValue, JSONObject }
