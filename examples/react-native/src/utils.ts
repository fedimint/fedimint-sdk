export const extractErrorMessage = (error: any): string => {
  if (error instanceof Error) return error.message
  if (typeof error === 'object' && error !== null) {
    if (error.error) return error.error
    if (error.message) return error.message
  }
  return 'Operation failed'
}

export const TESTNET_FEDERATION_CODE =
  'fed11qgqrgvnhwden5te0v9k8q6rp9ekh2arfdeukuet595cr2ttpd3jhq6rzve6zuer9wchxvetyd938gcewvdhk6tcqqysptkuvknc7erjgf4em3zfh90kffqf9srujn6q53d6r056e4apze5cw27h75'
