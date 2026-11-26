import { useEffect, useLayoutEffect } from 'react'
import { useNavigation, useRouter } from 'expo-router'
import { model, useDemo } from '../../src/model/DemoModel'
import { closeButton, noCloseButton } from '../../src/navigation/options'
import { RestoreScreen } from '../../src/screens/onboarding/RestoreScreen'

/**
 * Replace wallet, from the wallet's menu. It stays up while a replace runs, against Close, a swipe
 * and Android's back button alike: its outcome, a failure included, shows only here.
 */
export default function ReplaceWallet() {
  const router = useRouter()
  const navigation = useNavigation()
  const replacing = useDemo().replacing
  useLayoutEffect(() => {
    navigation.setOptions({
      gestureEnabled: !replacing,
      ...(replacing ? noCloseButton : closeButton(() => router.back())),
    })
  }, [navigation, router, replacing])
  useEffect(
    () =>
      navigation.addListener('beforeRemove', (event) => {
        // Read live: the sheet closes itself the moment a replace ends.
        if (model.getState().replacing) event.preventDefault()
      }),
    [navigation],
  )
  return <RestoreScreen replaced={() => router.back()} />
}
