import { Stack } from 'expo-router'
import { model } from '../../src/model/DemoModel'
import { stackOptions } from '../../src/navigation/options'

export const unstable_settings = { initialRouteName: 'index' }

/** Everything before a wallet is open: Welcome, and Restore pushed from it. */
export default function OnboardingLayout() {
  return (
    <Stack
      screenOptions={stackOptions}
      screenListeners={({ route }) => ({
        // Restore's failure stays with Restore. Cleared on leaving it rather than when Welcome
        // shows, since Welcome may be showing why a stored wallet failed to reopen.
        beforeRemove: () => {
          if (route.name === 'restore' && !model.isAttaching) {
            model.clearFailure('wallet')
          }
        },
      })}
    >
      <Stack.Screen name="index" options={{ headerShown: false }} />
      <Stack.Screen name="restore" options={{ title: 'Restore' }} />
    </Stack>
  )
}
