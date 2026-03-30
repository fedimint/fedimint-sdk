import React from 'react'
import { SafeAreaView, StatusBar, Text } from 'react-native'
import { NavigationContainer, DefaultTheme } from '@react-navigation/native'
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs'
import { SafeAreaProvider } from 'react-native-safe-area-context'
import s from './styles'

import { WalletScreen } from './screens/WalletScreen'
import { ReceiveScreen } from './screens/ReceiveScreen'
import { SendScreen } from './screens/SendScreen'
import { HistoryScreen } from './screens/HistoryScreen'
import { SettingsScreen } from './screens/SettingsScreen'

const Tab = createBottomTabNavigator()

const DarkTheme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    background: '#1a1a2e',
    card: '#0f3460',
    text: '#ffffff',
    border: '#1a4a7a',
    primary: '#4CAF50',
  },
}

const App = () => {
  return (
    <SafeAreaProvider>
      <SafeAreaView style={s.safeArea}>
        <StatusBar barStyle="light-content" backgroundColor="#1a1a2e" />
        <NavigationContainer theme={DarkTheme}>
          <Tab.Navigator
            screenOptions={{
              headerShown: false,
              tabBarStyle: {
                backgroundColor: '#0f3460',
                borderTopColor: '#1a4a7a',
              },
              tabBarActiveTintColor: '#60a5fa',
              tabBarInactiveTintColor: '#888',
            }}
          >
            <Tab.Screen name="Wallet" component={WalletScreen} options={{ tabBarIcon: () => <Text>👛</Text> }} />
            <Tab.Screen name="Receive" component={ReceiveScreen} options={{ tabBarIcon: () => <Text>⬇️</Text> }} />
            <Tab.Screen name="Send" component={SendScreen} options={{ tabBarIcon: () => <Text>⬆️</Text> }} />
            <Tab.Screen name="History" component={HistoryScreen} options={{ tabBarIcon: () => <Text>📜</Text> }} />
            <Tab.Screen name="Settings" component={SettingsScreen} options={{ tabBarIcon: () => <Text>⚙️</Text> }} />
          </Tab.Navigator>
        </NavigationContainer>
      </SafeAreaView>
    </SafeAreaProvider>
  )
}

export default App