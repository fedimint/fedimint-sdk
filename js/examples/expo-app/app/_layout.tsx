import React, { useCallback, useState } from 'react'
import { Stack } from 'expo-router'
import { StatusBar } from 'expo-status-bar'
import { director, walletReady } from '../src/wallet'
import SplashScreen from '../src/screens/SplashScreen'
import OnboardingScreen from '../src/screens/OnboardingScreen'

type AppPhase = 'splash' | 'checking' | 'onboarding' | 'ready'

export default function RootLayout() {
  const [phase, setPhase] = useState<AppPhase>('splash')

  const checkMnemonic = useCallback(async () => {
    setPhase('checking')
    try {
      await walletReady
      const has = await director.hasMnemonicSet()
      setPhase(has ? 'ready' : 'onboarding')
    } catch {
      // If check fails (e.g. first launch), show onboarding
      setPhase('onboarding')
    }
  }, [])

  const onSplashFinish = useCallback(() => {
    checkMnemonic()
  }, [checkMnemonic])

  const onOnboardingComplete = useCallback(() => {
    setPhase('ready')
  }, [])

  if (phase === 'splash' || phase === 'checking') {
    return (
      <>
        <StatusBar style="light" backgroundColor="#1a1a2e" />
        <SplashScreen onFinish={onSplashFinish} />
      </>
    )
  }

  if (phase === 'onboarding') {
    return (
      <>
        <StatusBar style="light" backgroundColor="#1a1a2e" />
        <OnboardingScreen onComplete={onOnboardingComplete} />
      </>
    )
  }

  return (
    <>
      <StatusBar style="light" backgroundColor="#1a1a2e" />
      <Stack
        screenOptions={{
          headerStyle: { backgroundColor: '#1a1a2e' },
          headerTintColor: '#ffffff',
          headerTitleStyle: { fontWeight: '700' },
          contentStyle: { backgroundColor: '#1a1a2e' },
        }}
      >
        <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
        <Stack.Screen
          name="send"
          options={{ title: 'Send', presentation: 'card' }}
        />
        <Stack.Screen
          name="receive"
          options={{ title: 'Receive', presentation: 'card' }}
        />
      </Stack>
    </>
  )
}
