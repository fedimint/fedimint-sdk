import { Stack, useNavigation } from 'expo-router'
import { closeButton, stackOptions } from '../../../src/navigation/options'
import { CloseSheet } from '../../../src/navigation/sheet'

export const unstable_settings = { initialRouteName: 'index' }

/** 14 Send, 06 Review and the result, in their own stack inside the Send sheet. */
export default function SendLayout() {
  // A layout's navigation is the sheet's place in the stack it was presented over.
  const navigation = useNavigation()
  const close = navigation.goBack
  return (
    <CloseSheet.Provider value={close}>
      <Stack screenOptions={stackOptions}>
        <Stack.Screen
          name="index"
          options={{ title: 'Send', ...closeButton(close) }}
        />
        <Stack.Screen name="review" options={{ title: 'Review payment' }} />
      </Stack>
    </CloseSheet.Provider>
  )
}
