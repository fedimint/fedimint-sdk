import React, { useEffect } from 'react'
import { View, Text, StyleSheet } from 'react-native'
import { Ionicons } from '@expo/vector-icons'

export default function SplashScreen({ onFinish }: { onFinish: () => void }) {
  useEffect(() => {
    const timer = setTimeout(onFinish, 1500)
    return () => clearTimeout(timer)
  }, [onFinish])

  return (
    <View style={styles.container}>
      <Ionicons name="wallet" size={72} color="#60a5fa" />
      <Text style={styles.title}>Fedimint Wallet</Text>
      <Text style={styles.subtitle}>Powered by Fedimint SDK</Text>
    </View>
  )
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#1a1a2e',
    alignItems: 'center',
    justifyContent: 'center',
  },
  title: {
    color: '#ffffff',
    fontSize: 28,
    fontWeight: '700',
    marginTop: 16,
  },
  subtitle: {
    color: '#888',
    fontSize: 14,
    marginTop: 8,
  },
})
