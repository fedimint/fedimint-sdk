import { useCallback, useEffect, useState } from 'react'
import {
  ActivityIndicator,
  StatusBar,
  StyleSheet,
  Text,
  View,
} from 'react-native'
import { Stack } from 'expo-router'
import { ThemeProvider } from '@react-navigation/native'
import { model, useDemo } from '../src/model/DemoModel'
import { FedimintLogoOnDark } from '../src/components/Brand'
import { Button } from '../src/components/Buttons'
import { reason } from '../src/model/errors'
import { theme } from '../src/navigation/options'
import { Metrics, Palette, Typeface } from '../src/theme/tokens'

/**
 * Shown while a wallet left by an earlier launch is reopened, so the first frame is not a
 * Welcome screen offering to create a wallet that already exists.
 */
function LaunchScreen({
  failure,
  retry,
}: {
  failure?: string
  retry: () => void
}) {
  return (
    <View style={styles.launch}>
      <FedimintLogoOnDark height={32} />
      {failure === undefined ? (
        <ActivityIndicator color="#FFFFFF" />
      ) : (
        <View style={styles.launchFailure}>
          <Text
            selectable
            style={[
              Typeface.callout,
              { color: '#FFFFFF', textAlign: 'center' },
            ]}
          >
            {failure}
          </Text>
          <Button title="Try again" kind="primaryOnDark" onPress={retry} />
        </View>
      )}
    </View>
  )
}

/**
 * The root: picks the stage the app is in. Reopening a stored wallet, onboarding, the backup
 * step right after creation, or the wallet itself, each a route group behind a guard.
 */
export default function RootLayout() {
  const state = useDemo()
  /** Whether an earlier launch left a wallet behind, read once at launch. */
  const [reopening, setReopening] = useState<boolean>()
  /** Why the launch stopped before onboarding: a wallet could not be moved into place. */
  const [launchFailure, setLaunchFailure] = useState<string>()
  const attaching = model.isAttaching

  const launch = useCallback(() => {
    setLaunchFailure(undefined)
    // A launch's own screen, with its spinner, until it decides: never a Try again meanwhile.
    setReopening(undefined)
    model.start().then(
      (stored) => {
        setReopening(stored)
        if (!stored) return
        // Never a phrase: on Android, words from a Restore abandoned before the back button
        // closed the app outlive the remount, and would turn this reopen into a restore.
        model.setRestoreWords('')
        model.openWallet()
      },
      (error) => setLaunchFailure(reason(error)),
    )
  }, [])

  useEffect(() => {
    // On Android the back button remounts the root while the JS runtime and its open wallet
    // carry on: opening that wallet again would be refused as storage in use.
    if (model.getState().hasSdk) return
    launch()
  }, [launch])

  useEffect(() => {
    // A reopen that failed falls back to onboarding, where its reason is shown.
    if (!attaching && !state.hasSdk && reopening) setReopening(false)
  }, [attaching, state.hasSdk, reopening])

  const backingUp =
    state.hasSdk && state.walletOrigin === 'created' && !state.backedUp
  // A replace's old wallet is not back: nothing may be created over the spot it returns to.
  const leftOver = !state.hasSdk && state.replaceLeftOver !== undefined
  const onboarding = !state.hasSdk && reopening === false && !leftOver

  return (
    <ThemeProvider value={theme}>
      <StatusBar barStyle={state.hasSdk ? 'dark-content' : 'light-content'} />
      {leftOver || (!state.hasSdk && reopening !== false) ? (
        <LaunchScreen
          failure={state.replaceLeftOver ?? launchFailure}
          retry={launch}
        />
      ) : (
        <Stack screenOptions={{ headerShown: false, animation: 'none' }}>
          <Stack.Protected guard={onboarding}>
            <Stack.Screen name="(onboarding)" />
          </Stack.Protected>
          <Stack.Protected guard={backingUp}>
            <Stack.Screen name="(backup)" />
          </Stack.Protected>
          <Stack.Protected guard={state.hasSdk && !backingUp}>
            <Stack.Screen name="(main)" />
          </Stack.Protected>
        </Stack>
      )}
    </ThemeProvider>
  )
}

const styles = StyleSheet.create({
  launch: {
    flex: 1,
    backgroundColor: Palette.navy,
    alignItems: 'center',
    justifyContent: 'center',
    gap: Metrics.sectionGap,
  },
  launchFailure: {
    alignSelf: 'stretch',
    gap: Metrics.gap,
    paddingHorizontal: Metrics.screenPadding,
  },
})
