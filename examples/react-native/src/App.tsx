import React from 'react'
import { SafeAreaView, StatusBar } from 'react-native'
import { NavigationContainer, DefaultTheme } from '@react-navigation/native'
import { createBottomTabNavigator } from '@react-navigation/bottom-tabs'
import { SafeAreaProvider } from 'react-native-safe-area-context'
import { Wallet, ArrowDownToLine, ArrowUpFromLine, History, Settings } from 'lucide-react-native'
import s from './styles'

import { WalletScreen } from './screens/WalletScreen'
import { ReceiveScreen } from './screens/ReceiveScreen'
import { SendScreen } from './screens/SendScreen'
import { HistoryScreen } from './screens/HistoryScreen'
import { SettingsScreen } from './screens/SettingsScreen'

const Tab = createBottomTabNavigator()

// A bright, soft Claymorphism theme
const ClayTheme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    background: '#E0E5EC',
    card: '#E0E5EC',
    text: '#2d3748',
    border: '#d1d8e0',
    primary: '#4fd1c5',
  },
}

const App = () => {
  return (
    <SafeAreaProvider>
      <SafeAreaView style={s.safeArea}>
        <StatusBar barStyle="dark-content" backgroundColor="#E0E5EC" />
        <NavigationContainer theme={ClayTheme}>
          <Tab.Navigator
            screenOptions={{
              headerShown: false,
              tabBarStyle: {
                backgroundColor: '#ebf0f5',
                borderTopColor: '#d1d8e0',
                borderTopWidth: 1,
                elevation: 0,
                // Soft shadow for iOS indicating a clay layer
                shadowColor: '#a3b1c6',
                shadowOffset: { width: 0, height: -4 },
                shadowOpacity: 0.15,
                shadowRadius: 10,
              },
              tabBarActiveTintColor: '#4fd1c5',
              tabBarInactiveTintColor: '#a0aec0',
              tabBarLabelStyle: { fontWeight: '600' }
            }}
          >
            <Tab.Screen name="Wallet" component={WalletScreen} options={{ tabBarIcon: ({color, size}) => <Wallet color={color} size={size} /> }} />
            <Tab.Screen name="Receive" component={ReceiveScreen} options={{ tabBarIcon: ({color, size}) => <ArrowDownToLine color={color} size={size} /> }} />
            <Tab.Screen name="Send" component={SendScreen} options={{ tabBarIcon: ({color, size}) => <ArrowUpFromLine color={color} size={size} /> }} />
            <Tab.Screen name="History" component={HistoryScreen} options={{ tabBarIcon: ({color, size}) => <History color={color} size={size} /> }} />
            <Tab.Screen name="Settings" component={SettingsScreen} options={{ tabBarIcon: ({color, size}) => <Settings color={color} size={size} /> }} />
          </Tab.Navigator>
        </NavigationContainer>
      </SafeAreaView>
    </SafeAreaProvider>
  )
}

export default App