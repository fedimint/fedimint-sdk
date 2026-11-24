import { Stack, useRouter } from 'expo-router'
import { closeButton, stackOptions } from '../../src/navigation/options'

export const unstable_settings = { initialRouteName: '(tabs)' }

/** The wallet once one is open: the design's three tabs, with Send and Receive as sheets. */
export default function MainLayout() {
  const router = useRouter()
  const close = () => router.back()
  return (
    <Stack screenOptions={stackOptions}>
      <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
      <Stack.Screen
        name="send"
        options={{ presentation: 'modal', headerShown: false }}
      />
      <Stack.Screen
        name="receive"
        options={{
          presentation: 'modal',
          title: 'Receive',
          ...closeButton(close),
        }}
      />
      <Stack.Screen
        name="recovery-phrase"
        options={{ presentation: 'modal', title: '', ...closeButton(close) }}
      />
    </Stack>
  )
}
