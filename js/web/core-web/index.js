const warnedKey = '__FEDIMINT_CORE_WEB_SHIM__'

if (typeof globalThis !== 'undefined') {
  const flagHost = globalThis
  if (!flagHost[warnedKey]) {
    flagHost[warnedKey] = true
    if (typeof console !== 'undefined' && typeof console.warn === 'function') {
      console.warn(
        '[fedimint] `@fedimint/core-web` has been renamed to `@fedimint/core`. Please update your dependencies to use the new package.',
      )
    }
  }
}

export * from '@fedimint/core'
