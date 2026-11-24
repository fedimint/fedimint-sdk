import { useEffect } from 'react'
import { Stack } from 'expo-router'
import { model } from '../../src/model/DemoModel'
import { stackOptions } from '../../src/navigation/options'

export const unstable_settings = { initialRouteName: 'backup' }

/** Back up, then Verify, straight after a wallet is created and before it is used. */
export default function BackupLayout() {
  // Hides the phrase on every way out: finishing, and on Android the back button, which closes
  // the app while the model, and so the words, stay in memory for its next launch.
  useEffect(() => () => model.hideSeed(), [])
  return (
    <Stack screenOptions={{ ...stackOptions, title: '' }}>
      <Stack.Screen name="backup" options={{ headerShown: false }} />
      <Stack.Screen name="verify" />
    </Stack>
  )
}
