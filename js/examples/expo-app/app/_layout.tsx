import React, { useCallback, useState } from 'react'
import { Alert, StyleSheet, Text, TouchableOpacity, View } from 'react-native'
import { Stack } from 'expo-router'
import { StatusBar } from 'expo-status-bar'
import { errorMessage, open, reset, walletExists } from '../src/sdk'
import SplashScreen from '../src/screens/SplashScreen'
import OnboardingScreen from '../src/screens/OnboardingScreen'
import s from '../src/styles'

type AppPhase = 'splash' | 'checking' | 'onboarding' | 'ready' | 'error'

export default function RootLayout() {
  const [phase, setPhase] = useState<AppPhase>('splash')
  const [openError, setOpenError] = useState('')

  const checkWallet = useCallback(async () => {
    setPhase('checking')
    try {
      const exists = await walletExists()
      if (!exists) {
        setPhase('onboarding')
        return
      }
      await open()
      setPhase('ready')
    } catch (error) {
      // A wallet directory exists but could not be opened (a storage error, most likely).
      // Restoring against it without clearing it first would just fail the same way, so
      // this stops short of onboarding and offers a retry or a full reset instead.
      setOpenError(errorMessage(error))
      setPhase('error')
    }
  }, [])

  const onSplashFinish = useCallback(() => {
    checkWallet()
  }, [checkWallet])

  const onOnboardingComplete = useCallback(() => {
    setPhase('ready')
  }, [])

  const confirmResetWallet = useCallback(() => {
    Alert.alert(
      'Reset wallet?',
      'This permanently deletes the wallet data stored on this device. You will need your ' +
        'recovery phrase to restore it afterwards.',
      [
        { text: 'Cancel', style: 'cancel' },
        {
          text: 'Reset wallet',
          style: 'destructive',
          onPress: async () => {
            try {
              await reset()
              setPhase('onboarding')
            } catch (error) {
              setOpenError(errorMessage(error))
              setPhase('error')
            }
          },
        },
      ],
    )
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

  if (phase === 'error') {
    return (
      <>
        <StatusBar style="light" backgroundColor="#1a1a2e" />
        <View style={[s.container, errorStyles.center]}>
          <Text style={s.header}>Could not open wallet</Text>
          <Text style={errorStyles.message}>{openError}</Text>
          <View style={errorStyles.buttonRow}>
            <TouchableOpacity style={s.actionBtn} onPress={checkWallet}>
              <Text style={s.actionBtnText}>Retry</Text>
            </TouchableOpacity>
            <TouchableOpacity style={s.actionBtn} onPress={confirmResetWallet}>
              <Text style={s.actionBtnText}>Reset wallet</Text>
            </TouchableOpacity>
          </View>
        </View>
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

const errorStyles = StyleSheet.create({
  center: {
    justifyContent: 'center',
    alignItems: 'center',
    padding: 24,
  },
  message: {
    color: '#b0b0b0',
    fontSize: 15,
    textAlign: 'center',
    marginTop: 12,
    marginBottom: 24,
  },
  buttonRow: {
    flexDirection: 'row',
    gap: 12,
    width: '100%',
  },
})
