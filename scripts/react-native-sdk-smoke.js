/**
 * The entry file the release bundles into the example app instead of its index.js: the same
 * app, plus a self-check of the installed packages that reports to the device log.
 *
 * scripts/react-native-sdk-example.sh copies this file next to the example's index.js as
 * index.smoke.js, the Android build picks it up through ENTRY_FILE, and
 * scripts/react-native-sdk-smoke.sh waits for the one line it logs.
 *
 * The check does what an app does first: it loads the native library (importing the package
 * does that, and compares the library's API checksums with the bindings'), generates a
 * mnemonic, opens the SDK over a fresh directory in the app's private storage and reads the
 * seed it created back. None of that needs a federation or the network.
 *
 * A failure is logged as soon as it happens. Success is only logged once the example's own
 * root component has mounted, so that it also says the app starts with these packages.
 */
import React, { useEffect } from 'react'
import { AppRegistry } from 'react-native'
import { name as appName } from './app.json'

const MARKER = 'FEDIMINT_RN_SMOKE'

const report = (result) => console.log(`${MARKER} ${JSON.stringify(result)}`)

async function selfCheck() {
  // Required here and not imported at the top: loading the package is the first thing being
  // checked, and a failure there has to reach `report` below.
  const RNFS = require('react-native-fs')
  const { Mnemonic, openSdk } = require('@fedimint/react-native')

  const generated = Mnemonic.generate().words().length
  const dataDir = `${RNFS.DocumentDirectoryPath}/fedimint-smoke`
  if (await RNFS.exists(dataDir)) await RNFS.unlink(dataDir)
  const session = await openSdk({ dataDir })
  let stored
  try {
    stored = session.sdk.exportMnemonic().words().length
  } finally {
    await session.close()
  }
  await RNFS.unlink(dataDir)

  // Only the word counts are reported: the words themselves are a seed.
  for (const count of [generated, stored]) {
    if (count !== 12 && count !== 24) {
      throw new Error(`a mnemonic of ${count} words`)
    }
  }
  return { status: 'ok', generated, stored }
}

const checked = selfCheck()
checked.catch((error) => report({ status: 'fail', error: String(error) }))

function Smoke() {
  useEffect(() => {
    checked.then(report, () => {})
  }, [])
  const App = require('./src/App').default
  return <App />
}

AppRegistry.registerComponent(appName, () => Smoke)
